use serde::{Deserialize, Serialize};

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
    pub description: String,
    pub argument_type: ArgumentType,
    #[serde(default)]
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
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub enum_values: Vec<String>,
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
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ArgumentType {
    Bool,
    String,
    Int,
    Float,
    Path,
    Enum,
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
