/// Get detailed info for a single option
pub fn option_detail_system_prompt() -> String {
    r#"You are a CLI command parser. Extract detailed information about command-line options.

Respond with valid JSON only, no markdown formatting."#.to_string()
}

/// Build the cached context containing help text and manpage
pub fn build_cached_context(command: &str, help_text: &str, manpage_text: Option<&str>) -> String {
    let manpage_section = if let Some(manpage) = manpage_text {
        format!("\n\n--- MANPAGE ---\n{}", manpage)
    } else {
        String::new()
    };

    format!(r#"COMMAND: {command}

DOCUMENTATION:
{help_text}{manpage_section}"#)
}

/// User prompt for single option extraction (used with cached context)
pub fn single_option_query(flags: &[String]) -> String {
    let flags_str = flags.join(", ");

    format!(r#"Extract detailed information for this option: {}

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
  "enum_values": [],
  "level": "basic"
}}

Guidelines:
- description: Full description from the documentation above
- argument_type: "bool", "string", "int", "float", "path", or "enum"
- sensitive: true if this typically contains secrets/tokens/passwords
- conflicts_with: list of flags that cannot be used with this one
- requires: list of flags that must be used with this one
- enum_values: if argument_type is "enum", list allowed values
- default: default value if specified
- level: "basic" for common/frequently-used options, "advanced" for specialized/rarely-used options

Respond with only JSON, no other text."#,
        flags_str
    )
}

/// User prompt for single positional argument extraction (used with cached context)
pub fn single_positional_arg_query(arg_name: &str) -> String {
    format!(r#"Extract detailed information for this positional argument: {arg_name}

Return a JSON object with this structure:
{{
  "name": "{arg_name}",
  "description": "Detailed description of what this argument represents",
  "argument_type": "string",
  "required": true,
  "sensitive": false,
  "default": null
}}

Guidelines:
- description: Full description from the documentation above explaining what this argument is for
- argument_type: "bool", "string", "int", "float", "path", or "enum"
- required: true if this argument must be provided, false if optional
- sensitive: true if this typically contains secrets/tokens/passwords
- default: default value if specified in documentation

Respond with only JSON, no other text."#)
}
