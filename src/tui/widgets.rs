use crate::parser::{ArgumentType, CommandOption, PositionalArg};
use crate::shell::get_env_suggestions;
use std::collections::HashMap;

/// Tab categories for organizing options
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OptionTab {
    All,
    Frequent,
}

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
            value: String::new(),
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
            value: String::new(),
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
    // Search state
    pub search_mode: bool,
    pub search_query: String,
    pub filtered_indices: Vec<usize>,
    pub include_description: bool,
    // Tab state
    pub current_tab: OptionTab,
    pub frequent_indices: Vec<usize>, // indices of fields that have cached values
    // Env var suggestion state
    pub showing_suggestions: bool,
    pub env_suggestions: Vec<(String, String)>, // (name, value)
    pub selected_suggestion: usize,
    // Description scroll state
    pub description_scroll: u16,
}

impl FormState {
    pub fn new(fields: Vec<FormField>) -> Self {
        let indices: Vec<usize> = (0..fields.len()).collect();
        Self {
            fields,
            selected: 0,
            editing: false,
            cursor_pos: 0,
            search_mode: false,
            search_query: String::new(),
            filtered_indices: indices,
            include_description: false,
            current_tab: OptionTab::All,
            frequent_indices: Vec::new(),
            showing_suggestions: false,
            env_suggestions: Vec::new(),
            selected_suggestion: 0,
            description_scroll: 0,
        }
    }

    /// Cycle to next tab
    pub fn next_tab(&mut self) {
        self.current_tab = match self.current_tab {
            OptionTab::All => OptionTab::Frequent,
            OptionTab::Frequent => OptionTab::All,
        };
        self.apply_tab_filter();
    }

    /// Set specific tab
    pub fn set_tab(&mut self, tab: OptionTab) {
        self.current_tab = tab;
        self.apply_tab_filter();
    }

    /// Apply tab-based filtering
    fn apply_tab_filter(&mut self) {
        match self.current_tab {
            OptionTab::All => {
                self.filtered_indices = (0..self.fields.len()).collect();
            }
            OptionTab::Frequent => {
                if self.frequent_indices.is_empty() {
                    // No frequent items, show all
                    self.filtered_indices = (0..self.fields.len()).collect();
                } else {
                    self.filtered_indices = self.frequent_indices.clone();
                }
            }
        }

        // Re-apply search filter if there's an active search
        if !self.search_query.is_empty() {
            self.update_filter();
        } else {
            // Ensure selection is valid
            if !self.filtered_indices.is_empty() && !self.filtered_indices.contains(&self.selected) {
                self.selected = self.filtered_indices[0];
            }
        }
    }

    /// Start search mode
    pub fn start_search(&mut self, include_description: bool) {
        self.search_mode = true;
        self.search_query.clear();
        self.include_description = include_description;
        self.update_filter();
    }

    /// Stop search mode
    pub fn stop_search(&mut self) {
        self.search_mode = false;
    }

    /// Clear search and show all fields
    pub fn clear_search(&mut self) {
        self.search_query.clear();
        self.filtered_indices = (0..self.fields.len()).collect();
        self.search_mode = false;
        self.selected = 0;
    }

    /// Add character to search query
    pub fn search_insert_char(&mut self, c: char) {
        self.search_query.push(c);
        self.update_filter();
    }

    /// Delete character from search query
    pub fn search_delete_char(&mut self) {
        self.search_query.pop();
        self.update_filter();
    }

    /// Update filtered indices based on search query
    pub fn update_filter(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_indices = (0..self.fields.len()).collect();
        } else {
            let query = self.search_query.to_lowercase();

            // Score and sort results - prefer exact flag matches
            let mut scored: Vec<(usize, i32)> = self.fields
                .iter()
                .enumerate()
                .filter_map(|(i, field)| {
                    let label_lower = field.label.to_lowercase();
                    let id_lower = field.id.to_lowercase();
                    let desc_lower = field.description.to_lowercase();

                    // Exact flag match gets highest priority
                    if id_lower == query || label_lower.contains(&format!("{},", &query)) {
                        return Some((i, 100));
                    }

                    // Flag starts with query
                    if id_lower.starts_with(&query) || label_lower.starts_with(&query) {
                        return Some((i, 50));
                    }

                    // Flag contains query
                    if id_lower.contains(&query) || label_lower.contains(&query) {
                        return Some((i, 25));
                    }

                    // Description contains query (if enabled)
                    if self.include_description && desc_lower.contains(&query) {
                        return Some((i, 10));
                    }

                    None
                })
                .collect();

            // Sort by score descending
            scored.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered_indices = scored.into_iter().map(|(i, _)| i).collect();
        }

        // Adjust selected to stay within filtered results
        if !self.filtered_indices.is_empty() {
            // Try to keep the currently selected field if it's still in the filtered list
            if let Some(current_idx) = self.fields.get(self.selected).and_then(|_| {
                self.filtered_indices.iter().position(|&i| i == self.selected)
            }) {
                // Current selection is still visible, keep it selected in filtered view
                self.selected = self.filtered_indices[current_idx.min(self.filtered_indices.len() - 1)];
            } else {
                // Current selection is not visible, select first filtered item
                self.selected = self.filtered_indices[0];
            }
        }
    }

    /// Get visible fields (filtered)
    pub fn visible_fields(&self) -> Vec<(usize, &FormField)> {
        self.filtered_indices
            .iter()
            .map(|&i| (i, &self.fields[i]))
            .collect()
    }

    pub fn current_field(&self) -> Option<&FormField> {
        self.fields.get(self.selected)
    }

    pub fn current_field_mut(&mut self) -> Option<&mut FormField> {
        self.fields.get_mut(self.selected)
    }

    pub fn move_up(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }

        // Find current position in filtered list
        let current_pos = self.filtered_indices
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);

        if current_pos > 0 {
            self.selected = self.filtered_indices[current_pos - 1];
            self.description_scroll = 0; // Reset scroll when changing field
        }
    }

    pub fn move_down(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }

        // Find current position in filtered list
        let current_pos = self.filtered_indices
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);

        if current_pos < self.filtered_indices.len() - 1 {
            self.selected = self.filtered_indices[current_pos + 1];
            self.description_scroll = 0; // Reset scroll when changing field
        }
    }

    /// Scroll description up (show earlier content)
    pub fn scroll_description_up(&mut self) {
        if self.description_scroll > 0 {
            self.description_scroll -= 1;
        }
    }

    /// Scroll description down (show later content)
    pub fn scroll_description_down(&mut self, max_scroll: u16) {
        if self.description_scroll < max_scroll {
            self.description_scroll += 1;
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
                if field.required {
                    // Required enums: cycle through values only
                    let current_idx = field
                        .enum_values
                        .iter()
                        .position(|v| v == &field.value)
                        .unwrap_or(0);
                    let next_idx = (current_idx + 1) % field.enum_values.len();
                    field.value = field.enum_values[next_idx].clone();
                } else {
                    // Optional enums: include empty state in cycle
                    if field.value.is_empty() {
                        // Empty -> first value
                        field.value = field.enum_values[0].clone();
                    } else if let Some(current_idx) =
                        field.enum_values.iter().position(|v| v == &field.value)
                    {
                        // Current value found -> next value or empty
                        if current_idx + 1 < field.enum_values.len() {
                            field.value = field.enum_values[current_idx + 1].clone();
                        } else {
                            field.value = String::new();
                        }
                    } else {
                        // Value not in enum_values -> reset to empty
                        field.value = String::new();
                    }
                }
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

    /// Clear all field values
    pub fn clear_all_values(&mut self) {
        for field in &mut self.fields {
            field.value = String::new();
        }
    }

    /// Load cached values and track frequent fields
    pub fn load_cached_values(&mut self, cached: &HashMap<String, String>) {
        self.frequent_indices.clear();
        for (i, field) in self.fields.iter_mut().enumerate() {
            if let Some(value) = cached.get(&field.id) {
                field.value = value.clone();
                self.frequent_indices.push(i);
            }
        }
    }

    /// Update env var suggestions based on current field value
    pub fn update_env_suggestions(&mut self) {
        if let Some(field) = self.current_field() {
            // Find the last $ in the value and get the text after it
            if let Some(dollar_pos) = field.value.rfind('$') {
                let after_dollar = &field.value[dollar_pos + 1..];
                // Check if we're past the $ in cursor position and it's a valid var name start
                if self.cursor_pos > dollar_pos {
                    // Get prefix (text after $, could be empty)
                    let prefix = if after_dollar.contains(|c: char| !c.is_alphanumeric() && c != '_') {
                        // There's a non-var char after the $, not in env var mode
                        self.showing_suggestions = false;
                        self.env_suggestions.clear();
                        return;
                    } else {
                        after_dollar
                    };

                    // Get suggestions
                    let suggestions = get_env_suggestions(prefix);
                    if !suggestions.is_empty() {
                        self.env_suggestions = suggestions;
                        self.showing_suggestions = true;
                        self.selected_suggestion = 0;
                    } else {
                        self.showing_suggestions = false;
                        self.env_suggestions.clear();
                    }
                    return;
                }
            }
        }

        // Not in suggestion mode
        self.showing_suggestions = false;
        self.env_suggestions.clear();
    }

    /// Move to next suggestion
    pub fn next_suggestion(&mut self) {
        if !self.env_suggestions.is_empty() {
            self.selected_suggestion = (self.selected_suggestion + 1) % self.env_suggestions.len();
        }
    }

    /// Move to previous suggestion
    pub fn prev_suggestion(&mut self) {
        if !self.env_suggestions.is_empty() {
            if self.selected_suggestion == 0 {
                self.selected_suggestion = self.env_suggestions.len() - 1;
            } else {
                self.selected_suggestion -= 1;
            }
        }
    }

    /// Accept the currently selected suggestion
    pub fn accept_suggestion(&mut self) {
        if self.showing_suggestions && !self.env_suggestions.is_empty() {
            let var_name = self.env_suggestions[self.selected_suggestion].0.clone();

            if let Some(field) = self.current_field_mut() {
                // Find the last $ and replace everything after it with the var name
                if let Some(dollar_pos) = field.value.rfind('$') {
                    field.value.truncate(dollar_pos + 1);
                    field.value.push_str(&var_name);
                    self.cursor_pos = field.value.len();
                }
            }

            self.showing_suggestions = false;
            self.env_suggestions.clear();
        }
    }

    /// Cancel showing suggestions
    pub fn cancel_suggestions(&mut self) {
        self.showing_suggestions = false;
        self.env_suggestions.clear();
    }
}
