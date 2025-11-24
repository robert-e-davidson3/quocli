use ratatui::style::{Color, Modifier, Style};

pub struct Theme {
    pub normal: Style,
    pub selected: Style,
    pub required: Style,
    pub sensitive: Style,
    pub header: Style,
    pub preview: Style,
    pub danger: Style,
    pub help: Style,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            normal: Style::default().fg(Color::White),
            selected: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            required: Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            sensitive: Style::default().fg(Color::Magenta),
            header: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            preview: Style::default().fg(Color::Green),
            danger: Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
            help: Style::default().fg(Color::DarkGray),
        }
    }

    pub fn light() -> Self {
        Self {
            normal: Style::default().fg(Color::Black),
            selected: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            required: Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
            sensitive: Style::default().fg(Color::Magenta),
            header: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            preview: Style::default().fg(Color::DarkGray),
            danger: Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
            help: Style::default().fg(Color::Gray),
        }
    }
}
