use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    app::{AppState, LoginField},
    ui::{components::centered_rect, theme::Theme},
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let background = vault_background();
    let background_widget = Paragraph::new(background)
        .style(Style::default().fg(theme.dim))
        .alignment(Alignment::Center);
    frame.render_widget(background_widget, area);

    let card_area = centered_rect(60, 45, area);
    let block = Block::default()
        .title("Vault Login")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(&block, card_area);

    let inner = block.inner(card_area);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    let login = &state.login;
    let fields = [
        ("Username", login.username.as_str(), LoginField::Username),
        (
            "Password",
            masked_password(&login.password),
            LoginField::Password,
        ),
    ];

    for (idx, (label, value, field)) in fields.iter().enumerate() {
        let is_focused = login.focus == *field;
        let line = render_field(label, value, is_focused, &theme);
        frame.render_widget(Paragraph::new(line), rows[idx]);
    }

    let hint = Line::from(vec![
        Span::styled("Enter", Style::default().fg(theme.accent)),
        Span::raw(" login  "),
        Span::styled("Tab", Style::default().fg(theme.accent)),
        Span::raw(" next  "),
        Span::styled("Esc", Style::default().fg(theme.accent)),
        Span::raw(" quit"),
    ]);
    frame.render_widget(Paragraph::new(hint), rows[3]);

    if let Some(message) = &login.message {
        let msg = Paragraph::new(Line::from(Span::styled(
            message.as_str(),
            Style::default().fg(theme.error),
        )))
        .alignment(Alignment::Left);
        frame.render_widget(msg, rows[4]);
    }
}

fn render_field(label: &str, value: &str, focused: bool, theme: &Theme) -> Line<'static> {
    let label_style = if focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };

    let value_style = if focused {
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };

    Line::from(vec![
        Span::styled(format!("{label:<12}"), label_style),
        Span::raw(" "),
        Span::styled(value.to_string(), value_style),
    ])
}

fn masked_password(value: &str) -> &str {
    if value.is_empty() { "" } else { "********" }
}

fn vault_background() -> Vec<Line<'static>> {
    vec![
        Line::from("               _____________________________               "),
        Line::from("          .-''                               ''-.          "),
        Line::from("       .-'                                       '-.       "),
        Line::from("     .'     ___________________________________     '.     "),
        Line::from("    /     .'                                   '.     \\    "),
        Line::from("   |     /     _____________     ___________     \\     |   "),
        Line::from("   |    |     /             \\   /           \\     |    |   "),
        Line::from("   |    |    |   _________   | |   _______   |    |    |   "),
        Line::from("   |    |    |  /         \\  | |  /       \\  |    |    |   "),
        Line::from("   |    |    | |  .-----.  | | | |  .---. | |    |    |   "),
        Line::from("   |    |    | |  |  o  |  | | | |  | o | | |    |    |   "),
        Line::from("   |    |    | |  '-----'  | | | |  '---' | |    |    |   "),
        Line::from("   |    |    |  \\_________/  | |  \\_______/  |    |    |   "),
        Line::from("   |    |     \\             /   \\           /     |    |   "),
        Line::from("   |    |      '.         .'     '.       .'      |    |   "),
        Line::from("   |    |        '-.   .-'           '-.-'        |    |   "),
        Line::from("   |    |           | |    .-----.    | |         |    |   "),
        Line::from("   |    |           | |   /  o  \\    | |         |    |   "),
        Line::from("   |    |           | |   \\_____/    | |         |    |   "),
        Line::from("   |    |           | |      |       | |         |    |   "),
        Line::from("   |     \\          | |     / \\      | |        /     |   "),
        Line::from("    \\      '.        \\ \\   /___\\    / /      .'      /    "),
        Line::from("     '.       '-.      '---------''      .-'       .'     "),
        Line::from("       '-.         _____________       .-'          "),
        Line::from("          ''-.___.'             '.__.-''           "),
    ]
}
