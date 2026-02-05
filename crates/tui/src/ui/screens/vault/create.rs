use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::{app::AppState, ui::theme::Theme};

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let form = &state.vault_ui.form;

    let block = Block::default()
        .title(Span::styled(
            " Create Vault ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Name field
            Constraint::Length(1), // Currency field
            Constraint::Min(0),    // Error
        ])
        .split(inner);

    // Name field
    let name_value = if form.name.is_empty() { "_" } else { form.name.as_str() };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "  Name      ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(name_value.to_string(), Style::default().fg(theme.text)),
            Span::styled("_", Style::default().fg(theme.accent)),
        ])),
        layout[0],
    );

    // Currency field (fixed for now)
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Currency  ", Style::default().fg(theme.text_muted)),
            Span::styled("EUR", Style::default().fg(theme.text)),
        ])),
        layout[1],
    );

    // Error
    if let Some(err) = form.error.as_ref() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  ✗ ", Style::default().fg(theme.negative)),
                Span::styled(err.clone(), Style::default().fg(theme.negative)),
            ])),
            layout[2],
        );
    }
}
