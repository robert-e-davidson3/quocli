use crate::config::Config;
use crate::parser::{CommandSpec, HelpDocumentation};
use crate::QuocliError;

use super::providers::anthropic::AnthropicClient;

/// Trait for LLM clients
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    async fn generate_spec(
        &self,
        command: &str,
        subcommands: &[String],
        docs: &HelpDocumentation,
        help_hash: &str,
    ) -> Result<CommandSpec, QuocliError>;

    async fn chat(
        &self,
        context: &str,
        message: &str,
    ) -> Result<String, QuocliError>;
}

/// Create an LLM client based on configuration
pub fn create_client(config: &Config) -> Result<Box<dyn LlmClient>, QuocliError> {
    match config.llm.provider.as_str() {
        "anthropic" => {
            let api_key = std::env::var(&config.llm.api_key_env).map_err(|_| {
                QuocliError::Config(format!(
                    "API key not found in environment variable: {}",
                    config.llm.api_key_env
                ))
            })?;

            Ok(Box::new(AnthropicClient::new(
                api_key,
                config.llm.model.clone(),
            )))
        }
        provider => Err(QuocliError::Config(format!(
            "Unsupported LLM provider: {}",
            provider
        ))),
    }
}

// Re-export async_trait for providers
pub use async_trait::async_trait;
