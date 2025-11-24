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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{CommandOption, DangerLevel, OptionLevel, PositionalArg};

    // Helper to create a minimal CommandSpec
    fn create_test_spec(command: &str) -> CommandSpec {
        CommandSpec {
            command: command.to_string(),
            version_hash: "hash".to_string(),
            description: "test".to_string(),
            options: vec![],
            positional_args: vec![],
            subcommands: vec![],
            danger_level: DangerLevel::Low,
            examples: vec![],
            positionals_first: false,
        }
    }

    // Helper to create a CommandOption
    fn create_option(flags: Vec<&str>, arg_type: ArgumentType) -> CommandOption {
        CommandOption {
            flags: flags.iter().map(|s| s.to_string()).collect(),
            description: "test option".to_string(),
            argument_type: arg_type,
            argument_name: None,
            required: false,
            sensitive: false,
            repeatable: false,
            conflicts_with: vec![],
            requires: vec![],
            default: None,
            enum_values: vec![],
            level: OptionLevel::Basic,
        }
    }

    #[test]
    fn test_build_command_simple() {
        let spec = create_test_spec("ls");
        let values = HashMap::new();

        let result = build_command(&spec, &values);
        assert_eq!(result, "ls");
    }

    #[test]
    fn test_build_command_with_bool_flag_true() {
        let mut spec = create_test_spec("ls");
        spec.options.push(create_option(vec!["--all", "-a"], ArgumentType::Bool));

        let mut values = HashMap::new();
        values.insert("--all".to_string(), "true".to_string());

        let result = build_command(&spec, &values);
        assert_eq!(result, "ls --all");
    }

    #[test]
    fn test_build_command_with_bool_flag_false() {
        let mut spec = create_test_spec("ls");
        spec.options.push(create_option(vec!["--all", "-a"], ArgumentType::Bool));

        let mut values = HashMap::new();
        values.insert("--all".to_string(), "false".to_string());

        let result = build_command(&spec, &values);
        // False bool flags should not appear in command
        assert_eq!(result, "ls");
    }

    #[test]
    fn test_build_command_with_string_option() {
        let mut spec = create_test_spec("grep");
        spec.options.push(create_option(vec!["--pattern", "-e"], ArgumentType::String));

        let mut values = HashMap::new();
        values.insert("--pattern".to_string(), "foo".to_string());

        let result = build_command(&spec, &values);
        assert_eq!(result, "grep --pattern foo");
    }

    #[test]
    fn test_build_command_with_string_containing_spaces() {
        let mut spec = create_test_spec("grep");
        spec.options.push(create_option(vec!["--pattern"], ArgumentType::String));

        let mut values = HashMap::new();
        values.insert("--pattern".to_string(), "hello world".to_string());

        let result = build_command(&spec, &values);
        assert_eq!(result, "grep --pattern \"hello world\"");
    }

    #[test]
    fn test_build_command_with_path_option() {
        let mut spec = create_test_spec("cat");
        spec.options.push(create_option(vec!["--output", "-o"], ArgumentType::Path));

        let mut values = HashMap::new();
        values.insert("--output".to_string(), "/tmp/out.txt".to_string());

        let result = build_command(&spec, &values);
        assert_eq!(result, "cat --output /tmp/out.txt");
    }

    #[test]
    fn test_build_command_with_path_containing_spaces() {
        let mut spec = create_test_spec("cat");
        spec.options.push(create_option(vec!["--output"], ArgumentType::Path));

        let mut values = HashMap::new();
        values.insert("--output".to_string(), "/path/with spaces/file.txt".to_string());

        let result = build_command(&spec, &values);
        assert_eq!(result, "cat --output \"/path/with spaces/file.txt\"");
    }

    #[test]
    fn test_build_command_with_tilde_expansion() {
        let mut spec = create_test_spec("cat");
        spec.options.push(create_option(vec!["--output"], ArgumentType::Path));

        let mut values = HashMap::new();
        values.insert("--output".to_string(), "~/file.txt".to_string());

        let result = build_command(&spec, &values);
        // Tilde should be expanded to home directory
        assert!(result.contains("/file.txt"));
        assert!(!result.contains("~"));
    }

    #[test]
    fn test_build_command_with_int_option() {
        let mut spec = create_test_spec("head");
        spec.options.push(create_option(vec!["--lines", "-n"], ArgumentType::Int));

        let mut values = HashMap::new();
        values.insert("--lines".to_string(), "10".to_string());

        let result = build_command(&spec, &values);
        assert_eq!(result, "head --lines 10");
    }

    #[test]
    fn test_build_command_with_float_option() {
        let mut spec = create_test_spec("test");
        spec.options.push(create_option(vec!["--scale"], ArgumentType::Float));

        let mut values = HashMap::new();
        values.insert("--scale".to_string(), "1.5".to_string());

        let result = build_command(&spec, &values);
        assert_eq!(result, "test --scale 1.5");
    }

    #[test]
    fn test_build_command_with_enum_option() {
        let mut spec = create_test_spec("test");
        let mut opt = create_option(vec!["--color"], ArgumentType::Enum);
        opt.enum_values = vec!["auto".to_string(), "always".to_string(), "never".to_string()];
        spec.options.push(opt);

        let mut values = HashMap::new();
        values.insert("--color".to_string(), "always".to_string());

        let result = build_command(&spec, &values);
        assert_eq!(result, "test --color always");
    }

    #[test]
    fn test_build_command_with_positional_arg() {
        let mut spec = create_test_spec("cat");
        spec.positional_args.push(PositionalArg {
            name: "file".to_string(),
            description: "File to read".to_string(),
            required: true,
            sensitive: false,
            argument_type: ArgumentType::Path,
            default: None,
        });

        let mut values = HashMap::new();
        values.insert("_pos_file".to_string(), "/tmp/input.txt".to_string());

        let result = build_command(&spec, &values);
        assert_eq!(result, "cat /tmp/input.txt");
    }

    #[test]
    fn test_build_command_with_multiple_positional_args() {
        let mut spec = create_test_spec("cp");
        // Names are sorted alphabetically, so use names that sort correctly
        spec.positional_args.push(PositionalArg {
            name: "1_source".to_string(),
            description: "Source file".to_string(),
            required: true,
            sensitive: false,
            argument_type: ArgumentType::Path,
            default: None,
        });
        spec.positional_args.push(PositionalArg {
            name: "2_dest".to_string(),
            description: "Destination".to_string(),
            required: true,
            sensitive: false,
            argument_type: ArgumentType::Path,
            default: None,
        });

        let mut values = HashMap::new();
        values.insert("_pos_1_source".to_string(), "/tmp/a.txt".to_string());
        values.insert("_pos_2_dest".to_string(), "/tmp/b.txt".to_string());

        let result = build_command(&spec, &values);
        assert_eq!(result, "cp /tmp/a.txt /tmp/b.txt");
    }

    #[test]
    fn test_build_command_positional_with_spaces() {
        let mut spec = create_test_spec("cat");
        spec.positional_args.push(PositionalArg {
            name: "file".to_string(),
            description: "File".to_string(),
            required: true,
            sensitive: false,
            argument_type: ArgumentType::String,
            default: None,
        });

        let mut values = HashMap::new();
        values.insert("_pos_file".to_string(), "my file.txt".to_string());

        let result = build_command(&spec, &values);
        assert_eq!(result, "cat \"my file.txt\"");
    }

    #[test]
    fn test_build_command_flags_before_positionals() {
        let mut spec = create_test_spec("ls");
        spec.options.push(create_option(vec!["--all"], ArgumentType::Bool));
        spec.positional_args.push(PositionalArg {
            name: "dir".to_string(),
            description: "Directory".to_string(),
            required: false,
            sensitive: false,
            argument_type: ArgumentType::Path,
            default: None,
        });
        spec.positionals_first = false;

        let mut values = HashMap::new();
        values.insert("--all".to_string(), "true".to_string());
        values.insert("_pos_dir".to_string(), "/tmp".to_string());

        let result = build_command(&spec, &values);
        assert_eq!(result, "ls --all /tmp");
    }

    #[test]
    fn test_build_command_positionals_first() {
        let mut spec = create_test_spec("find");
        spec.options.push(create_option(vec!["--name"], ArgumentType::String));
        spec.positional_args.push(PositionalArg {
            name: "path".to_string(),
            description: "Path".to_string(),
            required: true,
            sensitive: false,
            argument_type: ArgumentType::Path,
            default: None,
        });
        spec.positionals_first = true;

        let mut values = HashMap::new();
        values.insert("--name".to_string(), "*.txt".to_string());
        values.insert("_pos_path".to_string(), "/home".to_string());

        let result = build_command(&spec, &values);
        assert_eq!(result, "find /home --name *.txt");
    }

    #[test]
    fn test_build_command_multiple_flags() {
        let mut spec = create_test_spec("ls");
        spec.options.push(create_option(vec!["--all", "-a"], ArgumentType::Bool));
        spec.options.push(create_option(vec!["--long", "-l"], ArgumentType::Bool));
        spec.options.push(create_option(vec!["--human-readable", "-h"], ArgumentType::Bool));

        let mut values = HashMap::new();
        values.insert("--all".to_string(), "true".to_string());
        values.insert("--long".to_string(), "true".to_string());
        values.insert("--human-readable".to_string(), "true".to_string());

        let result = build_command(&spec, &values);
        assert!(result.contains("--all"));
        assert!(result.contains("--long"));
        assert!(result.contains("--human-readable"));
    }

    #[test]
    fn test_build_command_empty_values_ignored() {
        let mut spec = create_test_spec("grep");
        spec.options.push(create_option(vec!["--pattern"], ArgumentType::String));
        spec.options.push(create_option(vec!["--file"], ArgumentType::Path));

        let mut values = HashMap::new();
        values.insert("--pattern".to_string(), "foo".to_string());
        values.insert("--file".to_string(), "".to_string()); // empty

        let result = build_command(&spec, &values);
        assert_eq!(result, "grep --pattern foo");
    }

    #[test]
    fn test_build_command_uses_primary_flag() {
        let mut spec = create_test_spec("ls");
        // Primary flag is the longest one
        spec.options.push(create_option(vec!["--all", "-a"], ArgumentType::Bool));

        let mut values = HashMap::new();
        values.insert("--all".to_string(), "true".to_string());

        let result = build_command(&spec, &values);
        // Should use --all, not -a
        assert_eq!(result, "ls --all");
    }

    #[test]
    fn test_build_command_complex_scenario() {
        let mut spec = create_test_spec("curl");
        spec.options.push(create_option(vec!["--request", "-X"], ArgumentType::String));
        spec.options.push(create_option(vec!["--header", "-H"], ArgumentType::String));
        spec.options.push(create_option(vec!["--data", "-d"], ArgumentType::String));
        spec.options.push(create_option(vec!["--output", "-o"], ArgumentType::Path));
        spec.positional_args.push(PositionalArg {
            name: "url".to_string(),
            description: "URL".to_string(),
            required: true,
            sensitive: false,
            argument_type: ArgumentType::String,
            default: None,
        });

        let mut values = HashMap::new();
        values.insert("--request".to_string(), "POST".to_string());
        values.insert("--header".to_string(), "Content-Type: application/json".to_string());
        values.insert("--data".to_string(), "{\"key\": \"value\"}".to_string());
        values.insert("_pos_url".to_string(), "https://api.example.com".to_string());

        let result = build_command(&spec, &values);
        assert!(result.starts_with("curl"));
        assert!(result.contains("--request POST"));
        assert!(result.contains("--header \"Content-Type: application/json\""));
        assert!(result.contains("https://api.example.com"));
    }

    #[test]
    fn test_build_command_positional_ordering() {
        let mut spec = create_test_spec("test");
        spec.positional_args.push(PositionalArg {
            name: "aaa".to_string(),
            description: "First".to_string(),
            required: true,
            sensitive: false,
            argument_type: ArgumentType::String,
            default: None,
        });
        spec.positional_args.push(PositionalArg {
            name: "bbb".to_string(),
            description: "Second".to_string(),
            required: true,
            sensitive: false,
            argument_type: ArgumentType::String,
            default: None,
        });
        spec.positional_args.push(PositionalArg {
            name: "ccc".to_string(),
            description: "Third".to_string(),
            required: true,
            sensitive: false,
            argument_type: ArgumentType::String,
            default: None,
        });

        let mut values = HashMap::new();
        values.insert("_pos_ccc".to_string(), "third".to_string());
        values.insert("_pos_aaa".to_string(), "first".to_string());
        values.insert("_pos_bbb".to_string(), "second".to_string());

        let result = build_command(&spec, &values);
        // Should be sorted by key name
        assert_eq!(result, "test first second third");
    }

    #[tokio::test]
    async fn test_execute_simple_command() {
        let result = execute("echo hello").await.unwrap();
        assert_eq!(result.code, Some(0));
    }

    #[tokio::test]
    async fn test_execute_command_with_args() {
        let result = execute("echo hello world").await.unwrap();
        assert_eq!(result.code, Some(0));
    }

    #[tokio::test]
    async fn test_execute_command_with_quoted_args() {
        let result = execute("echo \"hello world\"").await.unwrap();
        assert_eq!(result.code, Some(0));
    }

    #[tokio::test]
    async fn test_execute_empty_command_fails() {
        let result = execute("").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_nonexistent_command_fails() {
        let result = execute("nonexistent_command_12345").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_command_exit_code() {
        // true command always exits with 0
        let result = execute("true").await.unwrap();
        assert_eq!(result.code, Some(0));

        // false command always exits with 1
        let result = execute("false").await.unwrap();
        assert_eq!(result.code, Some(1));
    }

    #[test]
    fn test_build_command_env_var_in_value() {
        let mut spec = create_test_spec("echo");
        spec.positional_args.push(PositionalArg {
            name: "text".to_string(),
            description: "Text".to_string(),
            required: true,
            sensitive: false,
            argument_type: ArgumentType::String,
            default: None,
        });

        // Set a test env var
        std::env::set_var("TEST_BUILD_VAR", "resolved");

        let mut values = HashMap::new();
        values.insert("_pos_text".to_string(), "$TEST_BUILD_VAR".to_string());

        let result = build_command(&spec, &values);
        assert_eq!(result, "echo resolved");

        std::env::remove_var("TEST_BUILD_VAR");
    }
}
