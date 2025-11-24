use crate::parser::{ArgumentType, CommandSpec};
use crate::shell::resolve_and_convert;
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
    let mut flag_parts: Vec<String> = Vec::new();
    let mut positional_parts: Vec<String> = Vec::new();

    // Separate positional and flag values
    let mut positional_values: Vec<(String, String)> = Vec::new();
    for (key, value) in values {
        if key.starts_with("_pos_") {
            positional_values.push((key.clone(), value.clone()));
        }
    }

    // Sort positional by name to maintain order
    positional_values.sort_by(|a, b| a.0.cmp(&b.0));

    // Process options into flag_parts
    for opt in &spec.options {
        let primary = opt.primary_flag();
        if let Some(value) = values.get(primary) {
            if value.is_empty() {
                continue;
            }

            // Resolve environment variables and convert to appropriate type
            let resolved = resolve_and_convert(value, &opt.argument_type);

            match opt.argument_type {
                ArgumentType::Bool => {
                    if resolved == "true" {
                        flag_parts.push(primary.to_string());
                    }
                }
                ArgumentType::Path => {
                    flag_parts.push(primary.to_string());
                    // Expand tilde for path arguments
                    let expanded = shellexpand::tilde(&resolved).to_string();
                    if expanded.contains(' ') {
                        flag_parts.push(format!("\"{}\"", expanded));
                    } else {
                        flag_parts.push(expanded);
                    }
                }
                _ => {
                    flag_parts.push(primary.to_string());
                    // Quote values with spaces
                    if resolved.contains(' ') {
                        flag_parts.push(format!("\"{}\"", resolved));
                    } else {
                        flag_parts.push(resolved);
                    }
                }
            }
        }
    }

    // Process positional arguments into positional_parts
    for (key, value) in positional_values {
        // Check if this positional arg is a path type
        let arg_type = spec.positional_args.iter()
            .find(|a| format!("_pos_{}", a.name) == key)
            .map(|a| a.argument_type.clone())
            .unwrap_or(ArgumentType::String);

        // Resolve environment variables and convert to appropriate type
        let resolved = resolve_and_convert(&value, &arg_type);

        let final_value = if arg_type == ArgumentType::Path {
            shellexpand::tilde(&resolved).to_string()
        } else {
            resolved
        };

        if final_value.contains(' ') {
            positional_parts.push(format!("\"{}\"", final_value));
        } else {
            positional_parts.push(final_value);
        }
    }

    // Combine based on positionals_first setting
    if spec.positionals_first {
        parts.extend(positional_parts);
        parts.extend(flag_parts);
    } else {
        parts.extend(flag_parts);
        parts.extend(positional_parts);
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
