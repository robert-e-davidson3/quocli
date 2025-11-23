use crate::config::Config;
use crate::parser::{ArgumentType, CommandSpec, DangerLevel};
use crate::tui::theme::Theme;
use crate::tui::widgets::{FormField, FormState};
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
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

/// Run the interactive form
pub async fn run_form(
    config: &Config,
    spec: &CommandSpec,
    cached_values: HashMap<String, String>,
) -> Result<Option<HashMap<String, String>>> {
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
        return Ok(Some(HashMap::new()));
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
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_form_loop(&mut terminal, &mut state, spec, &theme, config);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}

fn run_form_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut FormState,
    spec: &CommandSpec,
    theme: &Theme,
    config: &Config,
) -> Result<Option<HashMap<String, String>>> {
    loop {
        // Draw UI
        terminal.draw(|f| draw_form(f, state, spec, theme, config))?;

        // Handle input
        if let Event::Key(key) = event::read()? {
            if state.editing {
                match key.code {
                    KeyCode::Esc => state.stop_editing(),
                    KeyCode::Enter => state.stop_editing(),
                    KeyCode::Backspace => state.delete_char(),
                    KeyCode::Char(c) => state.insert_char(c),
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(None),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(None)
                    }
                    KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(Some(state.get_values()))
                    }
                    KeyCode::Up | KeyCode::Char('k') => state.move_up(),
                    KeyCode::Down | KeyCode::Char('j') => state.move_down(),
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

fn draw_form(
    f: &mut Frame,
    state: &FormState,
    spec: &CommandSpec,
    theme: &Theme,
    config: &Config,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(10),    // Form fields
            Constraint::Length(5),  // Command preview
            Constraint::Length(2),  // Help
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

    // Form fields
    let items: Vec<ListItem> = state
        .fields
        .iter()
        .enumerate()
        .map(|(i, field)| {
            let is_selected = i == state.selected;
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

    let list = List::new(items)
        .block(Block::default().title("Options").borders(Borders::ALL))
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

    // Help text
    let help_text = if state.editing {
        "ESC/Enter: finish editing | Backspace: delete"
    } else {
        "↑/↓: navigate | Enter: edit | Ctrl+E: execute | q/Esc: cancel"
    };
    let help = Paragraph::new(help_text).style(theme.help);
    f.render_widget(help, chunks[3]);

    // Show description popup when field is selected
    if let Some(field) = state.current_field() {
        if !field.description.is_empty() {
            let area = centered_rect(60, 20, f.area());
            f.render_widget(Clear, area);
            let desc = Paragraph::new(field.description.clone())
                .block(Block::default().title("Description").borders(Borders::ALL))
                .wrap(Wrap { trim: true });
            f.render_widget(desc, area);
        }
    }
}

fn build_preview(spec: &CommandSpec, state: &FormState) -> String {
    let mut parts = vec![spec.command.clone()];

    for field in &state.fields {
        if field.value.is_empty() {
            continue;
        }

        // Handle positional arguments
        if field.id.starts_with("_pos_") {
            if field.sensitive {
                parts.push("***".to_string());
            } else {
                parts.push(field.value.clone());
            }
            continue;
        }

        // Handle flags
        match field.field_type {
            ArgumentType::Bool => {
                if field.value == "true" {
                    parts.push(field.id.clone());
                }
            }
            _ => {
                parts.push(field.id.clone());
                if field.sensitive {
                    parts.push("***".to_string());
                } else {
                    parts.push(field.value.clone());
                }
            }
        }
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
