use crate::config::ShellConfig;
use anyhow::Result;
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

/// Export executed command to shell history
pub fn export_to_history(config: &ShellConfig, command_line: &str) -> Result<()> {
    let shell_type = detect_shell(&config.shell_type);
    let history_path = get_history_path(&config.history_file, &shell_type)?;

    tracing::info!("Exporting to history: {:?}", history_path);

    // Open history file in append mode
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&history_path)?;

    // Format based on shell type
    let entry = match shell_type.as_str() {
        "zsh" => {
            // Zsh uses extended history format
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            format!(": {}:0;{}\n", timestamp, command_line)
        }
        "fish" => {
            // Fish uses a different format
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            format!("- cmd: {}\n  when: {}\n", command_line, timestamp)
        }
        _ => {
            // Bash and others use simple format
            format!("{}\n", command_line)
        }
    };

    file.write_all(entry.as_bytes())?;

    // Add comment marker for traceability
    let marker = format!("# via quocli\n");
    file.write_all(marker.as_bytes())?;

    Ok(())
}

/// Detect the current shell type
fn detect_shell(configured: &str) -> String {
    if configured != "auto" {
        return configured.to_string();
    }

    // Try to detect from SHELL environment variable
    if let Ok(shell) = env::var("SHELL") {
        if shell.contains("zsh") {
            return "zsh".to_string();
        } else if shell.contains("fish") {
            return "fish".to_string();
        } else if shell.contains("bash") {
            return "bash".to_string();
        }
    }

    // Default to bash
    "bash".to_string()
}

/// Get the history file path
fn get_history_path(configured: &str, shell_type: &str) -> Result<PathBuf> {
    if configured != "auto" {
        return Ok(PathBuf::from(shellexpand::tilde(configured).to_string()));
    }

    let home = env::var("HOME")?;

    let path = match shell_type {
        "zsh" => format!("{}/.zsh_history", home),
        "fish" => format!("{}/.local/share/fish/fish_history", home),
        _ => {
            // Try HISTFILE first, then default
            env::var("HISTFILE").unwrap_or_else(|_| format!("{}/.bash_history", home))
        }
    };

    Ok(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_shell_bash() {
        assert_eq!(detect_shell("bash"), "bash");
    }

    #[test]
    fn test_detect_shell_zsh() {
        assert_eq!(detect_shell("zsh"), "zsh");
    }
}
