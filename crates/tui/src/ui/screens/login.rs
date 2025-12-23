use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
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

    // Centered login box - minimal size
    let box_width = 40;
    let box_height = 8;
    let card_area = centered_box(box_width, box_height, area);

    // Clear the area behind the form
    frame.render_widget(Clear, card_area);

    // Main container with rounded borders
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(card_area);
    frame.render_widget(block, card_area);

    // Layout: username, password, hint
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Username
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Password
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Hint
        ])
        .split(inner);

    let login = &state.login;

    // Username field
    let username_focused = login.focus == LoginField::Username;
    render_field(
        frame,
        rows[0],
        "Username",
        &login.username,
        false,
        username_focused,
        &theme,
    );

    // Password field
    let password_focused = login.focus == LoginField::Password;
    let password_display = mask_password(&login.password);
    render_field(
        frame,
        rows[2],
        "Password",
        &password_display,
        true,
        password_focused,
        &theme,
    );

    // Hint line
    let hint = Line::from(vec![
        Span::styled("Enter", Style::default().fg(theme.accent)),
        Span::styled(" to login  ", Style::default().fg(theme.dim)),
        Span::styled("Tab", Style::default().fg(theme.accent)),
        Span::styled(" to switch", Style::default().fg(theme.dim)),
    ]);
    frame.render_widget(
        Paragraph::new(hint).alignment(Alignment::Center),
        rows[4],
    );

    // Error message below the box
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

fn render_field(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    value: &str,
    is_password: bool,
    focused: bool,
    theme: &Theme,
) {
    let label_style = Style::default().fg(theme.dim);
    let value_style = if focused {
        Style::default()
            .fg(theme.text)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_muted)
    };

    let cursor = if focused { "│" } else { "" };

    // Show placeholder for empty password field
    let display_value = if is_password && value.is_empty() && !focused {
        "••••••••".to_string()
    } else if value.is_empty() && !focused {
        "".to_string()
    } else {
        format!("{value}{cursor}")
    };

    let line = Line::from(vec![
        Span::styled(format!("{label}: "), label_style),
        Span::styled(display_value, value_style),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

/// Masks password with bullets, one per character
fn mask_password(password: &str) -> String {
    if password.is_empty() {
        String::new()
    } else {
        "•".repeat(password.len())
    }
}
