use serde::{Deserialize, Deserializer, Serialize};

/// Custom deserializer for Option<String> that handles LLM returning boolean/number instead of null
fn deserialize_optional_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct OptionalStringVisitor;

    impl<'de> Visitor<'de> for OptionalStringVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("null, string, boolean, or number")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            // LLM sometimes returns false instead of null - treat as None
            // If it's true, could be interpreted as "true" string
            if v {
                Ok(Some("true".to_string()))
            } else {
                Ok(None)
            }
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v.to_string()))
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v.to_string()))
        }

        fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v.to_string()))
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v.is_empty() {
                Ok(None)
            } else {
                Ok(Some(v.to_string()))
            }
        }

        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v.is_empty() {
                Ok(None)
            } else {
                Ok(Some(v))
            }
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(OptionalStringVisitor)
        }
    }

    deserializer.deserialize_any(OptionalStringVisitor)
}

/// Custom deserializer for String that handles LLM returning boolean/number instead of string
fn deserialize_flexible_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct FlexibleStringVisitor;

    impl<'de> Visitor<'de> for FlexibleStringVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string, boolean, or number")
        }

        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v.to_string())
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v.to_string())
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v.to_string())
        }

        fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v.to_string())
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v.to_string())
        }

        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v)
        }
    }

    deserializer.deserialize_any(FlexibleStringVisitor)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSpec {
    pub command: String,
    pub version_hash: String,
    pub description: String,
    pub options: Vec<CommandOption>,
    pub positional_args: Vec<PositionalArg>,
    pub subcommands: Vec<String>,
    pub danger_level: DangerLevel,
    pub examples: Vec<String>,
    /// Whether positional args come before flags (e.g., `find /path -name`)
    /// Default is false (standard: `command [flags] <positionals>`)
    #[serde(default)]
    pub positionals_first: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOption {
    pub flags: Vec<String>,
    #[serde(deserialize_with = "deserialize_flexible_string")]
    pub description: String,
    pub argument_type: ArgumentType,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub argument_name: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub repeatable: bool,
    #[serde(default)]
    pub conflicts_with: Vec<String>,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub default: Option<String>,
    #[serde(default)]
    pub enum_values: Vec<String>,
    #[serde(default)]
    pub level: OptionLevel,
}

impl CommandOption {
    /// Get the primary flag name (longest one, typically --long-form)
    pub fn primary_flag(&self) -> &str {
        self.flags
            .iter()
            .max_by_key(|f| f.len())
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    /// Get the short flag if available
    pub fn short_flag(&self) -> Option<&str> {
        self.flags
            .iter()
            .find(|f| f.starts_with('-') && !f.starts_with("--"))
            .map(|s| s.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionalArg {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub argument_type: ArgumentType,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentType {
    Bool,
    String,
    Int,
    Float,
    Path,
    Enum,
}

// Custom deserializer to handle LLM variations like "file" -> "path"
impl<'de> serde::Deserialize<'de> for ArgumentType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = std::string::String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "bool" | "boolean" | "flag" => Ok(ArgumentType::Bool),
            "string" | "str" | "text" => Ok(ArgumentType::String),
            "int" | "integer" | "number" => Ok(ArgumentType::Int),
            "float" | "decimal" | "double" => Ok(ArgumentType::Float),
            "path" | "file" | "filename" | "filepath" | "directory" | "dir" => Ok(ArgumentType::Path),
            "enum" | "choice" | "select" | "option" => Ok(ArgumentType::Enum),
            _ => Ok(ArgumentType::String), // Default to string for unknown types
        }
    }
}

impl serde::Serialize for ArgumentType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            ArgumentType::Bool => "bool",
            ArgumentType::String => "string",
            ArgumentType::Int => "int",
            ArgumentType::Float => "float",
            ArgumentType::Path => "path",
            ArgumentType::Enum => "enum",
        };
        serializer.serialize_str(s)
    }
}

impl Default for ArgumentType {
    fn default() -> Self {
        ArgumentType::String
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DangerLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Level indicating how commonly used an option is
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OptionLevel {
    /// Common options shown in basic --help
    Basic,
    /// Advanced options from --help all or manpage
    Advanced,
}

impl Default for OptionLevel {
    fn default() -> Self {
        OptionLevel::Basic
    }
}

impl Default for DangerLevel {
    fn default() -> Self {
        DangerLevel::Low
    }
}

impl std::fmt::Display for DangerLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DangerLevel::Low => write!(f, "low"),
            DangerLevel::Medium => write!(f, "medium"),
            DangerLevel::High => write!(f, "high"),
            DangerLevel::Critical => write!(f, "critical"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argument_type_deserialize_bool() {
        let cases = ["\"bool\"", "\"boolean\"", "\"flag\""];
        for case in cases {
            let result: ArgumentType = serde_json::from_str(case).unwrap();
            assert_eq!(result, ArgumentType::Bool, "Failed for {}", case);
        }
    }

    #[test]
    fn test_argument_type_deserialize_string() {
        let cases = ["\"string\"", "\"str\"", "\"text\""];
        for case in cases {
            let result: ArgumentType = serde_json::from_str(case).unwrap();
            assert_eq!(result, ArgumentType::String, "Failed for {}", case);
        }
    }

    #[test]
    fn test_argument_type_deserialize_int() {
        let cases = ["\"int\"", "\"integer\"", "\"number\""];
        for case in cases {
            let result: ArgumentType = serde_json::from_str(case).unwrap();
            assert_eq!(result, ArgumentType::Int, "Failed for {}", case);
        }
    }

    #[test]
    fn test_argument_type_deserialize_float() {
        let cases = ["\"float\"", "\"decimal\"", "\"double\""];
        for case in cases {
            let result: ArgumentType = serde_json::from_str(case).unwrap();
            assert_eq!(result, ArgumentType::Float, "Failed for {}", case);
        }
    }

    #[test]
    fn test_argument_type_deserialize_path() {
        let cases = ["\"path\"", "\"file\"", "\"filename\"", "\"filepath\"", "\"directory\"", "\"dir\""];
        for case in cases {
            let result: ArgumentType = serde_json::from_str(case).unwrap();
            assert_eq!(result, ArgumentType::Path, "Failed for {}", case);
        }
    }

    #[test]
    fn test_argument_type_deserialize_enum() {
        let cases = ["\"enum\"", "\"choice\"", "\"select\"", "\"option\""];
        for case in cases {
            let result: ArgumentType = serde_json::from_str(case).unwrap();
            assert_eq!(result, ArgumentType::Enum, "Failed for {}", case);
        }
    }

    #[test]
    fn test_argument_type_deserialize_unknown_defaults_to_string() {
        let result: ArgumentType = serde_json::from_str("\"unknown_type\"").unwrap();
        assert_eq!(result, ArgumentType::String);
    }

    #[test]
    fn test_argument_type_case_insensitive() {
        let result: ArgumentType = serde_json::from_str("\"BOOL\"").unwrap();
        assert_eq!(result, ArgumentType::Bool);

        let result: ArgumentType = serde_json::from_str("\"Path\"").unwrap();
        assert_eq!(result, ArgumentType::Path);
    }

    #[test]
    fn test_argument_type_serialize() {
        assert_eq!(serde_json::to_string(&ArgumentType::Bool).unwrap(), "\"bool\"");
        assert_eq!(serde_json::to_string(&ArgumentType::String).unwrap(), "\"string\"");
        assert_eq!(serde_json::to_string(&ArgumentType::Int).unwrap(), "\"int\"");
        assert_eq!(serde_json::to_string(&ArgumentType::Float).unwrap(), "\"float\"");
        assert_eq!(serde_json::to_string(&ArgumentType::Path).unwrap(), "\"path\"");
        assert_eq!(serde_json::to_string(&ArgumentType::Enum).unwrap(), "\"enum\"");
    }

    #[test]
    fn test_danger_level_display() {
        assert_eq!(DangerLevel::Low.to_string(), "low");
        assert_eq!(DangerLevel::Medium.to_string(), "medium");
        assert_eq!(DangerLevel::High.to_string(), "high");
        assert_eq!(DangerLevel::Critical.to_string(), "critical");
    }

    #[test]
    fn test_danger_level_deserialize() {
        let result: DangerLevel = serde_json::from_str("\"low\"").unwrap();
        assert_eq!(result, DangerLevel::Low);

        let result: DangerLevel = serde_json::from_str("\"high\"").unwrap();
        assert_eq!(result, DangerLevel::High);
    }

    #[test]
    fn test_option_level_default() {
        let level = OptionLevel::default();
        assert_eq!(level, OptionLevel::Basic);
    }

    #[test]
    fn test_command_option_primary_flag() {
        let opt = CommandOption {
            flags: vec!["--verbose".to_string(), "-v".to_string()],
            description: "test".to_string(),
            argument_type: ArgumentType::Bool,
            argument_name: None,
            required: false,
            sensitive: false,
            repeatable: false,
            conflicts_with: vec![],
            requires: vec![],
            default: None,
            enum_values: vec![],
            level: OptionLevel::Basic,
        };

        // Primary flag should be the longest
        assert_eq!(opt.primary_flag(), "--verbose");
    }

    #[test]
    fn test_command_option_short_flag() {
        let opt = CommandOption {
            flags: vec!["--verbose".to_string(), "-v".to_string()],
            description: "test".to_string(),
            argument_type: ArgumentType::Bool,
            argument_name: None,
            required: false,
            sensitive: false,
            repeatable: false,
            conflicts_with: vec![],
            requires: vec![],
            default: None,
            enum_values: vec![],
            level: OptionLevel::Basic,
        };

        assert_eq!(opt.short_flag(), Some("-v"));
    }

    #[test]
    fn test_command_option_no_short_flag() {
        let opt = CommandOption {
            flags: vec!["--verbose".to_string()],
            description: "test".to_string(),
            argument_type: ArgumentType::Bool,
            argument_name: None,
            required: false,
            sensitive: false,
            repeatable: false,
            conflicts_with: vec![],
            requires: vec![],
            default: None,
            enum_values: vec![],
            level: OptionLevel::Basic,
        };

        assert_eq!(opt.short_flag(), None);
    }

    #[test]
    fn test_optional_string_deserializer_with_string() {
        let json = r#"{"argument_name": "FILE"}"#;
        #[derive(Deserialize)]
        struct Test {
            #[serde(default, deserialize_with = "super::deserialize_optional_string")]
            argument_name: Option<String>,
        }
        let result: Test = serde_json::from_str(json).unwrap();
        assert_eq!(result.argument_name, Some("FILE".to_string()));
    }

    #[test]
    fn test_optional_string_deserializer_with_null() {
        let json = r#"{"argument_name": null}"#;
        #[derive(Deserialize)]
        struct Test {
            #[serde(default, deserialize_with = "super::deserialize_optional_string")]
            argument_name: Option<String>,
        }
        let result: Test = serde_json::from_str(json).unwrap();
        assert_eq!(result.argument_name, None);
    }

    #[test]
    fn test_optional_string_deserializer_with_false() {
        // LLM sometimes returns false instead of null
        let json = r#"{"argument_name": false}"#;
        #[derive(Deserialize)]
        struct Test {
            #[serde(default, deserialize_with = "super::deserialize_optional_string")]
            argument_name: Option<String>,
        }
        let result: Test = serde_json::from_str(json).unwrap();
        assert_eq!(result.argument_name, None);
    }

    #[test]
    fn test_optional_string_deserializer_with_true() {
        let json = r#"{"argument_name": true}"#;
        #[derive(Deserialize)]
        struct Test {
            #[serde(default, deserialize_with = "super::deserialize_optional_string")]
            argument_name: Option<String>,
        }
        let result: Test = serde_json::from_str(json).unwrap();
        assert_eq!(result.argument_name, Some("true".to_string()));
    }

    #[test]
    fn test_optional_string_deserializer_with_number() {
        let json = r#"{"argument_name": 42}"#;
        #[derive(Deserialize)]
        struct Test {
            #[serde(default, deserialize_with = "super::deserialize_optional_string")]
            argument_name: Option<String>,
        }
        let result: Test = serde_json::from_str(json).unwrap();
        assert_eq!(result.argument_name, Some("42".to_string()));
    }

    #[test]
    fn test_optional_string_deserializer_with_empty_string() {
        let json = r#"{"argument_name": ""}"#;
        #[derive(Deserialize)]
        struct Test {
            #[serde(default, deserialize_with = "super::deserialize_optional_string")]
            argument_name: Option<String>,
        }
        let result: Test = serde_json::from_str(json).unwrap();
        assert_eq!(result.argument_name, None);
    }

    #[test]
    fn test_flexible_string_deserializer_with_string() {
        let json = r#"{"description": "test"}"#;
        #[derive(Deserialize)]
        struct Test {
            #[serde(deserialize_with = "super::deserialize_flexible_string")]
            description: String,
        }
        let result: Test = serde_json::from_str(json).unwrap();
        assert_eq!(result.description, "test");
    }

    #[test]
    fn test_flexible_string_deserializer_with_bool() {
        let json = r#"{"description": true}"#;
        #[derive(Deserialize)]
        struct Test {
            #[serde(deserialize_with = "super::deserialize_flexible_string")]
            description: String,
        }
        let result: Test = serde_json::from_str(json).unwrap();
        assert_eq!(result.description, "true");
    }

    #[test]
    fn test_flexible_string_deserializer_with_int() {
        let json = r#"{"description": 123}"#;
        #[derive(Deserialize)]
        struct Test {
            #[serde(deserialize_with = "super::deserialize_flexible_string")]
            description: String,
        }
        let result: Test = serde_json::from_str(json).unwrap();
        assert_eq!(result.description, "123");
    }

    #[test]
    fn test_flexible_string_deserializer_with_float() {
        let json = r#"{"description": 3.14}"#;
        #[derive(Deserialize)]
        struct Test {
            #[serde(deserialize_with = "super::deserialize_flexible_string")]
            description: String,
        }
        let result: Test = serde_json::from_str(json).unwrap();
        assert_eq!(result.description, "3.14");
    }

    #[test]
    fn test_command_spec_deserialize() {
        let json = r#"{
            "command": "ls",
            "version_hash": "abc123",
            "description": "List directory contents",
            "options": [],
            "positional_args": [],
            "subcommands": [],
            "danger_level": "low",
            "examples": ["ls -la"]
        }"#;

        let spec: CommandSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.command, "ls");
        assert_eq!(spec.version_hash, "abc123");
        assert_eq!(spec.danger_level, DangerLevel::Low);
        assert!(!spec.positionals_first); // default
    }

    #[test]
    fn test_command_spec_with_options() {
        let json = r#"{
            "command": "grep",
            "version_hash": "hash",
            "description": "Search patterns",
            "options": [{
                "flags": ["--pattern", "-e"],
                "description": "Pattern to search",
                "argument_type": "string",
                "required": true,
                "sensitive": false
            }],
            "positional_args": [],
            "subcommands": [],
            "danger_level": "low",
            "examples": []
        }"#;

        let spec: CommandSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.options.len(), 1);
        assert_eq!(spec.options[0].flags, vec!["--pattern", "-e"]);
        assert!(spec.options[0].required);
    }

    #[test]
    fn test_command_spec_with_positional_args() {
        let json = r#"{
            "command": "cat",
            "version_hash": "hash",
            "description": "Concatenate files",
            "options": [],
            "positional_args": [{
                "name": "file",
                "description": "File to read",
                "required": true,
                "argument_type": "path"
            }],
            "subcommands": [],
            "danger_level": "low",
            "examples": []
        }"#;

        let spec: CommandSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.positional_args.len(), 1);
        assert_eq!(spec.positional_args[0].name, "file");
        assert_eq!(spec.positional_args[0].argument_type, ArgumentType::Path);
    }

    #[test]
    fn test_command_option_with_enum_values() {
        let json = r#"{
            "flags": ["--color"],
            "description": "Colorize output",
            "argument_type": "enum",
            "enum_values": ["auto", "always", "never"]
        }"#;

        let opt: CommandOption = serde_json::from_str(json).unwrap();
        assert_eq!(opt.argument_type, ArgumentType::Enum);
        assert_eq!(opt.enum_values, vec!["auto", "always", "never"]);
    }

    #[test]
    fn test_command_option_with_conflicts_and_requires() {
        let json = r#"{
            "flags": ["--verbose"],
            "description": "Verbose output",
            "argument_type": "bool",
            "conflicts_with": ["--quiet"],
            "requires": ["--output"]
        }"#;

        let opt: CommandOption = serde_json::from_str(json).unwrap();
        assert_eq!(opt.conflicts_with, vec!["--quiet"]);
        assert_eq!(opt.requires, vec!["--output"]);
    }

    #[test]
    fn test_positional_arg_with_default() {
        let json = r#"{
            "name": "count",
            "description": "Number of lines",
            "required": false,
            "argument_type": "int",
            "default": "10"
        }"#;

        let arg: PositionalArg = serde_json::from_str(json).unwrap();
        assert_eq!(arg.default, Some("10".to_string()));
    }

    #[test]
    fn test_command_spec_roundtrip() {
        let spec = CommandSpec {
            command: "test".to_string(),
            version_hash: "hash".to_string(),
            description: "Test command".to_string(),
            options: vec![CommandOption {
                flags: vec!["--flag".to_string()],
                description: "A flag".to_string(),
                argument_type: ArgumentType::Bool,
                argument_name: None,
                required: false,
                sensitive: false,
                repeatable: false,
                conflicts_with: vec![],
                requires: vec![],
                default: None,
                enum_values: vec![],
                level: OptionLevel::Basic,
            }],
            positional_args: vec![],
            subcommands: vec!["sub1".to_string()],
            danger_level: DangerLevel::Medium,
            examples: vec!["test --flag".to_string()],
            positionals_first: true,
        };

        let serialized = serde_json::to_string(&spec).unwrap();
        let deserialized: CommandSpec = serde_json::from_str(&serialized).unwrap();

        assert_eq!(spec.command, deserialized.command);
        assert_eq!(spec.options.len(), deserialized.options.len());
        assert_eq!(spec.danger_level, deserialized.danger_level);
        assert_eq!(spec.positionals_first, deserialized.positionals_first);
    }
}
