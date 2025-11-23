use crate::parser::ArgumentType;
use regex::Regex;
use std::collections::HashMap;
use std::env;

/// Scan environment for variables that might match command arguments
pub fn scan_matching_env_vars(patterns: &[&str]) -> HashMap<String, String> {
    let mut matches = HashMap::new();

    for (key, value) in env::vars() {
        for pattern in patterns {
            if key.to_lowercase().contains(&pattern.to_lowercase()) {
                matches.insert(key.clone(), value.clone());
            }
        }
    }

    matches
}

/// Get all environment variables
pub fn get_all_env_vars() -> HashMap<String, String> {
    env::vars().collect()
}

/// Generate a suggested environment variable name
pub fn generate_env_var_name(command: &str, flag: &str) -> String {
    let flag_clean = flag
        .trim_start_matches('-')
        .to_uppercase()
        .replace('-', "_");
    format!("QUOCLI_{}_{}", command.to_uppercase(), flag_clean)
}

/// Resolve environment variable references in a string value
/// Supports both $VAR and ${VAR} syntax
pub fn resolve_env_vars(value: &str) -> String {
    // Pattern to match $VAR or ${VAR}
    let re = Regex::new(r"\$\{([^}]+)\}|\$([A-Za-z_][A-Za-z0-9_]*)").unwrap();

    re.replace_all(value, |caps: &regex::Captures| {
        // Get the variable name from either capture group
        let var_name = caps.get(1).or_else(|| caps.get(2))
            .map(|m| m.as_str())
            .unwrap_or("");

        // Look up the environment variable
        env::var(var_name).unwrap_or_else(|_| {
            // If not found, return the original match
            caps.get(0).map(|m| m.as_str().to_string()).unwrap_or_default()
        })
    }).to_string()
}

/// Check if a value contains environment variable references
pub fn contains_env_var(value: &str) -> bool {
    let re = Regex::new(r"\$\{([^}]+)\}|\$([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    re.is_match(value)
}

/// Convert an environment variable value to the appropriate type
/// Returns the converted value as a string, or the original if conversion fails
pub fn convert_env_value(value: &str, target_type: &ArgumentType) -> String {
    match target_type {
        ArgumentType::Bool => {
            // Convert common boolean representations
            match value.to_lowercase().as_str() {
                "true" | "1" | "yes" | "on" | "enabled" => "true".to_string(),
                "false" | "0" | "no" | "off" | "disabled" | "" => "false".to_string(),
                _ => value.to_string(),
            }
        }
        ArgumentType::Int => {
            // Parse as integer, or return original if fails
            value.parse::<i64>()
                .map(|n| n.to_string())
                .unwrap_or_else(|_| value.to_string())
        }
        ArgumentType::Float => {
            // Parse as float, or return original if fails
            value.parse::<f64>()
                .map(|n| n.to_string())
                .unwrap_or_else(|_| value.to_string())
        }
        ArgumentType::String | ArgumentType::Path | ArgumentType::Enum => {
            // No conversion needed for these types
            value.to_string()
        }
    }
}

/// Resolve environment variables and convert to target type
pub fn resolve_and_convert(value: &str, target_type: &ArgumentType) -> String {
    let resolved = resolve_env_vars(value);
    convert_env_value(&resolved, target_type)
}

/// Get environment variable suggestions based on a prefix
pub fn get_env_suggestions(prefix: &str) -> Vec<(String, String)> {
    let prefix_lower = prefix.to_lowercase();
    let mut suggestions: Vec<(String, String)> = env::vars()
        .filter(|(key, _)| key.to_lowercase().starts_with(&prefix_lower))
        .collect();

    // Sort by key name
    suggestions.sort_by(|a, b| a.0.cmp(&b.0));

    // Limit to 10 suggestions
    suggestions.truncate(10);
    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_env_var_name() {
        assert_eq!(
            generate_env_var_name("curl", "--header"),
            "QUOCLI_CURL_HEADER"
        );
        assert_eq!(
            generate_env_var_name("git", "-m"),
            "QUOCLI_GIT_M"
        );
    }

    #[test]
    fn test_resolve_env_vars() {
        // Set test env var
        env::set_var("TEST_VAR", "test_value");

        assert_eq!(resolve_env_vars("$TEST_VAR"), "test_value");
        assert_eq!(resolve_env_vars("${TEST_VAR}"), "test_value");
        // Use braces to delimit variable when followed by valid var chars
        assert_eq!(resolve_env_vars("prefix_${TEST_VAR}_suffix"), "prefix_test_value_suffix");
        assert_eq!(resolve_env_vars("${TEST_VAR}/path"), "test_value/path");
        // Without braces, the whole thing is treated as the var name
        assert_eq!(resolve_env_vars("prefix_$TEST_VAR"), "prefix_test_value");

        // Unknown var should be preserved
        assert_eq!(resolve_env_vars("$UNKNOWN_VAR_123"), "$UNKNOWN_VAR_123");

        env::remove_var("TEST_VAR");
    }

    #[test]
    fn test_contains_env_var() {
        assert!(contains_env_var("$HOME"));
        assert!(contains_env_var("${HOME}"));
        assert!(contains_env_var("path/$HOME/dir"));
        assert!(!contains_env_var("no vars here"));
        assert!(!contains_env_var("just $"));
    }

    #[test]
    fn test_convert_env_value_bool() {
        assert_eq!(convert_env_value("true", &ArgumentType::Bool), "true");
        assert_eq!(convert_env_value("1", &ArgumentType::Bool), "true");
        assert_eq!(convert_env_value("yes", &ArgumentType::Bool), "true");
        assert_eq!(convert_env_value("false", &ArgumentType::Bool), "false");
        assert_eq!(convert_env_value("0", &ArgumentType::Bool), "false");
        assert_eq!(convert_env_value("no", &ArgumentType::Bool), "false");
    }

    #[test]
    fn test_convert_env_value_int() {
        assert_eq!(convert_env_value("42", &ArgumentType::Int), "42");
        assert_eq!(convert_env_value("-10", &ArgumentType::Int), "-10");
        assert_eq!(convert_env_value("not a number", &ArgumentType::Int), "not a number");
    }

    #[test]
    fn test_convert_env_value_float() {
        assert_eq!(convert_env_value("3.14", &ArgumentType::Float), "3.14");
        assert_eq!(convert_env_value("42", &ArgumentType::Float), "42");
    }
}
