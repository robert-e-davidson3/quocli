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
