use crate::QuocliError;
use sha2::{Digest, Sha256};
use std::process::Command;

/// Get help text for a command, trying various methods
pub fn get_help_text(command: &str, subcommands: &[String]) -> Result<String, QuocliError> {
    let mut args: Vec<&str> = subcommands.iter().map(|s| s.as_str()).collect();

    // Try --help first
    args.push("--help");
    if let Ok(output) = try_command(command, &args) {
        if !output.is_empty() && output.len() > 50 {
            return Ok(output);
        }
    }
    args.pop();

    // Try -h
    args.push("-h");
    if let Ok(output) = try_command(command, &args) {
        if !output.is_empty() && output.len() > 50 {
            return Ok(output);
        }
    }
    args.pop();

    // Try help subcommand
    let mut help_args: Vec<&str> = vec!["help"];
    help_args.extend(subcommands.iter().map(|s| s.as_str()));
    if let Ok(output) = try_command(command, &help_args) {
        if !output.is_empty() && output.len() > 50 {
            return Ok(output);
        }
    }

    // Try man page
    let man_command = if subcommands.is_empty() {
        command.to_string()
    } else {
        format!("{}-{}", command, subcommands.join("-"))
    };

    if let Ok(output) = try_command("man", &[&man_command]) {
        if !output.is_empty() && output.len() > 50 {
            return Ok(output);
        }
    }

    Err(QuocliError::NoHelpText(command.to_string()))
}

/// Try to run a command and get its output
fn try_command(command: &str, args: &[&str]) -> Result<String, QuocliError> {
    let output = Command::new(command)
        .args(args)
        .output()
        .map_err(|_| QuocliError::CommandNotFound(command.to_string()))?;

    // Some commands output help to stderr
    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).to_string()
    } else {
        String::from_utf8_lossy(&output.stdout).to_string()
    };

    Ok(text)
}

/// Hash help text using SHA-256
pub fn hash_help_text(help_text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(help_text.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_help_text() {
        let hash1 = hash_help_text("hello world");
        let hash2 = hash_help_text("hello world");
        let hash3 = hash_help_text("different text");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 64); // SHA-256 produces 64 hex chars
    }
}
