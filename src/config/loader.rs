use super::Config;
use anyhow::Result;
use std::path::PathBuf;

/// Load configuration from file or return defaults
pub fn load_config() -> Result<Config> {
    let config_path = get_config_path();

    if config_path.exists() {
        let contents = std::fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    } else {
        // Create default config directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(Config::default())
    }
}

/// Get the path to the config file
fn get_config_path() -> PathBuf {
    directories::ProjectDirs::from("", "", "quocli")
        .map(|dirs| dirs.config_dir().join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("~/.config/quocli/config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.llm.provider, "anthropic");
        assert_eq!(config.cache.ttl_days, 30);
    }

    #[test]
    fn test_default_llm_config() {
        let config = super::super::LlmConfig::default();
        assert_eq!(config.provider, "anthropic");
        assert_eq!(config.api_key_env, "ANTHROPIC_API_KEY");
        assert_eq!(config.model, "claude-sonnet-4-5-20250929");
        assert_eq!(config.fallback_model, "claude-haiku-4-5-20250514");
    }

    #[test]
    fn test_default_cache_config() {
        let config = super::super::CacheConfig::default();
        assert!(config.auto_refresh);
        assert_eq!(config.ttl_days, 30);
        // Path should end with cache.db
        assert!(config.path.to_string_lossy().ends_with("cache.db"));
    }

    #[test]
    fn test_default_ui_config() {
        let config = super::super::UiConfig::default();
        assert_eq!(config.theme, "dark");
        assert!(config.show_examples);
        assert!(config.preview_command);
    }

    #[test]
    fn test_default_shell_config() {
        let config = super::super::ShellConfig::default();
        assert_eq!(config.shell_type, "auto");
        assert_eq!(config.history_file, "auto");
        assert!(config.export_envvars);
    }

    #[test]
    fn test_default_security_config() {
        let config = super::super::SecurityConfig::default();
        assert!(!config.keyring_integration);
        assert!(config.confirm_dangerous);
        assert!(config.audit_log);
    }

    #[test]
    fn test_load_config_from_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("quocli");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("config.toml");

        let config_content = r#"
[llm]
provider = "custom"
model = "custom-model"
api_key_env = "CUSTOM_API_KEY"
fallback_model = "fallback"

[cache]
auto_refresh = false
ttl_days = 60

[ui]
theme = "light"
show_examples = false
preview_command = false

[shell]
shell_type = "zsh"
history_file = "~/.custom_history"
export_envvars = false

[security]
keyring_integration = true
confirm_dangerous = false
audit_log = false
"#;

        std::fs::write(&config_path, config_content).unwrap();

        // We can't easily test load_config() directly because it uses a fixed path,
        // but we can test parsing
        let config: Config = toml::from_str(config_content).unwrap();

        assert_eq!(config.llm.provider, "custom");
        assert_eq!(config.llm.model, "custom-model");
        assert_eq!(config.llm.api_key_env, "CUSTOM_API_KEY");
        assert!(!config.cache.auto_refresh);
        assert_eq!(config.cache.ttl_days, 60);
        assert_eq!(config.ui.theme, "light");
        assert!(!config.ui.show_examples);
        assert_eq!(config.shell.shell_type, "zsh");
        assert!(config.security.keyring_integration);
        assert!(!config.security.confirm_dangerous);
    }

    #[test]
    fn test_partial_config_uses_defaults() {
        let config_content = r#"
[llm]
model = "custom-model"

[cache]
ttl_days = 90
"#;

        let config: Config = toml::from_str(config_content).unwrap();

        // Specified values
        assert_eq!(config.llm.model, "custom-model");
        assert_eq!(config.cache.ttl_days, 90);

        // Default values for unspecified fields
        assert_eq!(config.llm.provider, "anthropic");
        assert_eq!(config.llm.api_key_env, "ANTHROPIC_API_KEY");
        assert!(config.cache.auto_refresh);
        assert_eq!(config.ui.theme, "dark");
        assert!(config.security.confirm_dangerous);
    }

    #[test]
    fn test_empty_config_uses_defaults() {
        let config_content = "";
        let config: Config = toml::from_str(config_content).unwrap();

        // All should be defaults
        assert_eq!(config.llm.provider, "anthropic");
        assert_eq!(config.cache.ttl_days, 30);
        assert_eq!(config.ui.theme, "dark");
        assert_eq!(config.shell.shell_type, "auto");
        assert!(config.security.confirm_dangerous);
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = Config::default();
        let serialized = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();

        assert_eq!(config.llm.provider, deserialized.llm.provider);
        assert_eq!(config.cache.ttl_days, deserialized.cache.ttl_days);
        assert_eq!(config.ui.theme, deserialized.ui.theme);
    }

    #[test]
    fn test_invalid_config_returns_error() {
        // Use truly invalid TOML syntax
        let invalid_content = r#"
[llm
provider = missing closing bracket
"#;

        let result: std::result::Result<Config, _> = toml::from_str(invalid_content);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_with_unknown_fields() {
        // Unknown fields should be ignored
        let config_content = r#"
[llm]
provider = "anthropic"
unknown_field = "value"

[unknown_section]
key = "value"
"#;

        let config: Config = toml::from_str(config_content).unwrap();
        assert_eq!(config.llm.provider, "anthropic");
    }
}
