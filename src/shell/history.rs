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
    use tempfile::TempDir;

    #[test]
    fn test_detect_shell_bash() {
        assert_eq!(detect_shell("bash"), "bash");
    }

    #[test]
    fn test_detect_shell_zsh() {
        assert_eq!(detect_shell("zsh"), "zsh");
    }

    #[test]
    fn test_detect_shell_fish() {
        assert_eq!(detect_shell("fish"), "fish");
    }

    #[test]
    fn test_detect_shell_auto_with_bash_env() {
        env::set_var("SHELL", "/bin/bash");
        assert_eq!(detect_shell("auto"), "bash");
    }

    #[test]
    fn test_detect_shell_auto_with_zsh_env() {
        env::set_var("SHELL", "/usr/bin/zsh");
        assert_eq!(detect_shell("auto"), "zsh");
    }

    #[test]
    fn test_detect_shell_auto_with_fish_env() {
        env::set_var("SHELL", "/usr/bin/fish");
        assert_eq!(detect_shell("auto"), "fish");
    }

    #[test]
    fn test_get_history_path_custom() {
        let result = get_history_path("~/.custom_history", "bash").unwrap();
        assert!(result.to_string_lossy().contains(".custom_history"));
    }

    #[test]
    fn test_get_history_path_auto_bash() {
        let _home = env::var("HOME").unwrap();
        let result = get_history_path("auto", "bash").unwrap();

        // Should be either HISTFILE or ~/.bash_history
        let path_str = result.to_string_lossy();
        assert!(
            path_str.contains(".bash_history") || path_str.contains("history"),
            "Path was: {}",
            path_str
        );
    }

    #[test]
    fn test_get_history_path_auto_zsh() {
        let home = env::var("HOME").unwrap();
        let result = get_history_path("auto", "zsh").unwrap();
        assert_eq!(result, PathBuf::from(format!("{}/.zsh_history", home)));
    }

    #[test]
    fn test_get_history_path_auto_fish() {
        let home = env::var("HOME").unwrap();
        let result = get_history_path("auto", "fish").unwrap();
        assert_eq!(
            result,
            PathBuf::from(format!("{}/.local/share/fish/fish_history", home))
        );
    }

    #[test]
    fn test_export_to_history_bash_format() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join(".bash_history");

        let config = ShellConfig {
            shell_type: "bash".to_string(),
            history_file: history_path.to_string_lossy().to_string(),
            export_envvars: true,
        };

        export_to_history(&config, "ls -la").unwrap();

        let content = std::fs::read_to_string(&history_path).unwrap();
        assert!(content.contains("ls -la"));
        assert!(content.contains("# via quocli"));
    }

    #[test]
    fn test_export_to_history_zsh_format() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join(".zsh_history");

        let config = ShellConfig {
            shell_type: "zsh".to_string(),
            history_file: history_path.to_string_lossy().to_string(),
            export_envvars: true,
        };

        export_to_history(&config, "echo test").unwrap();

        let content = std::fs::read_to_string(&history_path).unwrap();
        // Zsh format: ": timestamp:0;command"
        assert!(content.contains(":0;echo test"));
        assert!(content.contains("# via quocli"));
    }

    #[test]
    fn test_export_to_history_fish_format() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join("fish_history");

        let config = ShellConfig {
            shell_type: "fish".to_string(),
            history_file: history_path.to_string_lossy().to_string(),
            export_envvars: true,
        };

        export_to_history(&config, "git status").unwrap();

        let content = std::fs::read_to_string(&history_path).unwrap();
        // Fish format: "- cmd: command\n  when: timestamp"
        assert!(content.contains("- cmd: git status"));
        assert!(content.contains("when:"));
    }

    #[test]
    fn test_export_to_history_appends() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join(".bash_history");

        // Write initial content
        std::fs::write(&history_path, "existing command\n").unwrap();

        let config = ShellConfig {
            shell_type: "bash".to_string(),
            history_file: history_path.to_string_lossy().to_string(),
            export_envvars: true,
        };

        export_to_history(&config, "new command").unwrap();

        let content = std::fs::read_to_string(&history_path).unwrap();
        assert!(content.contains("existing command"));
        assert!(content.contains("new command"));
    }

    #[test]
    fn test_export_to_history_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join("new_history");

        let config = ShellConfig {
            shell_type: "bash".to_string(),
            history_file: history_path.to_string_lossy().to_string(),
            export_envvars: true,
        };

        assert!(!history_path.exists());
        export_to_history(&config, "test").unwrap();
        assert!(history_path.exists());
    }

    #[test]
    fn test_export_to_history_with_special_chars() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join(".bash_history");

        let config = ShellConfig {
            shell_type: "bash".to_string(),
            history_file: history_path.to_string_lossy().to_string(),
            export_envvars: true,
        };

        let command = r#"echo "hello world" && grep 'pattern' file.txt"#;
        export_to_history(&config, command).unwrap();

        let content = std::fs::read_to_string(&history_path).unwrap();
        assert!(content.contains(command));
    }
}
