/// Generate the system prompt for spec generation
pub fn spec_generation_system_prompt() -> String {
    r#"You are a CLI command parser. Your task is to analyze command-line help text and extract structured information about the command's options and arguments.

You must respond with valid JSON only, no markdown formatting or explanation.

Guidelines for parsing:
- Identify all command-line flags (both short like -v and long like --verbose)
- Determine the argument type for each flag: bool, string, int, float, path, or enum
- Mark flags that typically contain secrets/tokens/passwords as sensitive: true
- Identify conflicting flags (e.g., --quiet vs --verbose)
- Identify flags that require other flags to be set
- For repeatable flags (can be specified multiple times), set repeatable: true
- Assess danger_level based on potential for data loss or system damage:
  - low: read-only or safe operations
  - medium: writes data but with safeguards
  - high: can delete/overwrite data
  - critical: can cause system-wide damage (dd, rm -rf, mkfs)
- Extract positional arguments
- Note any subcommands mentioned"#.to_string()
}

/// Generate the user prompt for spec generation
pub fn spec_generation_user_prompt(command: &str, subcommands: &[String], help_text: &str, help_hash: &str) -> String {
    let full_command = if subcommands.is_empty() {
        command.to_string()
    } else {
        format!("{} {}", command, subcommands.join(" "))
    };

    format!(r#"Parse this command's help text into a structured specification.

COMMAND: {full_command}

HELP TEXT:
{help_text}

Return a JSON object with this exact structure:
{{
  "command": "{command}",
  "version_hash": "{help_hash}",
  "description": "Brief description of what the command does",
  "options": [
    {{
      "flags": ["-X", "--request"],
      "description": "Description of what this flag does",
      "argument_type": "string",
      "argument_name": "METHOD",
      "required": false,
      "sensitive": false,
      "repeatable": false,
      "conflicts_with": [],
      "requires": [],
      "default": null,
      "enum_values": []
    }}
  ],
  "positional_args": [
    {{
      "name": "ARG_NAME",
      "description": "Description",
      "required": true,
      "sensitive": false,
      "argument_type": "string",
      "default": null
    }}
  ],
  "subcommands": [],
  "danger_level": "low",
  "examples": []
}}

Important:
- For boolean flags (no argument), use argument_type: "bool" and argument_name: null
- For enum types, populate enum_values with the allowed values
- Common sensitive patterns: token, key, password, secret, auth, credential
- danger_level should be "critical" for commands like dd, rm, mkfs, fdisk

Respond with only the JSON object, no other text."#)
}

/// Generate context for chat interactions
pub fn chat_context(command: &str, spec_summary: &str, current_values: &str) -> String {
    format!(r#"You are helping a user construct a command-line invocation.

Current command: {command}

Available options:
{spec_summary}

Current form values:
{current_values}

Help the user understand the command options and suggest appropriate values. You can:
- Explain what specific flags do
- Suggest flag combinations for common tasks
- Warn about potentially dangerous options
- Help fill in the form based on the user's natural language request

Keep responses concise and focused on the CLI command at hand."#)
}
