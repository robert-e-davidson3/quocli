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

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.llm.provider, "anthropic");
        assert_eq!(config.cache.ttl_days, 30);
    }
}
