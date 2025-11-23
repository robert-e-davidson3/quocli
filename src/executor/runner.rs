use crate::parser::{ArgumentType, CommandSpec};
use crate::QuocliError;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::process::Command;

pub struct ExecutionResult {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Build the command line string from spec and values
pub fn build_command(spec: &CommandSpec, values: &HashMap<String, String>) -> String {
    let mut parts = vec![spec.command.clone()];
    let mut positional_values: Vec<(String, String)> = Vec::new();

    // Separate positional and flag values
    for (key, value) in values {
        if key.starts_with("_pos_") {
            positional_values.push((key.clone(), value.clone()));
        }
    }

    // Sort positional by name to maintain order
    positional_values.sort_by(|a, b| a.0.cmp(&b.0));

    // Process options
    for opt in &spec.options {
        let primary = opt.primary_flag();
        if let Some(value) = values.get(primary) {
            if value.is_empty() {
                continue;
            }

            match opt.argument_type {
                ArgumentType::Bool => {
                    if value == "true" {
                        parts.push(primary.to_string());
                    }
                }
                _ => {
                    parts.push(primary.to_string());
                    // Quote values with spaces
                    if value.contains(' ') {
                        parts.push(format!("\"{}\"", value));
                    } else {
                        parts.push(value.clone());
                    }
                }
            }
        }
    }

    // Add positional arguments at the end
    for (_, value) in positional_values {
        if value.contains(' ') {
            parts.push(format!("\"{}\"", value));
        } else {
            parts.push(value);
        }
    }

    parts.join(" ")
}

/// Execute a command and return the result
pub async fn execute(command_line: &str) -> Result<ExecutionResult, QuocliError> {
    tracing::info!("Executing: {}", command_line);

    // Parse the command line
    let parts: Vec<String> = shell_words::split(command_line)
        .map_err(|e| QuocliError::Execution(format!("Failed to parse command: {}", e)))?;

    if parts.is_empty() {
        return Err(QuocliError::Execution("Empty command".to_string()));
    }

    let program = &parts[0];
    let args = &parts[1..];

    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| QuocliError::Execution(format!("Failed to spawn command: {}", e)))?
        .wait()
        .await
        .map_err(|e| QuocliError::Execution(format!("Failed to wait for command: {}", e)))?;

    Ok(ExecutionResult {
        code: output.code(),
        stdout: String::new(), // Output goes directly to terminal
        stderr: String::new(),
    })
}
