use crate::llm::client::{async_trait, LlmClient};
use crate::llm::prompt;
use crate::parser::{CommandOption, CommandSpec, DangerLevel};
use crate::QuocliError;
use futures::future;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Maximum concurrent API requests to avoid rate limiting
const MAX_CONCURRENT_REQUESTS: usize = 10;

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

    /// Make an API call and return the text response
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

        Ok(strip_markdown_code_blocks(&text))
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

#[async_trait]
impl LlmClient for AnthropicClient {
    async fn generate_spec(
        &self,
        command: &str,
        subcommands: &[String],
        help_text: &str,
        help_hash: &str,
    ) -> Result<CommandSpec, QuocliError> {
        let full_command = if subcommands.is_empty() {
            command.to_string()
        } else {
            format!("{} {}", command, subcommands.join(" "))
        };

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

        // === PASS 2: Get details for each option (concurrent) ===
        let detail_system = prompt::option_detail_system_prompt();

        tracing::info!("Pass 2: Getting details for {} options concurrently (max {} parallel)",
            extracted_flags.len(), MAX_CONCURRENT_REQUESTS);

        // Prepare all prompts upfront
        let prompts: Vec<(usize, Vec<String>, String)> = extracted_flags
            .iter()
            .enumerate()
            .map(|(i, flags)| {
                let detail_user = prompt::option_detail_user_prompt(&full_command, flags, help_text);
                (i, flags.clone(), detail_user)
            })
            .collect();

        // Process in batches to limit concurrency
        let mut detailed_options: Vec<CommandOption> = Vec::with_capacity(prompts.len());

        for chunk in prompts.chunks(MAX_CONCURRENT_REQUESTS) {
            let futures: Vec<_> = chunk
                .iter()
                .map(|(i, flags, detail_user)| {
                    let i = *i;
                    let flags = flags.clone();
                    let detail_user = detail_user.clone();
                    let detail_system = detail_system.clone();

                    async move {
                        tracing::debug!("Getting details for option {}: {:?}", i + 1, flags);

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

            tracing::debug!("Completed batch, {} options processed so far", detailed_options.len());
        }

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
