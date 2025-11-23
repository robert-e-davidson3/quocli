use crate::llm::client::{async_trait, LlmClient};
use crate::llm::prompt;
use crate::parser::{CommandOption, CommandSpec, DangerLevel, HelpDocumentation};
use crate::QuocliError;
use futures::future;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{self, Write};

/// Maximum concurrent API requests to avoid rate limiting
const MAX_CONCURRENT_REQUESTS: usize = 10;

/// Number of options to request per API call when batching
const OPTIONS_PER_BATCH: usize = 16;

pub struct AnthropicClient {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl AnthropicClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            client: reqwest::Client::new(),
        }
    }

    /// Make an API call and return the text response with retry logic
    async fn call_api(&self, system: &str, user: &str, max_tokens: u32) -> Result<String, QuocliError> {
        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens,
            system: system.to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: user.to_string(),
            }],
        };

        let mut last_error = None;
        let retry_delays = [2000, 4000, 8000, 16000]; // milliseconds

        for attempt in 0..=retry_delays.len() {
            let result = self
                .client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&request)
                .send()
                .await;

            match result {
                Ok(response) => {
                    if !response.status().is_success() {
                        let status = response.status();
                        let error_text = response.text().await.unwrap_or_default();
                        return Err(QuocliError::Llm(format!(
                            "API request failed with status {}: {}",
                            status, error_text
                        )));
                    }

                    let api_response: AnthropicResponse = response.json().await?;

                    let text = api_response
                        .content
                        .first()
                        .map(|c| c.text.clone())
                        .ok_or_else(|| QuocliError::Llm("Empty response from API".to_string()))?;

                    return Ok(strip_markdown_code_blocks(&text));
                }
                Err(e) => {
                    // Only retry on connection/network errors
                    if e.is_connect() || e.is_request() {
                        last_error = Some(e);
                        if attempt < retry_delays.len() {
                            let delay = retry_delays[attempt];
                            tracing::warn!("Connection error, retrying in {}ms (attempt {}/{})",
                                delay, attempt + 1, retry_delays.len());
                            tokio::time::sleep(tokio::time::Duration::from_millis(delay as u64)).await;
                            continue;
                        }
                    } else {
                        return Err(e.into());
                    }
                }
            }
        }

        Err(last_error.map(|e| e.into()).unwrap_or_else(||
            QuocliError::Llm("Max retries exceeded".to_string())))
    }

    /// Make an API call with prompt caching for the context
    async fn call_api_cached(
        &self,
        system: &str,
        cached_context: &str,
        user_query: &str,
        max_tokens: u32,
    ) -> Result<String, QuocliError> {
        let request = CachedAnthropicRequest {
            model: self.model.clone(),
            max_tokens,
            system: system.to_string(),
            messages: vec![CachedMessage {
                role: "user".to_string(),
                content: vec![
                    CachedContentBlock {
                        content_type: "text".to_string(),
                        text: cached_context.to_string(),
                        cache_control: Some(CacheControl {
                            cache_type: "ephemeral".to_string(),
                        }),
                    },
                    CachedContentBlock {
                        content_type: "text".to_string(),
                        text: user_query.to_string(),
                        cache_control: None,
                    },
                ],
            }],
        };

        let mut last_error = None;
        let retry_delays = [2000, 4000, 8000, 16000];

        for attempt in 0..=retry_delays.len() {
            let result = self
                .client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("anthropic-beta", "prompt-caching-2024-07-31")
                .header("content-type", "application/json")
                .json(&request)
                .send()
                .await;

            match result {
                Ok(response) => {
                    if !response.status().is_success() {
                        let status = response.status();
                        let error_text = response.text().await.unwrap_or_default();
                        return Err(QuocliError::Llm(format!(
                            "API request failed with status {}: {}",
                            status, error_text
                        )));
                    }

                    let api_response: AnthropicResponse = response.json().await?;

                    let text = api_response
                        .content
                        .first()
                        .map(|c| c.text.clone())
                        .ok_or_else(|| QuocliError::Llm("Empty response from API".to_string()))?;

                    return Ok(strip_markdown_code_blocks(&text));
                }
                Err(e) => {
                    if e.is_connect() || e.is_request() {
                        last_error = Some(e);
                        if attempt < retry_delays.len() {
                            let delay = retry_delays[attempt];
                            tracing::warn!("Connection error, retrying in {}ms (attempt {}/{})",
                                delay, attempt + 1, retry_delays.len());
                            tokio::time::sleep(tokio::time::Duration::from_millis(delay as u64)).await;
                            continue;
                        }
                    } else {
                        return Err(e.into());
                    }
                }
            }
        }

        Err(last_error.map(|e| e.into()).unwrap_or_else(||
            QuocliError::Llm("Max retries exceeded".to_string())))
    }
}


/// Strip markdown code blocks from LLM response
fn strip_markdown_code_blocks(text: &str) -> String {
    let text = text.trim();

    // Check for ```json or ``` at start
    if text.starts_with("```") {
        // Find the end of the first line (after ```json or ```)
        let start = text.find('\n').map(|i| i + 1).unwrap_or(0);

        // Find the closing ``` (search from after the opening)
        let end = if start < text.len() {
            text[start..].rfind("```").map(|i| start + i).unwrap_or(text.len())
        } else {
            text.len()
        };

        return text[start..end].trim().to_string();
    }

    text.to_string()
}

/// Extract flags from help text using regex (local, no LLM needed)
fn extract_flags_from_help(help_text: &str) -> Vec<Vec<String>> {
    let mut all_flags: Vec<Vec<String>> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Pattern to match flags like: -x, --long-option, -x <arg>, --option=value, etc.
    // Look for lines that start with whitespace followed by a dash
    let line_pattern = Regex::new(r"(?m)^\s+(-[a-zA-Z0-9](?:[,\s]+--[a-zA-Z0-9-]+)?|--[a-zA-Z0-9-]+(?:[,\s]+-[a-zA-Z0-9])?)").unwrap();

    // Pattern to extract individual flags from a match
    let flag_pattern = Regex::new(r"(-[a-zA-Z0-9]|--[a-zA-Z0-9-]+)").unwrap();

    for cap in line_pattern.captures_iter(help_text) {
        let matched = cap.get(1).unwrap().as_str();
        let mut flags: Vec<String> = Vec::new();

        for flag_cap in flag_pattern.captures_iter(matched) {
            let flag = flag_cap.get(1).unwrap().as_str().to_string();
            if !seen.contains(&flag) {
                flags.push(flag.clone());
                seen.insert(flag);
            }
        }

        if !flags.is_empty() {
            all_flags.push(flags);
        }
    }

    // Also try to catch standalone long options that might not be indented
    let standalone_pattern = Regex::new(r"(?m)^(--[a-zA-Z0-9][a-zA-Z0-9-]*)").unwrap();
    for cap in standalone_pattern.captures_iter(help_text) {
        let flag = cap.get(1).unwrap().as_str().to_string();
        if !seen.contains(&flag) {
            all_flags.push(vec![flag.clone()]);
            seen.insert(flag);
        }
    }

    all_flags
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
}

#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: String,
}

/// Request structure for cached API calls
#[derive(Serialize)]
struct CachedAnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<CachedMessage>,
}

#[derive(Serialize)]
struct CachedMessage {
    role: String,
    content: Vec<CachedContentBlock>,
}

#[derive(Serialize)]
struct CachedContentBlock {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<CacheControl>,
}

#[derive(Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    cache_type: String,
}

#[async_trait]
impl LlmClient for AnthropicClient {
    async fn generate_spec(
        &self,
        command: &str,
        subcommands: &[String],
        docs: &HelpDocumentation,
        help_hash: &str,
    ) -> Result<CommandSpec, QuocliError> {
        let full_command = if subcommands.is_empty() {
            command.to_string()
        } else {
            format!("{} {}", command, subcommands.join(" "))
        };

        let help_text = &docs.help_text;
        let has_manpage = !docs.manpage_text.is_empty();
        if has_manpage {
            tracing::info!("Manpage available ({} chars), will use for enhanced details",
                docs.manpage_text.len());
        }

        // === PASS 1: Extract flags locally using regex (instant, no token limits) ===
        tracing::info!("Pass 1: Extracting flags from help text for {}", full_command);

        let extracted_flags = extract_flags_from_help(help_text);
        tracing::info!("Extracted {} flag groups from help text", extracted_flags.len());

        // Get command metadata (description, danger level) with a small LLM call
        let metadata_system = "You are a CLI analyzer. Return only valid JSON.";
        let metadata_user = format!(
            r#"Analyze this command and return JSON with description and danger_level.

COMMAND: {full_command}

HELP TEXT (first 500 chars):
{}

Return: {{"description": "brief description", "danger_level": "low"}}
danger_level: low/medium/high/critical based on potential for data loss.

JSON only, no other text."#,
            help_text.chars().take(500).collect::<String>()
        );

        let metadata_json = self.call_api(metadata_system, &metadata_user, 256).await?;

        #[derive(Deserialize)]
        struct Metadata {
            description: String,
            danger_level: DangerLevel,
        }

        let metadata: Metadata = serde_json::from_str(&metadata_json).unwrap_or(Metadata {
            description: format!("Command: {}", full_command),
            danger_level: DangerLevel::Low,
        });

        tracing::info!("Got metadata: {} options to process", extracted_flags.len());

        // === PASS 2: Get details for each option ===
        let detail_system = prompt::option_detail_system_prompt();
        let total = extracted_flags.len();
        let mut detailed_options: Vec<CommandOption> = Vec::with_capacity(total);

        // Use batching with caching if we have more options than the batch size
        let use_caching = total > OPTIONS_PER_BATCH;

        // Show initial progress (after metadata call completes)
        if use_caching {
            let num_batches = (total + OPTIONS_PER_BATCH - 1) / OPTIONS_PER_BATCH;
            eprint!("\rProcessing options: 0/{} (batch 0/{})    ", total, num_batches);
        } else {
            eprint!("\rProcessing options: 0/{}    ", total);
        }
        io::stderr().flush().ok();

        if use_caching {
            // Build cached context once
            let manpage_opt = if has_manpage {
                Some(docs.manpage_text.as_str())
            } else {
                None
            };
            let cached_context = prompt::build_cached_context(&full_command, help_text, manpage_opt);

            let num_batches = (total + OPTIONS_PER_BATCH - 1) / OPTIONS_PER_BATCH;
            tracing::info!("Using prompt caching with {} options per batch ({} batches)", OPTIONS_PER_BATCH, num_batches);

            // Process in batches of OPTIONS_PER_BATCH
            for (batch_idx, chunk) in extracted_flags.chunks(OPTIONS_PER_BATCH).enumerate() {
                let query = prompt::batched_option_query(chunk);

                // Show which batch is being processed
                eprint!("\rProcessing options: {}/{} (batch {}/{})    ",
                    detailed_options.len(), total, batch_idx + 1, num_batches);
                io::stderr().flush().ok();

                // Calculate max tokens based on batch size (about 450 tokens per option for safety)
                let max_tokens = (chunk.len() * 450) as u32;

                let batch_json = self.call_api_cached(&detail_system, &cached_context, &query, max_tokens).await?;

                // Parse as array of CommandOption
                let batch_options: Vec<CommandOption> = serde_json::from_str(&batch_json).map_err(|e| {
                    tracing::warn!("Failed to parse batched options: {}", e);

                    // Save failed response to debug file
                    if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "quocli") {
                        let debug_dir = proj_dirs.data_dir().join("debug");
                        if std::fs::create_dir_all(&debug_dir).is_ok() {
                            let debug_file = debug_dir.join("failed_response.json");
                            if let Err(write_err) = std::fs::write(&debug_file, &batch_json) {
                                tracing::warn!("Failed to save debug file: {}", write_err);
                            } else {
                                tracing::info!("Saved failed response to {:?}", debug_file);
                                eprintln!("\nDebug: Failed JSON saved to {:?}", debug_file);
                            }
                        }
                    }

                    QuocliError::Llm(format!("Failed to parse batched options: {}", e))
                })?;

                detailed_options.extend(batch_options);
            }

            // Show final progress for batched mode
            eprint!("\rProcessing options: {}/{} (batch {}/{})    ",
                detailed_options.len(), total, num_batches, num_batches);
            io::stderr().flush().ok();
        } else {
            // Use simple approach for small number of options (no caching overhead)
            let manpage_opt = if has_manpage {
                Some(docs.manpage_text.as_str())
            } else {
                None
            };

            tracing::info!("Using simple approach for {} options (no caching)", total);

            let prompts: Vec<(Vec<String>, String)> = extracted_flags
                .iter()
                .map(|flags| {
                    let detail_user = prompt::option_detail_user_prompt(&full_command, flags, help_text, manpage_opt);
                    (flags.clone(), detail_user)
                })
                .collect();

            // Process concurrently
            for chunk in prompts.chunks(MAX_CONCURRENT_REQUESTS) {
                let futures: Vec<_> = chunk
                    .iter()
                    .map(|(flags, detail_user)| {
                        let flags = flags.clone();
                        let detail_user = detail_user.clone();
                        let detail_system = detail_system.clone();

                        async move {
                            let detail_json = self.call_api(&detail_system, &detail_user, 1024).await?;

                            let detailed: CommandOption = serde_json::from_str(&detail_json).map_err(|e| {
                                tracing::warn!("Failed to parse option details for {:?}: {}", flags, e);
                                QuocliError::Llm(format!("Failed to parse option detail: {}", e))
                            })?;

                            Ok::<CommandOption, QuocliError>(detailed)
                        }
                    })
                    .collect();

                let batch_results = future::try_join_all(futures).await?;
                detailed_options.extend(batch_results);

                // Show progress
                eprint!("\rProcessing options: {}/{}    ", detailed_options.len(), total);
                io::stderr().flush().ok();
            }
        }

        // Clear the progress line
        eprintln!("\rProcessing options: {}/{}    ", total, total);
        tracing::info!("Successfully processed {} options", detailed_options.len());

        // === Assemble final spec ===
        let spec = CommandSpec {
            command: command.to_string(),
            version_hash: help_hash.to_string(),
            description: metadata.description,
            options: detailed_options,
            positional_args: vec![], // TODO: extract positional args from help text
            subcommands: vec![],
            danger_level: metadata.danger_level,
            examples: vec![],
        };

        Ok(spec)
    }

    async fn chat(
        &self,
        context: &str,
        message: &str,
    ) -> Result<String, QuocliError> {
        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: 1024,
            system: context.to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: message.to_string(),
            }],
        };

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(QuocliError::Llm(format!(
                "API request failed with status {}: {}",
                status, error_text
            )));
        }

        let api_response: AnthropicResponse = response.json().await?;

        let text = api_response
            .content
            .first()
            .map(|c| c.text.clone())
            .ok_or_else(|| QuocliError::Llm("Empty response from API".to_string()))?;

        Ok(text)
    }
}
