use crate::llm::client::{async_trait, LlmClient};
use crate::llm::prompt;
use crate::parser::{CommandOption, CommandSpec, DangerLevel, PositionalArg};
use crate::QuocliError;
use serde::{Deserialize, Serialize};

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

// Intermediate structures for two-pass approach
#[derive(Deserialize)]
struct DiscoveryResponse {
    command: String,
    description: String,
    danger_level: DangerLevel,
    options: Vec<CompactOption>,
    positional_args: Vec<CompactPositional>,
    #[serde(default)]
    subcommands: Vec<String>,
}

#[derive(Deserialize)]
struct CompactOption {
    flags: Vec<String>,
}

#[derive(Deserialize)]
struct CompactPositional {
    name: String,
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

        // === PASS 1: Discover all options (compact format) ===
        tracing::info!("Pass 1: Discovering options for {}", full_command);

        let discovery_system = prompt::options_discovery_system_prompt();
        let discovery_user = prompt::options_discovery_user_prompt(&full_command, help_text);

        let discovery_json = self.call_api(&discovery_system, &discovery_user, 8192).await?;

        let discovery: DiscoveryResponse = serde_json::from_str(&discovery_json).map_err(|e| {
            QuocliError::Llm(format!("Failed to parse discovery JSON: {}. Response: {}", e, discovery_json))
        })?;

        tracing::info!("Discovered {} options, {} positional args",
            discovery.options.len(), discovery.positional_args.len());

        // === PASS 2: Get details for each option ===
        let detail_system = prompt::option_detail_system_prompt();
        let mut detailed_options: Vec<CommandOption> = Vec::new();

        for (i, opt) in discovery.options.iter().enumerate() {
            tracing::info!("Pass 2: Getting details for option {}/{}: {:?}",
                i + 1, discovery.options.len(), opt.flags);

            let detail_user = prompt::option_detail_user_prompt(&full_command, &opt.flags, help_text);
            let detail_json = self.call_api(&detail_system, &detail_user, 1024).await?;

            let detailed: CommandOption = serde_json::from_str(&detail_json).map_err(|e| {
                // If we fail to parse details, create a minimal option from discovery data
                tracing::warn!("Failed to parse option details: {}. Using minimal data.", e);
                return QuocliError::Llm(format!("Failed to parse option detail: {}", e));
            })?;

            detailed_options.push(detailed);
        }

        // === Get details for positional arguments ===
        let mut detailed_positionals: Vec<PositionalArg> = Vec::new();

        for (i, pos) in discovery.positional_args.iter().enumerate() {
            tracing::info!("Pass 2: Getting details for positional {}/{}: {}",
                i + 1, discovery.positional_args.len(), pos.name);

            let detail_user = prompt::positional_detail_user_prompt(&full_command, &pos.name, help_text);
            let detail_json = self.call_api(&detail_system, &detail_user, 512).await?;

            let detailed: PositionalArg = serde_json::from_str(&detail_json).map_err(|e| {
                tracing::warn!("Failed to parse positional details: {}. Using minimal data.", e);
                return QuocliError::Llm(format!("Failed to parse positional detail: {}", e));
            })?;

            detailed_positionals.push(detailed);
        }

        // === Assemble final spec ===
        let spec = CommandSpec {
            command: discovery.command,
            version_hash: help_hash.to_string(),
            description: discovery.description,
            options: detailed_options,
            positional_args: detailed_positionals,
            subcommands: discovery.subcommands,
            danger_level: discovery.danger_level,
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
