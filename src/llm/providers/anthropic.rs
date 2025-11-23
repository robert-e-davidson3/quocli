use crate::llm::client::{async_trait, LlmClient};
use crate::llm::prompt;
use crate::parser::CommandSpec;
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
        let system_prompt = prompt::spec_generation_system_prompt();
        let user_prompt = prompt::spec_generation_user_prompt(command, subcommands, help_text, help_hash);

        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: 4096,
            system: system_prompt,
            messages: vec![Message {
                role: "user".to_string(),
                content: user_prompt,
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

        // Parse the JSON response
        let spec: CommandSpec = serde_json::from_str(&text).map_err(|e| {
            QuocliError::Llm(format!("Failed to parse spec JSON: {}. Response: {}", e, text))
        })?;

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
