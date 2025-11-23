/// Get detailed info for a single option
pub fn option_detail_system_prompt() -> String {
    r#"You are a CLI command parser. Extract detailed information about a specific command-line option.

Respond with valid JSON only, no markdown formatting."#.to_string()
}

/// User prompt: get details for one option
pub fn option_detail_user_prompt(command: &str, flags: &[String], help_text: &str, manpage_text: Option<&str>) -> String {
    let flags_str = flags.join(", ");

    let manpage_section = if let Some(manpage) = manpage_text {
        // Find the section of the manpage that's relevant to this option
        // Look for the flag in the manpage and extract surrounding context
        let relevant_section = extract_manpage_section(manpage, flags);
        if !relevant_section.is_empty() {
            format!("\n\nMANPAGE SECTION (for additional details):\n{}", relevant_section)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    format!(r#"Extract detailed information about this specific option from the documentation.

COMMAND: {command}
OPTION: {flags_str}

HELP TEXT:
{help_text}{manpage_section}

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
- description: Full description from help text AND manpage if available
- sensitive: true if this typically contains secrets/tokens/passwords
- conflicts_with: list of flags that cannot be used with this one
- requires: list of flags that must be used with this one
- enum_values: if argument_type is "enum", list allowed values
- default: default value if specified in help

Respond with only JSON, no other text."#)
}

/// Extract relevant section from manpage for a specific flag
fn extract_manpage_section(manpage: &str, flags: &[String]) -> String {
    // Find the primary flag (prefer long form)
    let search_flag = flags.iter()
        .find(|f| f.starts_with("--"))
        .or_else(|| flags.first())
        .map(|s| s.as_str())
        .unwrap_or("");

    if search_flag.is_empty() {
        return String::new();
    }

    // Find the flag in the manpage
    let lines: Vec<&str> = manpage.lines().collect();
    let mut result = Vec::new();
    let mut found = false;
    let mut blank_count = 0;

    for line in lines {
        if !found {
            // Look for the flag at the start of a line (typical manpage format)
            if line.trim().starts_with(search_flag) ||
               line.contains(&format!("{} ", search_flag)) ||
               line.contains(&format!("{},", search_flag)) {
                found = true;
                result.push(line);
                blank_count = 0;
            }
        } else {
            // Collect lines until we hit the next option or too many blank lines
            if line.trim().is_empty() {
                blank_count += 1;
                if blank_count > 1 {
                    break;
                }
                result.push(line);
            } else if line.starts_with("       -") || line.starts_with("       --") {
                // Next option started (typical manpage indentation)
                break;
            } else {
                blank_count = 0;
                result.push(line);
            }

            // Limit the section size
            if result.len() > 50 {
                break;
            }
        }
    }

    result.join("\n").trim().to_string()
}
