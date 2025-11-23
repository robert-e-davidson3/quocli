use std::collections::HashMap;
use std::env;

/// Scan environment for variables that might match command arguments
#[allow(dead_code)]
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

/// Generate a suggested environment variable name
#[allow(dead_code)]
pub fn generate_env_var_name(command: &str, flag: &str) -> String {
    let flag_clean = flag
        .trim_start_matches('-')
        .to_uppercase()
        .replace('-', "_");
    format!("QUOCLI_{}_{}", command.to_uppercase(), flag_clean)
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
}
