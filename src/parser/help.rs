use crate::QuocliError;
use sha2::{Digest, Sha256};
use std::process::Command;

/// Combined help documentation for a command
pub struct HelpDocumentation {
    /// Help text from --help or similar
    pub help_text: String,
    /// Man page text if available (may be empty)
    pub manpage_text: String,
}

impl HelpDocumentation {
    /// Get the combined text for hashing (to detect changes)
    pub fn combined_text(&self) -> String {
        if self.manpage_text.is_empty() {
            self.help_text.clone()
        } else {
            format!("{}\n\n--- MANPAGE ---\n\n{}", self.help_text, self.manpage_text)
        }
    }
}

/// Get help text and manpage for a command
pub fn get_help_documentation(command: &str, subcommands: &[String]) -> Result<HelpDocumentation, QuocliError> {
    let help_text = get_help_text_only(command, subcommands)?;
    let manpage_text = get_manpage_text(command, subcommands).unwrap_or_default();

    Ok(HelpDocumentation {
        help_text,
        manpage_text,
    })
}

/// Get help text for a command, trying various methods
pub fn get_help_text(command: &str, subcommands: &[String]) -> Result<String, QuocliError> {
    get_help_text_only(command, subcommands)
}

/// Get help text only (no manpage fallback)
fn get_help_text_only(command: &str, subcommands: &[String]) -> Result<String, QuocliError> {
    let mut args: Vec<&str> = subcommands.iter().map(|s| s.as_str()).collect();

    // Try extended help variants first (for commands like curl that have truncated default help)
    for extended in &["--help", "all", "--help=all", "--help-all"] {
        let mut extended_args = args.clone();
        if *extended == "--help" {
            extended_args.push("--help");
            extended_args.push("all");
        } else {
            extended_args.push(extended);
        }
        if let Ok(output) = try_command(command, &extended_args) {
            // Extended help should be substantial
            if !output.is_empty() && output.len() > 500 {
                return Ok(output);
            }
        }
    }

    // Try --help
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

    Err(QuocliError::NoHelpText(command.to_string()))
}

/// Get manpage text for a command
fn get_manpage_text(command: &str, subcommands: &[String]) -> Result<String, QuocliError> {
    let man_command = if subcommands.is_empty() {
        command.to_string()
    } else {
        format!("{}-{}", command, subcommands.join("-"))
    };

    // Use col -b to strip formatting control characters from man output
    let output = Command::new("sh")
        .args(["-c", &format!("man {} 2>/dev/null | col -b", man_command)])
        .output()
        .map_err(|_| QuocliError::CommandNotFound("man".to_string()))?;

    let text = String::from_utf8_lossy(&output.stdout).to_string();

    if text.len() > 100 {
        Ok(text)
    } else {
        Err(QuocliError::NoHelpText(format!("man {}", man_command)))
    }
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
