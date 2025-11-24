use crate::parser::{ArgumentType, CommandOption, OptionLevel, PositionalArg};
use crate::shell::get_env_suggestions;
use std::collections::HashMap;

/// Tab categories for organizing options
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OptionTab {
    Basic,
    Advanced,
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
    pub level: OptionLevel,
}

impl FormField {
    pub fn from_option(opt: &CommandOption) -> Self {
        let id = opt.primary_flag().to_string();
        let label = if let Some(short) = opt.short_flag() {
            // Only show "short, long" if they're different
            if short != opt.primary_flag() {
                format!("{}, {}", short, opt.primary_flag())
            } else {
                short.to_string()
            }
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
            level: opt.level.clone(),
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
            level: OptionLevel::Basic, // Positional args are always basic
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
    pub basic_indices: Vec<usize>,    // indices of basic-level fields
    pub advanced_indices: Vec<usize>, // indices of advanced-level fields
    pub frequent_indices: Vec<usize>, // indices of fields that have cached values
    // Env var suggestion state
    pub showing_suggestions: bool,
    pub env_suggestions: Vec<(String, String)>, // (name, value)
    pub selected_suggestion: usize,
    // Description scroll state
    pub description_scroll: u16,
    // Help sheet state
    pub showing_help: bool,
}

impl FormState {
    pub fn new(fields: Vec<FormField>) -> Self {
        // Compute basic and advanced indices based on level
        let basic_indices: Vec<usize> = fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.level == OptionLevel::Basic)
            .map(|(i, _)| i)
            .collect();

        let advanced_indices: Vec<usize> = fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.level == OptionLevel::Advanced)
            .map(|(i, _)| i)
            .collect();

        // Start with basic indices as filtered (or all if no basic options)
        let filtered_indices = if basic_indices.is_empty() {
            (0..fields.len()).collect()
        } else {
            basic_indices.clone()
        };

        Self {
            fields,
            selected: 0,
            editing: false,
            cursor_pos: 0,
            search_mode: false,
            search_query: String::new(),
            filtered_indices,
            include_description: false,
            current_tab: OptionTab::Basic,
            basic_indices,
            advanced_indices,
            frequent_indices: Vec::new(),
            showing_suggestions: false,
            env_suggestions: Vec::new(),
            selected_suggestion: 0,
            description_scroll: 0,
            showing_help: false,
        }
    }

    /// Cycle to next tab
    pub fn next_tab(&mut self) {
        self.current_tab = match self.current_tab {
            OptionTab::Basic => OptionTab::Advanced,
            OptionTab::Advanced => OptionTab::Frequent,
            OptionTab::Frequent => OptionTab::Basic,
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
            OptionTab::Basic => {
                if self.basic_indices.is_empty() {
                    // No basic items, show all
                    self.filtered_indices = (0..self.fields.len()).collect();
                } else {
                    self.filtered_indices = self.basic_indices.clone();
                }
            }
            OptionTab::Advanced => {
                if self.advanced_indices.is_empty() {
                    // No advanced items, show all
                    self.filtered_indices = (0..self.fields.len()).collect();
                } else {
                    self.filtered_indices = self.advanced_indices.clone();
                }
            }
            OptionTab::Frequent => {
                // Only show options that have been used (have cached values)
                // Don't fall back to all - empty is correct when nothing has been used
                self.filtered_indices = self.frequent_indices.clone();
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

    /// Move to first field (Home)
    pub fn move_to_top(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.selected = self.filtered_indices[0];
            self.description_scroll = 0;
        }
    }

    /// Move to last field (End)
    pub fn move_to_bottom(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.selected = self.filtered_indices[self.filtered_indices.len() - 1];
            self.description_scroll = 0;
        }
    }

    /// Move up by a page (PageUp)
    pub fn page_up(&mut self, page_size: usize) {
        if self.filtered_indices.is_empty() {
            return;
        }

        let current_pos = self.filtered_indices
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);

        let new_pos = current_pos.saturating_sub(page_size);
        self.selected = self.filtered_indices[new_pos];
        self.description_scroll = 0;
    }

    /// Move down by a page (PageDown)
    pub fn page_down(&mut self, page_size: usize) {
        if self.filtered_indices.is_empty() {
            return;
        }

        let current_pos = self.filtered_indices
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);

        let new_pos = (current_pos + page_size).min(self.filtered_indices.len() - 1);
        self.selected = self.filtered_indices[new_pos];
        self.description_scroll = 0;
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

    /// Toggle help sheet visibility
    pub fn toggle_help(&mut self) {
        self.showing_help = !self.showing_help;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::CommandOption;

    // Helper to create a test FormField
    fn create_test_field(id: &str, field_type: ArgumentType, level: OptionLevel) -> FormField {
        FormField {
            id: id.to_string(),
            label: id.to_string(),
            description: format!("Description for {}", id),
            field_type,
            required: false,
            sensitive: false,
            value: String::new(),
            enum_values: vec![],
            default: None,
            level,
        }
    }

    #[test]
    fn test_form_field_from_option() {
        let opt = CommandOption {
            flags: vec!["--verbose".to_string(), "-v".to_string()],
            description: "Enable verbose output".to_string(),
            argument_type: ArgumentType::Bool,
            argument_name: None,
            required: false,
            sensitive: false,
            repeatable: false,
            conflicts_with: vec![],
            requires: vec![],
            default: Some("false".to_string()),
            enum_values: vec![],
            level: OptionLevel::Basic,
        };

        let field = FormField::from_option(&opt);
        assert_eq!(field.id, "--verbose");
        assert_eq!(field.label, "-v, --verbose");
        assert_eq!(field.field_type, ArgumentType::Bool);
        assert_eq!(field.default, Some("false".to_string()));
    }

    #[test]
    fn test_form_field_from_positional() {
        let arg = PositionalArg {
            name: "file".to_string(),
            description: "Input file".to_string(),
            required: true,
            sensitive: false,
            argument_type: ArgumentType::Path,
            default: None,
        };

        let field = FormField::from_positional(&arg);
        assert_eq!(field.id, "_pos_file");
        assert_eq!(field.label, "file");
        assert!(field.required);
        assert_eq!(field.level, OptionLevel::Basic);
    }

    #[test]
    fn test_form_field_display_value_normal() {
        let mut field = create_test_field("test", ArgumentType::String, OptionLevel::Basic);
        field.value = "hello".to_string();

        assert_eq!(field.display_value(), "hello");
    }

    #[test]
    fn test_form_field_display_value_sensitive() {
        let mut field = create_test_field("test", ArgumentType::String, OptionLevel::Basic);
        field.sensitive = true;
        field.value = "secret123".to_string();

        let display = field.display_value();
        assert!(display.contains("*"));
        assert!(!display.contains("secret"));
    }

    #[test]
    fn test_form_field_display_value_empty_with_default() {
        let mut field = create_test_field("test", ArgumentType::String, OptionLevel::Basic);
        field.default = Some("default_value".to_string());

        assert_eq!(field.display_value(), "(default: default_value)");
    }

    #[test]
    fn test_form_field_display_value_empty_no_default() {
        let field = create_test_field("test", ArgumentType::String, OptionLevel::Basic);
        assert_eq!(field.display_value(), "");
    }

    #[test]
    fn test_form_state_new_basic_fields() {
        let fields = vec![
            create_test_field("a", ArgumentType::String, OptionLevel::Basic),
            create_test_field("b", ArgumentType::String, OptionLevel::Advanced),
            create_test_field("c", ArgumentType::String, OptionLevel::Basic),
        ];

        let state = FormState::new(fields);
        assert_eq!(state.basic_indices, vec![0, 2]);
        assert_eq!(state.advanced_indices, vec![1]);
        assert_eq!(state.current_tab, OptionTab::Basic);
        // Initially filtered to basic fields
        assert_eq!(state.filtered_indices, vec![0, 2]);
    }

    #[test]
    fn test_form_state_navigation() {
        let fields = vec![
            create_test_field("a", ArgumentType::String, OptionLevel::Basic),
            create_test_field("b", ArgumentType::String, OptionLevel::Basic),
            create_test_field("c", ArgumentType::String, OptionLevel::Basic),
        ];

        let mut state = FormState::new(fields);

        // Initial selection
        assert_eq!(state.selected, 0);

        // Move down
        state.move_down();
        assert_eq!(state.selected, 1);

        state.move_down();
        assert_eq!(state.selected, 2);

        // Can't move past end
        state.move_down();
        assert_eq!(state.selected, 2);

        // Move up
        state.move_up();
        assert_eq!(state.selected, 1);

        state.move_up();
        assert_eq!(state.selected, 0);

        // Can't move before start
        state.move_up();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_form_state_move_to_top_and_bottom() {
        let fields = vec![
            create_test_field("a", ArgumentType::String, OptionLevel::Basic),
            create_test_field("b", ArgumentType::String, OptionLevel::Basic),
            create_test_field("c", ArgumentType::String, OptionLevel::Basic),
        ];

        let mut state = FormState::new(fields);

        state.move_to_bottom();
        assert_eq!(state.selected, 2);

        state.move_to_top();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_form_state_page_navigation() {
        let fields: Vec<FormField> = (0..20)
            .map(|i| create_test_field(&format!("f{}", i), ArgumentType::String, OptionLevel::Basic))
            .collect();

        let mut state = FormState::new(fields);

        state.page_down(5);
        assert_eq!(state.selected, 5);

        state.page_down(5);
        assert_eq!(state.selected, 10);

        state.page_up(3);
        assert_eq!(state.selected, 7);
    }

    #[test]
    fn test_form_state_tab_switching() {
        let fields = vec![
            create_test_field("a", ArgumentType::String, OptionLevel::Basic),
            create_test_field("b", ArgumentType::String, OptionLevel::Advanced),
        ];

        let mut state = FormState::new(fields);

        assert_eq!(state.current_tab, OptionTab::Basic);
        assert_eq!(state.filtered_indices, vec![0]);

        state.next_tab();
        assert_eq!(state.current_tab, OptionTab::Advanced);
        assert_eq!(state.filtered_indices, vec![1]);

        state.next_tab();
        assert_eq!(state.current_tab, OptionTab::Frequent);
        // No frequent items yet
        assert!(state.filtered_indices.is_empty());

        state.next_tab();
        assert_eq!(state.current_tab, OptionTab::Basic);
    }

    #[test]
    fn test_form_state_set_tab() {
        let fields = vec![
            create_test_field("a", ArgumentType::String, OptionLevel::Basic),
            create_test_field("b", ArgumentType::String, OptionLevel::Advanced),
        ];

        let mut state = FormState::new(fields);

        state.set_tab(OptionTab::Advanced);
        assert_eq!(state.current_tab, OptionTab::Advanced);
        assert_eq!(state.filtered_indices, vec![1]);
    }

    #[test]
    fn test_form_state_editing() {
        let fields = vec![create_test_field("test", ArgumentType::String, OptionLevel::Basic)];

        let mut state = FormState::new(fields);

        state.start_editing();
        assert!(state.editing);

        state.insert_char('h');
        state.insert_char('i');
        assert_eq!(state.fields[0].value, "hi");
        assert_eq!(state.cursor_pos, 2);

        state.delete_char();
        assert_eq!(state.fields[0].value, "h");
        assert_eq!(state.cursor_pos, 1);

        state.stop_editing();
        assert!(!state.editing);
    }

    #[test]
    fn test_form_state_toggle_bool() {
        let fields = vec![create_test_field("flag", ArgumentType::Bool, OptionLevel::Basic)];

        let mut state = FormState::new(fields);

        // Initially empty (treated as false)
        state.toggle_bool();
        assert_eq!(state.fields[0].value, "true");

        state.toggle_bool();
        assert_eq!(state.fields[0].value, "false");

        state.toggle_bool();
        assert_eq!(state.fields[0].value, "true");
    }

    #[test]
    fn test_form_state_cycle_enum() {
        let mut field = create_test_field("color", ArgumentType::Enum, OptionLevel::Basic);
        field.enum_values = vec!["red".to_string(), "green".to_string(), "blue".to_string()];

        let mut state = FormState::new(vec![field]);

        // Empty -> first value
        state.cycle_enum();
        assert_eq!(state.fields[0].value, "red");

        state.cycle_enum();
        assert_eq!(state.fields[0].value, "green");

        state.cycle_enum();
        assert_eq!(state.fields[0].value, "blue");

        // Back to empty for optional enum
        state.cycle_enum();
        assert_eq!(state.fields[0].value, "");
    }

    #[test]
    fn test_form_state_cycle_required_enum() {
        let mut field = create_test_field("color", ArgumentType::Enum, OptionLevel::Basic);
        field.enum_values = vec!["red".to_string(), "green".to_string()];
        field.required = true;

        let mut state = FormState::new(vec![field]);

        state.cycle_enum();
        assert_eq!(state.fields[0].value, "green"); // Starts at index 0, goes to 1

        state.cycle_enum();
        assert_eq!(state.fields[0].value, "red"); // Wraps around to 0
    }

    #[test]
    fn test_form_state_get_values() {
        let fields = vec![
            create_test_field("a", ArgumentType::String, OptionLevel::Basic),
            create_test_field("b", ArgumentType::String, OptionLevel::Basic),
            create_test_field("c", ArgumentType::String, OptionLevel::Basic),
        ];

        let mut state = FormState::new(fields);
        state.fields[0].value = "value_a".to_string();
        state.fields[2].value = "value_c".to_string();
        // fields[1] left empty

        let values = state.get_values();
        assert_eq!(values.len(), 2);
        assert_eq!(values.get("a"), Some(&"value_a".to_string()));
        assert_eq!(values.get("c"), Some(&"value_c".to_string()));
        assert!(values.get("b").is_none());
    }

    #[test]
    fn test_form_state_clear_all_values() {
        let fields = vec![
            create_test_field("a", ArgumentType::String, OptionLevel::Basic),
            create_test_field("b", ArgumentType::String, OptionLevel::Basic),
        ];

        let mut state = FormState::new(fields);
        state.fields[0].value = "value_a".to_string();
        state.fields[1].value = "value_b".to_string();

        state.clear_all_values();

        assert!(state.fields[0].value.is_empty());
        assert!(state.fields[1].value.is_empty());
    }

    #[test]
    fn test_form_state_load_cached_values() {
        let fields = vec![
            create_test_field("a", ArgumentType::String, OptionLevel::Basic),
            create_test_field("b", ArgumentType::String, OptionLevel::Basic),
            create_test_field("c", ArgumentType::String, OptionLevel::Basic),
        ];

        let mut state = FormState::new(fields);

        let mut cached = HashMap::new();
        cached.insert("a".to_string(), "cached_a".to_string());
        cached.insert("c".to_string(), "cached_c".to_string());

        state.load_cached_values(&cached);

        assert_eq!(state.fields[0].value, "cached_a");
        assert!(state.fields[1].value.is_empty());
        assert_eq!(state.fields[2].value, "cached_c");
        assert_eq!(state.frequent_indices, vec![0, 2]);
    }

    #[test]
    fn test_form_state_search() {
        let fields = vec![
            create_test_field("--verbose", ArgumentType::Bool, OptionLevel::Basic),
            create_test_field("--output", ArgumentType::Path, OptionLevel::Basic),
            create_test_field("--version", ArgumentType::Bool, OptionLevel::Basic),
        ];

        let mut state = FormState::new(fields);

        state.start_search(false);
        assert!(state.search_mode);

        state.search_insert_char('v');
        state.search_insert_char('e');
        state.search_insert_char('r');

        // Should match --verbose and --version
        assert_eq!(state.filtered_indices.len(), 2);
        assert!(state.filtered_indices.contains(&0)); // --verbose
        assert!(state.filtered_indices.contains(&2)); // --version

        state.clear_search();
        assert_eq!(state.filtered_indices.len(), 3);
    }

    #[test]
    fn test_form_state_search_delete_char() {
        let fields = vec![
            create_test_field("--verbose", ArgumentType::Bool, OptionLevel::Basic),
            create_test_field("--output", ArgumentType::Path, OptionLevel::Basic),
        ];

        let mut state = FormState::new(fields);

        state.start_search(false);
        state.search_insert_char('v');
        state.search_insert_char('e');

        assert_eq!(state.search_query, "ve");

        state.search_delete_char();
        assert_eq!(state.search_query, "v");
    }

    #[test]
    fn test_form_state_visible_fields() {
        let fields = vec![
            create_test_field("a", ArgumentType::String, OptionLevel::Basic),
            create_test_field("b", ArgumentType::String, OptionLevel::Advanced),
            create_test_field("c", ArgumentType::String, OptionLevel::Basic),
        ];

        let state = FormState::new(fields);

        let visible = state.visible_fields();
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].0, 0);
        assert_eq!(visible[0].1.id, "a");
        assert_eq!(visible[1].0, 2);
        assert_eq!(visible[1].1.id, "c");
    }

    #[test]
    fn test_form_state_current_field() {
        let fields = vec![
            create_test_field("a", ArgumentType::String, OptionLevel::Basic),
            create_test_field("b", ArgumentType::String, OptionLevel::Basic),
        ];

        let mut state = FormState::new(fields);

        assert_eq!(state.current_field().unwrap().id, "a");

        state.move_down();
        assert_eq!(state.current_field().unwrap().id, "b");
    }

    #[test]
    fn test_form_state_scroll_description() {
        let fields = vec![create_test_field("test", ArgumentType::String, OptionLevel::Basic)];

        let mut state = FormState::new(fields);

        assert_eq!(state.description_scroll, 0);

        state.scroll_description_down(10);
        assert_eq!(state.description_scroll, 1);

        state.scroll_description_down(10);
        state.scroll_description_down(10);
        assert_eq!(state.description_scroll, 3);

        state.scroll_description_up();
        assert_eq!(state.description_scroll, 2);

        // Can't scroll below 0
        state.scroll_description_up();
        state.scroll_description_up();
        state.scroll_description_up();
        assert_eq!(state.description_scroll, 0);
    }

    #[test]
    fn test_form_state_toggle_help() {
        let fields = vec![create_test_field("test", ArgumentType::String, OptionLevel::Basic)];

        let mut state = FormState::new(fields);

        assert!(!state.showing_help);

        state.toggle_help();
        assert!(state.showing_help);

        state.toggle_help();
        assert!(!state.showing_help);
    }

    #[test]
    fn test_form_state_suggestions() {
        let fields = vec![create_test_field("test", ArgumentType::String, OptionLevel::Basic)];

        let mut state = FormState::new(fields);

        assert!(!state.showing_suggestions);

        // Manually set suggestions for testing
        state.env_suggestions = vec![
            ("HOME".to_string(), "/home/user".to_string()),
            ("HOST".to_string(), "localhost".to_string()),
        ];
        state.showing_suggestions = true;
        state.selected_suggestion = 0;

        state.next_suggestion();
        assert_eq!(state.selected_suggestion, 1);

        state.next_suggestion();
        assert_eq!(state.selected_suggestion, 0); // Wraps

        state.prev_suggestion();
        assert_eq!(state.selected_suggestion, 1);

        state.cancel_suggestions();
        assert!(!state.showing_suggestions);
        assert!(state.env_suggestions.is_empty());
    }

    #[test]
    fn test_form_state_accept_suggestion() {
        let fields = vec![create_test_field("test", ArgumentType::String, OptionLevel::Basic)];

        let mut state = FormState::new(fields);

        // Set up a scenario where user typed "$HO"
        state.fields[0].value = "$HO".to_string();
        state.cursor_pos = 3;
        state.env_suggestions = vec![("HOME".to_string(), "/home/user".to_string())];
        state.showing_suggestions = true;
        state.selected_suggestion = 0;

        state.accept_suggestion();

        assert_eq!(state.fields[0].value, "$HOME");
        assert!(!state.showing_suggestions);
    }

    #[test]
    fn test_form_state_empty_fields() {
        let state = FormState::new(vec![]);

        assert!(state.fields.is_empty());
        assert!(state.filtered_indices.is_empty());
        assert!(state.current_field().is_none());
    }

    #[test]
    fn test_form_state_navigation_with_filtering() {
        let fields = vec![
            create_test_field("a", ArgumentType::String, OptionLevel::Basic),
            create_test_field("b", ArgumentType::String, OptionLevel::Advanced),
            create_test_field("c", ArgumentType::String, OptionLevel::Basic),
        ];

        let mut state = FormState::new(fields);

        // Only basic fields visible
        assert_eq!(state.filtered_indices, vec![0, 2]);

        // Navigate within filtered
        assert_eq!(state.selected, 0);
        state.move_down();
        assert_eq!(state.selected, 2); // Skips index 1
    }
}
