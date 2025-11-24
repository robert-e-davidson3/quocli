use crate::config::Config;
use crate::parser::{ArgumentType, CommandSpec, DangerLevel};
use crate::tui::theme::Theme;
use crate::tui::widgets::{FormField, FormState, OptionTab};
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::collections::HashMap;
use std::io;

/// Result of running the form
#[derive(Debug)]
pub enum FormResult {
    /// User wants to execute the command
    Execute(HashMap<String, String>),
    /// User wants to preview the command without executing
    Preview(HashMap<String, String>),
    /// User cancelled the form
    Cancel,
}

/// Run the interactive form
pub async fn run_form(
    config: &Config,
    spec: &CommandSpec,
    cached_values: HashMap<String, String>,
) -> Result<FormResult> {
    // Build form fields
    let mut fields: Vec<FormField> = Vec::new();

    // Add positional arguments first
    for arg in &spec.positional_args {
        fields.push(FormField::from_positional(arg));
    }

    // Add options
    for opt in &spec.options {
        fields.push(FormField::from_option(opt));
    }

    if fields.is_empty() {
        // No fields to edit, just return empty values
        return Ok(FormResult::Execute(HashMap::new()));
    }

    // Create form state
    let mut state = FormState::new(fields);
    state.load_cached_values(&cached_values);

    // Get theme
    let theme = if config.ui.theme == "light" {
        Theme::light()
    } else {
        Theme::dark()
    };

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_form_loop(&mut terminal, &mut state, spec, &theme, config);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;

    result
}

fn run_form_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut FormState,
    spec: &CommandSpec,
    theme: &Theme,
    config: &Config,
) -> Result<FormResult> {
    loop {
        // Draw UI
        terminal.draw(|f| draw_form(f, state, spec, theme, config))?;

        // Handle input
        let event = event::read()?;

        // Handle mouse events for description scrolling
        if let Event::Mouse(mouse) = event {
            // Only scroll if description is shown (not editing, not showing suggestions)
            if !state.editing && !state.showing_suggestions {
                if let Some(field) = state.current_field() {
                    if !field.description.is_empty() {
                        // Estimate max scroll based on description length
                        let max_scroll = estimate_max_scroll(&field.description, terminal.size()?.height);
                        match mouse.kind {
                            // Natural scrolling: scroll wheel up shows content above
                            MouseEventKind::ScrollUp => state.scroll_description_up(),
                            // Natural scrolling: scroll wheel down shows content below
                            MouseEventKind::ScrollDown => state.scroll_description_down(max_scroll),
                            _ => {}
                        }
                    }
                }
            }
            continue;
        }

        if let Event::Key(key) = event {
            if state.editing {
                if state.showing_suggestions {
                    // Handle suggestion navigation
                    match key.code {
                        KeyCode::Esc => state.cancel_suggestions(),
                        KeyCode::Tab | KeyCode::Enter => {
                            state.accept_suggestion();
                            state.update_env_suggestions();
                        }
                        KeyCode::Up => state.prev_suggestion(),
                        KeyCode::Down => state.next_suggestion(),
                        KeyCode::Backspace => {
                            state.delete_char();
                            state.update_env_suggestions();
                        }
                        KeyCode::Char(c) => {
                            state.insert_char(c);
                            state.update_env_suggestions();
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Esc => state.stop_editing(),
                        KeyCode::Enter => state.stop_editing(),
                        KeyCode::Backspace => {
                            state.delete_char();
                            state.update_env_suggestions();
                        }
                        KeyCode::Char(c) => {
                            state.insert_char(c);
                            state.update_env_suggestions();
                        }
                        _ => {}
                    }
                }
            } else if state.search_mode {
                // Search mode key handling
                match key.code {
                    KeyCode::Esc => state.clear_search(),
                    KeyCode::Enter => state.stop_search(),
                    KeyCode::Backspace => state.search_delete_char(),
                    KeyCode::Up => state.move_up(),
                    KeyCode::Down => state.move_down(),
                    KeyCode::Char(c) => state.search_insert_char(c),
                    _ => {}
                }
            } else if state.showing_help {
                // Close help sheet on any key
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                        state.showing_help = false;
                    }
                    _ => state.showing_help = false,
                }
            } else {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        if !state.search_query.is_empty() {
                            state.clear_search();
                        } else {
                            return Ok(FormResult::Cancel);
                        }
                    }
                    KeyCode::Char('?') => state.toggle_help(),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(FormResult::Cancel)
                    }
                    KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(FormResult::Execute(state.get_values()))
                    }
                    KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(FormResult::Preview(state.get_values()))
                    }
                    KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        state.clear_all_values()
                    }
                    // Description scrolling with Ctrl+Up/Down
                    KeyCode::Up if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        state.scroll_description_up();
                    }
                    KeyCode::Down if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(field) = state.current_field() {
                            if !field.description.is_empty() {
                                let term_height = terminal.size().map(|s| s.height).unwrap_or(24);
                                let max_scroll = estimate_max_scroll(&field.description, term_height);
                                state.scroll_description_down(max_scroll);
                            }
                        }
                    }
                    // Search: / for flag-only search, Ctrl+/ for including description
                    KeyCode::Char('/') => {
                        let include_desc = key.modifiers.contains(KeyModifiers::CONTROL);
                        state.start_search(include_desc);
                    }
                    // Tab switching
                    KeyCode::Char('`') => state.next_tab(),
                    KeyCode::Char('1') => state.set_tab(OptionTab::Basic),
                    KeyCode::Char('2') => state.set_tab(OptionTab::Advanced),
                    KeyCode::Char('3') => state.set_tab(OptionTab::Frequent),
                    KeyCode::Up | KeyCode::Char('k') => state.move_up(),
                    KeyCode::Down | KeyCode::Char('j') => state.move_down(),
                    KeyCode::PageUp => state.page_up(10),
                    KeyCode::PageDown => state.page_down(10),
                    KeyCode::Home => state.move_to_top(),
                    KeyCode::End => state.move_to_bottom(),
                    KeyCode::Enter => {
                        if let Some(field) = state.current_field() {
                            match field.field_type {
                                ArgumentType::Bool => state.toggle_bool(),
                                ArgumentType::Enum => state.cycle_enum(),
                                _ => state.start_editing(),
                            }
                        }
                    }
                    KeyCode::Tab => state.move_down(),
                    KeyCode::BackTab => state.move_up(),
                    _ => {}
                }
            }
        }
    }
}

/// Build help text lines with proper wrapping
fn build_help_lines(state: &FormState, width: usize) -> Vec<Line<'static>> {
    let commands: Vec<(&str, &str)> = if state.editing {
        if state.showing_suggestions {
            vec![
                ("↑/↓", "select"),
                ("Tab/Enter", "accept"),
                ("Esc", "cancel"),
            ]
        } else {
            vec![
                ("Esc/Enter", "finish"),
                ("$VAR", "env vars"),
            ]
        }
    } else if state.search_mode {
        vec![
            ("Type", "search"),
            ("↑/↓", "nav"),
            ("Enter", "select"),
            ("Esc", "clear"),
        ]
    } else {
        vec![
            ("↑/↓", "nav"),
            ("PgUp/Dn", "page"),
            ("Enter", "edit"),
            ("/", "search"),
            ("1/2/3", "tabs"),
            ("^X", "clear"),
            ("^E", "exec"),
            ("^P", "preview"),
            ("q", "cancel"),
        ]
    };

    // Format commands as "key: desc"
    let formatted: Vec<String> = commands
        .iter()
        .map(|(k, d)| format!("{}: {}", k, d))
        .collect();

    // Try to fit on one line
    let separator = " | ";
    let one_line = formatted.join(separator);

    // For non-editing mode, we may need to show "?: help"
    let help_suffix = if !state.editing && !state.search_mode { " | ?: help" } else { "" };

    if one_line.len() + help_suffix.len() <= width {
        return vec![Line::from(format!("{}{}", one_line, help_suffix))];
    }

    // Try to wrap to two lines without breaking commands
    let mut line1: Vec<String> = Vec::new();
    let mut line2: Vec<String> = Vec::new();
    let mut current_len = 0;
    let target_len = width.saturating_sub(2); // Leave some margin
    let mut on_first_line = true;

    for cmd in &formatted {
        let cmd_len = cmd.len();
        let sep_len = if (on_first_line && !line1.is_empty()) || (!on_first_line && !line2.is_empty()) {
            separator.len()
        } else {
            0
        };

        if on_first_line {
            if current_len + sep_len + cmd_len <= target_len {
                line1.push(cmd.clone());
                current_len += sep_len + cmd_len;
            } else {
                // Move to second line
                on_first_line = false;
                line2.push(cmd.clone());
                current_len = cmd_len;
            }
        } else {
            if current_len + sep_len + cmd_len <= target_len {
                line2.push(cmd.clone());
                current_len += sep_len + cmd_len;
            } else {
                // Would need a third line - truncate and add ?: help
                if !state.editing && !state.search_mode {
                    // Add ?: help at the end of line 1
                    let line1_text = line1.join(separator);
                    return vec![Line::from(format!("{} | ?: help", line1_text))];
                } else {
                    // Just show what fits on two lines
                    break;
                }
            }
        }
    }

    // Build the two lines
    let line1_text = line1.join(separator);
    let mut line2_text = line2.join(separator);

    // Add help suffix to line 2 if room and in normal mode
    if !state.editing && !state.search_mode && line2_text.len() + help_suffix.len() <= target_len {
        line2_text.push_str(help_suffix);
    }

    if line2_text.is_empty() {
        vec![Line::from(line1_text)]
    } else {
        vec![Line::from(line1_text), Line::from(line2_text)]
    }
}

fn draw_form(
    f: &mut Frame,
    state: &FormState,
    spec: &CommandSpec,
    theme: &Theme,
    config: &Config,
) {
    // Add search bar height when in search mode
    let search_height = if state.search_mode || !state.search_query.is_empty() { 3 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),              // Header
            Constraint::Min(10),                // Form fields
            Constraint::Length(5),              // Command preview
            Constraint::Length(search_height),  // Search bar
            Constraint::Length(2),              // Help
        ])
        .split(f.area());

    // Header
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(&spec.command, theme.header),
            Span::raw(" - "),
            Span::raw(&spec.description),
        ]),
        Line::from(Span::styled(
            format!("Danger level: {}", spec.danger_level),
            if spec.danger_level == DangerLevel::High || spec.danger_level == DangerLevel::Critical {
                theme.danger
            } else {
                theme.normal
            },
        )),
    ])
    .block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(header, chunks[0]);

    // Form fields - show only filtered results
    let visible = state.visible_fields();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|(i, field)| {
            let is_selected = *i == state.selected;
            let style = if is_selected {
                theme.selected
            } else if field.required {
                theme.required
            } else if field.sensitive {
                theme.sensitive
            } else {
                theme.normal
            };

            let marker = if field.required { "*" } else { " " };
            let value_display = field.display_value();
            let cursor = if is_selected && state.editing { "_" } else { "" };

            let content = format!(
                "{} {}: {}{}",
                marker, field.label, value_display, cursor
            );

            ListItem::new(Line::from(Span::styled(content, style)))
        })
        .collect();

    // Build title showing tab and count
    let tab_name = match state.current_tab {
        OptionTab::Basic => "Basic",
        OptionTab::Advanced => "Advanced",
        OptionTab::Frequent => "Frequent",
    };
    let title = if state.search_query.is_empty() {
        format!("[{}] Options ({})", tab_name, visible.len())
    } else {
        format!("[{}] Options ({}/{})", tab_name, visible.len(), state.fields.len())
    };

    let list = List::new(items)
        .block(Block::default().title(title).borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    f.render_widget(list, chunks[1]);

    // Command preview
    if config.ui.preview_command {
        let command_line = build_preview(spec, state);
        let preview = Paragraph::new(command_line)
            .style(theme.preview)
            .block(Block::default().title("Command Preview").borders(Borders::ALL))
            .wrap(Wrap { trim: false });
        f.render_widget(preview, chunks[2]);
    }

    // Search bar
    if state.search_mode || !state.search_query.is_empty() {
        let search_indicator = if state.include_description { "Search (+ desc): " } else { "Search: " };
        let cursor = if state.search_mode { "_" } else { "" };
        let search_text = format!("{}{}{}", search_indicator, state.search_query, cursor);
        let search = Paragraph::new(search_text)
            .style(if state.search_mode { theme.selected } else { theme.normal })
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(search, chunks[3]);
    }

    // Help text - render with dynamic wrapping
    let help_lines = build_help_lines(state, chunks[4].width as usize);
    let help = Paragraph::new(help_lines).style(theme.help);
    f.render_widget(help, chunks[4]);

    // Show help sheet popup when requested
    if state.showing_help {
        let area = centered_rect(70, 60, f.area());
        f.render_widget(Clear, area);

        let help_items = vec![
            ("↑/↓ or j/k", "Navigate fields"),
            ("PgUp/PgDn", "Page navigation"),
            ("Home/End", "Jump to top/bottom"),
            ("Enter", "Edit field / Toggle bool / Cycle enum"),
            ("Tab/Shift+Tab", "Next/previous field"),
            ("Ctrl+↑/↓", "Scroll description"),
            ("/", "Search by flag name"),
            ("Ctrl+/", "Search including descriptions"),
            ("1/2/3", "Switch to Basic/Advanced/Frequent tab"),
            ("`", "Cycle through tabs"),
            ("Ctrl+X", "Clear all values"),
            ("Ctrl+E", "Execute command"),
            ("Ctrl+P", "Preview command"),
            ("q/Esc", "Cancel"),
        ];

        let items: Vec<ListItem> = help_items
            .iter()
            .map(|(key, desc)| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:15}", key), Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(*desc),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().title("Help (press any key to close)").borders(Borders::ALL));
        f.render_widget(list, area);
    }

    // Show description popup when field is selected (but not when showing suggestions)
    if !state.showing_suggestions {
        if let Some(field) = state.current_field() {
            if !field.description.is_empty() {
                let area = centered_rect(60, 20, f.area());
                f.render_widget(Clear, area);

                // Calculate scroll info
                let (_, can_scroll_up, can_scroll_down) =
                    calc_scroll_info(&field.description, area, state.description_scroll);

                // Build scroll indicator for title
                let scroll_indicator = match (can_scroll_up, can_scroll_down) {
                    (true, true) => " ↑↓",
                    (true, false) => " ↑",
                    (false, true) => " ↓",
                    (false, false) => "",
                };
                let title = format!("Description{}", scroll_indicator);

                let desc = Paragraph::new(field.description.clone())
                    .block(Block::default().title(title).borders(Borders::ALL))
                    .wrap(Wrap { trim: true })
                    .scroll((state.description_scroll, 0));
                f.render_widget(desc, area);
            }
        }
    }

    // Show env var suggestions popup when available
    if state.showing_suggestions && !state.env_suggestions.is_empty() {
        let items: Vec<ListItem> = state
            .env_suggestions
            .iter()
            .enumerate()
            .map(|(i, (name, value))| {
                let style = if i == state.selected_suggestion {
                    theme.selected
                } else {
                    theme.normal
                };
                // Truncate value if too long
                let display_value = if value.len() > 30 {
                    format!("{}...", &value[..27])
                } else {
                    value.clone()
                };
                ListItem::new(Line::from(Span::styled(
                    format!("${} = {}", name, display_value),
                    style,
                )))
            })
            .collect();

        // Position the popup near the current field
        let area = suggestion_rect(50, state.env_suggestions.len() as u16 + 2, f.area());
        f.render_widget(Clear, area);
        let list = List::new(items)
            .block(Block::default().title("Env Variables (Tab/Enter to select)").borders(Borders::ALL));
        f.render_widget(list, area);
    }
}

/// Helper function to create a rect for suggestions popup
fn suggestion_rect(width: u16, height: u16, r: Rect) -> Rect {
    let height = height.min(15); // Max height of 15
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Length(width),
            Constraint::Min(0),
        ])
        .split(popup_layout[1])[1]
}

fn build_preview(spec: &CommandSpec, state: &FormState) -> String {
    let mut parts = vec![spec.command.clone()];
    let mut flag_parts: Vec<String> = Vec::new();
    let mut positional_parts: Vec<String> = Vec::new();

    // Process fields in two passes: flags and positionals separately
    // Then combine based on spec.positionals_first

    // First pass: flags (non-positional)
    for field in &state.fields {
        if field.value.is_empty() || field.id.starts_with("_pos_") {
            continue;
        }

        match field.field_type {
            ArgumentType::Bool => {
                if field.value == "true" {
                    flag_parts.push(field.id.clone());
                }
            }
            _ => {
                flag_parts.push(field.id.clone());
                let display_value = if field.sensitive {
                    "***".to_string()
                } else if field.value.contains(' ') {
                    format!("\"{}\"", field.value)
                } else {
                    field.value.clone()
                };
                flag_parts.push(display_value);
            }
        }
    }

    // Second pass: positional arguments
    for field in &state.fields {
        if field.value.is_empty() || !field.id.starts_with("_pos_") {
            continue;
        }

        let display_value = if field.sensitive {
            "***".to_string()
        } else if field.value.contains(' ') {
            format!("\"{}\"", field.value)
        } else {
            field.value.clone()
        };
        positional_parts.push(display_value);
    }

    // Combine based on positionals_first setting
    if spec.positionals_first {
        parts.extend(positional_parts);
        parts.extend(flag_parts);
    } else {
        parts.extend(flag_parts);
        parts.extend(positional_parts);
    }

    parts.join(" ")
}

/// Show danger confirmation dialog
pub fn confirm_dangerous(spec: &CommandSpec, command_line: &str) -> Result<bool> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_confirm_dialog(&mut terminal, spec, command_line);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}

fn run_confirm_dialog(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    spec: &CommandSpec,
    command_line: &str,
) -> Result<bool> {
    loop {
        terminal.draw(|f| {
            let area = centered_rect(70, 50, f.area());
            f.render_widget(Clear, area);

            let theme = Theme::dark();
            let content = vec![
                Line::from(Span::styled(
                    "⚠️  DANGEROUS COMMAND",
                    theme.danger,
                )),
                Line::from(""),
                Line::from(format!("This command has a {} danger level.", spec.danger_level)),
                Line::from(""),
                Line::from("Command to execute:"),
                Line::from(Span::styled(command_line, theme.preview)),
                Line::from(""),
                Line::from("Are you sure you want to proceed?"),
                Line::from(""),
                Line::from(Span::styled("Press 'y' to execute, 'n' to cancel", theme.help)),
            ];

            let paragraph = Paragraph::new(content)
                .block(Block::default().title("Confirmation Required").borders(Borders::ALL))
                .wrap(Wrap { trim: true });

            f.render_widget(paragraph, area);
        })?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(true),
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => return Ok(false),
                _ => {}
            }
        }
    }
}

/// Helper function to create a centered rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Estimate the maximum scroll offset for a description
fn estimate_max_scroll(description: &str, terminal_height: u16) -> u16 {
    // Popup is 20% of terminal height, minus 2 for borders
    let popup_height = (terminal_height as f32 * 0.20) as u16;
    let content_height = popup_height.saturating_sub(2);

    if content_height == 0 {
        return 0;
    }

    // Estimate wrapped lines: assume ~60 chars per line (60% of terminal width)
    let chars_per_line = 50;
    let estimated_lines = (description.len() as u16 / chars_per_line) + 1;

    // Max scroll is the number of lines that don't fit
    estimated_lines.saturating_sub(content_height)
}

/// Calculate scroll information for a description in the given area
fn calc_scroll_info(description: &str, area: Rect, scroll_offset: u16) -> (u16, bool, bool) {
    // Content area is area minus borders
    let content_height = area.height.saturating_sub(2);
    let content_width = area.width.saturating_sub(2);

    if content_height == 0 || content_width == 0 {
        return (0, false, false);
    }

    // Estimate wrapped lines
    let mut total_lines = 0u16;
    for line in description.lines() {
        let line_len = line.len() as u16;
        let wrapped = if line_len == 0 {
            1
        } else {
            (line_len + content_width - 1) / content_width
        };
        total_lines += wrapped;
    }

    let max_scroll = total_lines.saturating_sub(content_height);
    let can_scroll_up = scroll_offset > 0;
    let can_scroll_down = scroll_offset < max_scroll;

    (max_scroll, can_scroll_up, can_scroll_down)
}
