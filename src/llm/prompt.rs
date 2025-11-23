/// Generate context for chat interactions
#[allow(dead_code)]
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

// === Two-pass batching prompts ===

/// First pass: identify all options (compact format)
pub fn options_discovery_system_prompt() -> String {
    r#"You are a CLI command parser. Extract a list of all command-line options from help text.

Respond with valid JSON only, no markdown formatting."#.to_string()
}

/// First pass user prompt: get list of options
pub fn options_discovery_user_prompt(command: &str, help_text: &str) -> String {
    format!(r#"Extract all command-line options from this help text.

COMMAND: {command}

HELP TEXT:
{help_text}

Return a JSON object with this structure:
{{
  "command": "{command}",
  "description": "Brief description of the command",
  "danger_level": "low",
  "options": [
    {{
      "flags": ["-v", "--verbose"],
      "argument_type": "bool",
      "argument_name": null
    }}
  ],
  "positional_args": [
    {{
      "name": "FILE",
      "argument_type": "path"
    }}
  ],
  "subcommands": []
}}

Guidelines:
- List ALL options found in the help text
- argument_type: bool, string, int, float, path, or enum
- argument_name: the placeholder name (e.g., "FILE", "N") or null for booleans
- danger_level: low/medium/high/critical based on potential for data loss
- For positional arguments, include name and type

Respond with only JSON, no other text."#)
}

/// Second pass: get detailed info for a single option
pub fn option_detail_system_prompt() -> String {
    r#"You are a CLI command parser. Extract detailed information about a specific command-line option.

Respond with valid JSON only, no markdown formatting."#.to_string()
}

/// Second pass user prompt: get details for one option
pub fn option_detail_user_prompt(command: &str, flags: &[String], help_text: &str) -> String {
    let flags_str = flags.join(", ");

    format!(r#"Extract detailed information about this specific option from the help text.

COMMAND: {command}
OPTION: {flags_str}

HELP TEXT:
{help_text}

Return a JSON object with this structure:
{{
  "flags": ["-v", "--verbose"],
  "description": "Detailed description of what this option does",
  "argument_type": "bool",
  "argument_name": null,
  "required": false,
  "sensitive": false,
  "repeatable": false,
  "conflicts_with": [],
  "requires": [],
  "default": null,
  "enum_values": []
}}

Guidelines:
- description: Full description from help text
- sensitive: true if this typically contains secrets/tokens/passwords
- conflicts_with: list of flags that cannot be used with this one
- requires: list of flags that must be used with this one
- enum_values: if argument_type is "enum", list allowed values
- default: default value if specified in help

Respond with only JSON, no other text."#)
}

/// Second pass: get details for a positional argument
pub fn positional_detail_user_prompt(command: &str, arg_name: &str, help_text: &str) -> String {
    format!(r#"Extract detailed information about this positional argument from the help text.

COMMAND: {command}
ARGUMENT: {arg_name}

HELP TEXT:
{help_text}

Return a JSON object with this structure:
{{
  "name": "{arg_name}",
  "description": "Detailed description",
  "required": true,
  "sensitive": false,
  "argument_type": "string",
  "default": null
}}

Respond with only JSON, no other text."#)
}
