mod loader;

pub use loader::load_config;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub shell: ShellConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            cache: CacheConfig::default(),
            ui: UiConfig::default(),
            shell: ShellConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_fallback_model")]
    pub fallback_model: String,
}

fn default_provider() -> String {
    "anthropic".to_string()
}

fn default_api_key_env() -> String {
    "ANTHROPIC_API_KEY".to_string()
}

fn default_model() -> String {
    "claude-sonnet-4-5-20250929".to_string()
}

fn default_fallback_model() -> String {
    "claude-haiku-4-5-20250514".to_string()
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            api_key_env: default_api_key_env(),
            model: default_model(),
            fallback_model: default_fallback_model(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_cache_path")]
    pub path: PathBuf,
    #[serde(default = "default_auto_refresh")]
    pub auto_refresh: bool,
    #[serde(default = "default_ttl_days")]
    pub ttl_days: u32,
}

fn default_cache_path() -> PathBuf {
    directories::ProjectDirs::from("", "", "quocli")
        .map(|dirs| dirs.data_dir().join("cache.db"))
        .unwrap_or_else(|| PathBuf::from("~/.local/share/quocli/cache.db"))
}

fn default_auto_refresh() -> bool {
    true
}

fn default_ttl_days() -> u32 {
    30
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            path: default_cache_path(),
            auto_refresh: default_auto_refresh(),
            ttl_days: default_ttl_days(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_show_examples")]
    pub show_examples: bool,
    #[serde(default = "default_preview_command")]
    pub preview_command: bool,
}

fn default_theme() -> String {
    "dark".to_string()
}

fn default_show_examples() -> bool {
    true
}

fn default_preview_command() -> bool {
    true
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            show_examples: default_show_examples(),
            preview_command: default_preview_command(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    #[serde(default = "default_shell_type")]
    pub shell_type: String,
    #[serde(default = "default_history_file")]
    pub history_file: String,
    #[serde(default = "default_export_envvars")]
    pub export_envvars: bool,
}

fn default_shell_type() -> String {
    "auto".to_string()
}

fn default_history_file() -> String {
    "auto".to_string()
}

fn default_export_envvars() -> bool {
    true
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            shell_type: default_shell_type(),
            history_file: default_history_file(),
            export_envvars: default_export_envvars(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub keyring_integration: bool,
    #[serde(default = "default_confirm_dangerous")]
    pub confirm_dangerous: bool,
    #[serde(default = "default_audit_log")]
    pub audit_log: bool,
}

fn default_confirm_dangerous() -> bool {
    true
}

fn default_audit_log() -> bool {
    true
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            keyring_integration: false,
            confirm_dangerous: default_confirm_dangerous(),
            audit_log: default_audit_log(),
        }
    }
}
