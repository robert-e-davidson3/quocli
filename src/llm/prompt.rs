/// Get detailed info for a single option
pub fn option_detail_system_prompt() -> String {
    r#"You are a CLI command parser. Extract detailed information about a specific command-line option.

Respond with valid JSON only, no markdown formatting."#.to_string()
}

/// User prompt: get details for one option
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
