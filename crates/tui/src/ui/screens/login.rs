use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::Span,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::{
    app::{AppState, LoginField},
    ui::theme::Theme,
};

/// Calculates a centered rect for the login box
fn centered_box(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(width),
            Constraint::Min(0),
        ])
        .split(vertical[1]);

    horizontal[1]
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();

    // Centered login box - compact
    let box_width = 32;
    let box_height = 6;
    let card_area = centered_box(box_width, box_height, area);

    // Clear the area behind the form
    frame.render_widget(Clear, card_area);

    // Main container with rounded borders and title
    let block = Block::default()
        .title(" login ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(card_area);
    frame.render_widget(block, card_area);

    // Layout: just two input rows with spacer
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Username
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Password
        ])
        .margin(1)
        .split(inner);

    let login = &state.login;

    // Username field (no label)
    let username_focused = login.focus == LoginField::Username;
    render_input(frame, rows[0], &login.username, false, username_focused, &theme);

    // Password field (no label)
    let password_focused = login.focus == LoginField::Password;
    render_input(frame, rows[2], &login.password, true, password_focused, &theme);

    // Error message below the box (only shown when there's an error)
    if let Some(message) = &login.message {
        let error_area = Rect {
            x: card_area.x,
            y: card_area.y + card_area.height + 1,
            width: card_area.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                message.as_str(),
                Style::default().fg(theme.error),
            ))
            .alignment(Alignment::Center),
            error_area,
        );
    }
}

/// Renders a simple input field - just value and cursor, no labels
fn render_input(
    frame: &mut Frame<'_>,
    area: Rect,
    value: &str,
    is_password: bool,
    focused: bool,
    theme: &Theme,
) {
    let cursor = if focused { "│" } else { "" };

    let display = if is_password {
        format!("{}{}", mask_password(value), cursor)
    } else {
        format!("{value}{cursor}")
    };

    let style = if focused {
        Style::default().fg(theme.accent) // teal when focused
    } else {
        Style::default().fg(theme.text_muted)
    };

    frame.render_widget(Paragraph::new(Span::styled(display, style)), area);
}

/// Masks password with bullets, one per character
fn mask_password(password: &str) -> String {
    if password.is_empty() {
        String::new()
    } else {
        "•".repeat(password.len())
    }
}
