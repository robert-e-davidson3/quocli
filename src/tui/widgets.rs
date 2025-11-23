use crate::parser::{ArgumentType, CommandOption, PositionalArg};
use std::collections::HashMap;

/// Form field representing a single input
#[derive(Debug, Clone)]
pub struct FormField {
    pub id: String,
    pub label: String,
    pub description: String,
    pub field_type: ArgumentType,
    pub required: bool,
    pub sensitive: bool,
    pub value: String,
    pub enum_values: Vec<String>,
    pub default: Option<String>,
}

impl FormField {
    pub fn from_option(opt: &CommandOption) -> Self {
        let id = opt.primary_flag().to_string();
        let label = if let Some(short) = opt.short_flag() {
            format!("{}, {}", short, opt.primary_flag())
        } else {
            opt.primary_flag().to_string()
        };

        Self {
            id,
            label,
            description: opt.description.clone(),
            field_type: opt.argument_type.clone(),
            required: opt.required,
            sensitive: opt.sensitive,
            value: opt.default.clone().unwrap_or_default(),
            enum_values: opt.enum_values.clone(),
            default: opt.default.clone(),
        }
    }

    pub fn from_positional(arg: &PositionalArg) -> Self {
        Self {
            id: format!("_pos_{}", arg.name),
            label: arg.name.clone(),
            description: arg.description.clone(),
            field_type: arg.argument_type.clone(),
            required: arg.required,
            sensitive: arg.sensitive,
            value: arg.default.clone().unwrap_or_default(),
            enum_values: vec![],
            default: arg.default.clone(),
        }
    }

    /// Get display value (masked for sensitive)
    pub fn display_value(&self) -> String {
        if self.sensitive && !self.value.is_empty() {
            "*".repeat(self.value.len().min(20))
        } else if self.value.is_empty() {
            if let Some(default) = &self.default {
                format!("(default: {})", default)
            } else {
                String::new()
            }
        } else {
            self.value.clone()
        }
    }
}

/// Form state
pub struct FormState {
    pub fields: Vec<FormField>,
    pub selected: usize,
    pub editing: bool,
    pub cursor_pos: usize,
}

impl FormState {
    pub fn new(fields: Vec<FormField>) -> Self {
        Self {
            fields,
            selected: 0,
            editing: false,
            cursor_pos: 0,
        }
    }

    pub fn current_field(&self) -> Option<&FormField> {
        self.fields.get(self.selected)
    }

    pub fn current_field_mut(&mut self) -> Option<&mut FormField> {
        self.fields.get_mut(self.selected)
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected < self.fields.len().saturating_sub(1) {
            self.selected += 1;
        }
    }

    pub fn start_editing(&mut self) {
        self.editing = true;
        if let Some(field) = self.current_field() {
            self.cursor_pos = field.value.len();
        }
    }

    pub fn stop_editing(&mut self) {
        self.editing = false;
    }

    pub fn insert_char(&mut self, c: char) {
        let pos = self.cursor_pos;
        if let Some(field) = self.current_field_mut() {
            field.value.insert(pos, c);
        }
        self.cursor_pos += 1;
    }

    pub fn delete_char(&mut self) {
        let pos = self.cursor_pos;
        if pos > 0 {
            if let Some(field) = self.current_field_mut() {
                field.value.remove(pos - 1);
            }
            self.cursor_pos -= 1;
        }
    }

    pub fn toggle_bool(&mut self) {
        if let Some(field) = self.current_field_mut() {
            if field.field_type == ArgumentType::Bool {
                field.value = if field.value == "true" {
                    "false".to_string()
                } else {
                    "true".to_string()
                };
            }
        }
    }

    pub fn cycle_enum(&mut self) {
        if let Some(field) = self.current_field_mut() {
            if field.field_type == ArgumentType::Enum && !field.enum_values.is_empty() {
                let current_idx = field
                    .enum_values
                    .iter()
                    .position(|v| v == &field.value)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % field.enum_values.len();
                field.value = field.enum_values[next_idx].clone();
            }
        }
    }

    /// Get all values as a HashMap
    pub fn get_values(&self) -> HashMap<String, String> {
        self.fields
            .iter()
            .filter(|f| !f.value.is_empty())
            .map(|f| (f.id.clone(), f.value.clone()))
            .collect()
    }

    /// Load cached values
    pub fn load_cached_values(&mut self, cached: &HashMap<String, String>) {
        for field in &mut self.fields {
            if let Some(value) = cached.get(&field.id) {
                field.value = value.clone();
            }
        }
    }
}
