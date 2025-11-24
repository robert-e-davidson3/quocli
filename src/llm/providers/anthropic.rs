use crate::llm::client::{async_trait, LlmClient};
use crate::llm::prompt;
use crate::parser::{CommandOption, CommandSpec, DangerLevel, HelpDocumentation, PositionalArg};
use crate::QuocliError;
use futures::stream::{FuturesUnordered, StreamExt};
use futures::future::BoxFuture;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{self, Write};

/// Maximum concurrent API requests to avoid rate limiting
const MAX_CONCURRENT_REQUESTS: usize = 10;

/// Retry delays in milliseconds for exponential backoff
const RETRY_DELAYS_MS: &[u64] = &[2000, 4000, 8000, 16000];

/// HTTP status codes for retry logic
const HTTP_STATUS_OVERLOADED: u16 = 529;
const HTTP_STATUS_SERVICE_UNAVAILABLE: u16 = 503;

/// Fast model for detail extraction (cheaper, faster for simple tasks)
const HAIKU_MODEL: &str = "claude-haiku-4-5-20251001";

/// Delay in milliseconds to ensure cache is ready after first request
const CACHE_WARMUP_DELAY_MS: u64 = 500;

/// API endpoint URL
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

/// Log retry attempt and sleep for the specified delay
async fn retry_delay(attempt: usize, reason: &str) {
    let delay = RETRY_DELAYS_MS[attempt];
    tracing::warn!("{}, retrying in {}ms (attempt {}/{})",
        reason, delay, attempt + 1, RETRY_DELAYS_MS.len());
    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
}

/// Check if a request error is retryable (connection/network errors)
fn is_retryable_error(error: &reqwest::Error) -> bool {
    error.is_connect() || error.is_request()
}

/// Check if an HTTP status code is retryable (overloaded/unavailable)
fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status.as_u16() == HTTP_STATUS_OVERLOADED || status.as_u16() == HTTP_STATUS_SERVICE_UNAVAILABLE
}

/// Extract text content from an Anthropic API response
fn extract_response_text(response: AnthropicResponse) -> Result<String, QuocliError> {
    let text = response
        .content
        .first()
        .map(|c| c.text.clone())
        .ok_or_else(|| QuocliError::Llm("Empty response from API".to_string()))?;

    Ok(strip_markdown_code_blocks(&text))
}

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
    async fn call_api(&self, system: &str, user: &str, max_tokens: u32, model_override: Option<&str>) -> Result<String, QuocliError> {
        let model = model_override.map(|s| s.to_string()).unwrap_or_else(|| self.model.clone());
        let request = AnthropicRequest {
            model,
            max_tokens,
            system: system.to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: user.to_string(),
            }],
        };

        let mut last_error = None;

        for attempt in 0..=RETRY_DELAYS_MS.len() {
            let result = self
                .client
                .post(ANTHROPIC_API_URL)
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
                    return extract_response_text(api_response);
                }
                Err(e) => {
                    if is_retryable_error(&e) {
                        last_error = Some(e);
                        if attempt < RETRY_DELAYS_MS.len() {
                            retry_delay(attempt, "Connection error").await;
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
        model_override: Option<&str>,
    ) -> Result<String, QuocliError> {
        let model = model_override.map(|s| s.to_string()).unwrap_or_else(|| self.model.clone());
        let request = CachedAnthropicRequest {
            model,
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

        for attempt in 0..=RETRY_DELAYS_MS.len() {
            let result = self
                .client
                .post(ANTHROPIC_API_URL)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("anthropic-beta", "prompt-caching-2024-07-31")
                .header("content-type", "application/json")
                .json(&request)
                .send()
                .await;

            match result {
                Ok(response) => {
                    let status = response.status();

                    // Retry on overloaded or service unavailable
                    if is_retryable_status(status) {
                        if attempt < RETRY_DELAYS_MS.len() {
                            retry_delay(attempt, &format!("API overloaded ({})", status)).await;
                            continue;
                        } else {
                            let error_text = response.text().await.unwrap_or_default();
                            return Err(QuocliError::Llm(format!(
                                "API overloaded after {} retries: {}",
                                RETRY_DELAYS_MS.len(), error_text
                            )));
                        }
                    }

                    if !status.is_success() {
                        let error_text = response.text().await.unwrap_or_default();
                        return Err(QuocliError::Llm(format!(
                            "API request failed with status {}: {}",
                            status, error_text
                        )));
                    }

                    let api_response: AnthropicResponse = response.json().await?;
                    return extract_response_text(api_response);
                }
                Err(e) => {
                    if is_retryable_error(&e) && attempt < RETRY_DELAYS_MS.len() {
                        retry_delay(attempt, "Connection error").await;
                        continue;
                    }
                    return Err(e.into());
                }
            }
        }

        Err(QuocliError::Llm("Max retries exceeded".to_string()))
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

        // Build cached context with full help text and manpage (used for all LLM calls)
        let manpage_opt = if has_manpage {
            Some(docs.manpage_text.as_str())
        } else {
            None
        };
        let cached_context = prompt::build_cached_context(&full_command, help_text, manpage_opt);

        // Extract positional args using LLM with full context (use Sonnet for better semantic understanding)
        let positional_system = "You are a CLI command parser. Extract positional argument names from usage syntax.";
        let positional_query = prompt::extract_positional_args_query(&cached_context);

        let positional_json = self.call_api(positional_system, &positional_query, 512, None).await?;

        #[derive(Deserialize)]
        struct PositionalArgsResponse {
            args: Vec<String>,
            #[serde(default)]
            positionals_first: bool,
        }

        let (positional_names, positionals_first) = serde_json::from_str::<PositionalArgsResponse>(&positional_json)
            .map(|r| (r.args, r.positionals_first))
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to parse positional args JSON: {}", e);
                (vec![], false)
            });
        tracing::info!("Extracted {} positional arg names from help text (positionals_first: {})",
            positional_names.len(), positionals_first);

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

        let metadata_json = self.call_api(metadata_system, &metadata_user, 256, None).await?;

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

        tracing::info!("Using prompt caching for {} options ({} concurrent)", total, MAX_CONCURRENT_REQUESTS);

        // Show initial progress (after metadata call and context setup)
        eprint!("\rProcessing options: 0/{}    ", total);
        io::stderr().flush().ok();

        // Process first option alone to warm the cache
        if let Some(first_flags) = extracted_flags.first() {
            let query = prompt::single_option_query(first_flags);
            let detail_json = self.call_api_cached(
                &detail_system,
                &cached_context,
                &query,
                4096,
                Some(HAIKU_MODEL),
            ).await?;

            let detailed: CommandOption = serde_json::from_str(&detail_json).map_err(|e| {
                tracing::warn!("Failed to parse option details for {:?}: {}", first_flags, e);
                QuocliError::Llm(format!("Failed to parse option detail: {}", e))
            })?;

            detailed_options.push(detailed);
            eprint!("\rProcessing options: 1/{}    ", total);
            io::stderr().flush().ok();

            // Small delay to ensure cache is ready
            tokio::time::sleep(tokio::time::Duration::from_millis(CACHE_WARMUP_DELAY_MS)).await;
        }

        // Helper to create option extraction future
        let make_option_future = |flags: Vec<String>, detail_system: String, cached_context: String| -> BoxFuture<'_, Result<CommandOption, QuocliError>> {
            Box::pin(async move {
                let query = prompt::single_option_query(&flags);
                let detail_json = self.call_api_cached(
                    &detail_system,
                    &cached_context,
                    &query,
                    4096,
                    Some(HAIKU_MODEL),
                ).await?;

                let detailed: CommandOption = serde_json::from_str(&detail_json).map_err(|e| {
                    tracing::warn!("Failed to parse option details for {:?}: {}", flags, e);

                    // Save failed response to debug file
                    if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "quocli") {
                        let debug_dir = proj_dirs.data_dir().join("debug");
                        if std::fs::create_dir_all(&debug_dir).is_ok() {
                            let flag_name = flags.first().map(|f| f.trim_start_matches('-')).unwrap_or("unknown");
                            let debug_file = debug_dir.join(format!("failed_{}.json", flag_name));
                            if let Err(write_err) = std::fs::write(&debug_file, &detail_json) {
                                tracing::warn!("Failed to save debug file: {}", write_err);
                            } else {
                                tracing::info!("Saved failed response to {:?}", debug_file);
                                eprintln!("\nDebug: Failed JSON saved to {:?}", debug_file);
                            }
                        }
                    }

                    QuocliError::Llm(format!("Failed to parse option detail: {}", e))
                })?;

                Ok(detailed)
            })
        };

        // Process remaining options with streaming concurrency (start new request as each completes)
        let remaining_flags: Vec<_> = extracted_flags.iter().skip(1).cloned().collect();
        let mut flag_iter = remaining_flags.into_iter();
        let mut in_flight: FuturesUnordered<BoxFuture<'_, Result<CommandOption, QuocliError>>> = FuturesUnordered::new();

        // Start initial batch of concurrent requests
        for _ in 0..MAX_CONCURRENT_REQUESTS {
            if let Some(flags) = flag_iter.next() {
                in_flight.push(make_option_future(flags, detail_system.clone(), cached_context.clone()));
            }
        }

        // Process results as they complete, starting new requests immediately
        while let Some(result) = in_flight.next().await {
            let detailed = result?;
            detailed_options.push(detailed);

            // Show progress
            eprint!("\rProcessing options: {}/{}    ", detailed_options.len(), total);
            io::stderr().flush().ok();

            // Start next request if there are more flags
            if let Some(flags) = flag_iter.next() {
                in_flight.push(make_option_future(flags, detail_system.clone(), cached_context.clone()));
            }
        }

        // Clear the progress line
        eprintln!("\rProcessing options: {}/{}    ", total, total);
        tracing::info!("Successfully processed {} options", detailed_options.len());

        // === PASS 3: Get details for each positional argument ===
        let pos_total = positional_names.len();
        let mut detailed_positional: Vec<PositionalArg> = Vec::with_capacity(pos_total);

        if pos_total > 0 {
            tracing::info!("Processing {} positional arguments", pos_total);
            eprint!("\rProcessing positional args: 0/{}    ", pos_total);
            io::stderr().flush().ok();

            // Helper to create positional arg extraction future
            let make_positional_future = |arg_name: String, detail_system: String, cached_context: String| -> BoxFuture<'_, Result<PositionalArg, QuocliError>> {
                Box::pin(async move {
                    let query = prompt::single_positional_arg_query(&arg_name);
                    let detail_json = self.call_api_cached(
                        &detail_system,
                        &cached_context,
                        &query,
                        1024,
                        Some(HAIKU_MODEL),
                    ).await?;

                    let detailed: PositionalArg = serde_json::from_str(&detail_json).map_err(|e| {
                        tracing::warn!("Failed to parse positional arg details for {}: {}", arg_name, e);
                        QuocliError::Llm(format!("Failed to parse positional arg detail: {}", e))
                    })?;

                    Ok(detailed)
                })
            };

            // Process positional args with streaming concurrency
            let mut arg_iter = positional_names.into_iter();
            let mut pos_in_flight: FuturesUnordered<BoxFuture<'_, Result<PositionalArg, QuocliError>>> = FuturesUnordered::new();

            // Start initial batch of concurrent requests
            for _ in 0..MAX_CONCURRENT_REQUESTS {
                if let Some(arg_name) = arg_iter.next() {
                    pos_in_flight.push(make_positional_future(arg_name, detail_system.clone(), cached_context.clone()));
                }
            }

            // Process results as they complete, starting new requests immediately
            while let Some(result) = pos_in_flight.next().await {
                let detailed = result?;
                detailed_positional.push(detailed);

                // Show progress
                eprint!("\rProcessing positional args: {}/{}    ", detailed_positional.len(), pos_total);
                io::stderr().flush().ok();

                // Start next request if there are more args
                if let Some(arg_name) = arg_iter.next() {
                    pos_in_flight.push(make_positional_future(arg_name, detail_system.clone(), cached_context.clone()));
                }
            }

            // Clear the progress line
            eprintln!("\rProcessing positional args: {}/{}    ", pos_total, pos_total);
            tracing::info!("Successfully processed {} positional arguments", detailed_positional.len());
        }

        // === Assemble final spec ===
        let spec = CommandSpec {
            command: command.to_string(),
            version_hash: help_hash.to_string(),
            description: metadata.description,
            options: detailed_options,
            positional_args: detailed_positional,
            subcommands: vec![],
            danger_level: metadata.danger_level,
            examples: vec![],
            positionals_first,
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
