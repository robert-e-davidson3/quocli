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
