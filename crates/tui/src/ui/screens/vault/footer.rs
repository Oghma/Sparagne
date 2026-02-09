use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::{AppState, VaultMode},
    ui::theme::Theme,
};

pub(super) fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let hints = match state.vault_ui.mode {
        VaultMode::View => vec![
            ("[c]", "create"),
            ("[d]", "defaults"),
            ("[l]", "list"),
            ("[x]", "delete"),
        ],
        VaultMode::Create => vec![("[Enter]", "create"), ("[Esc]", "cancel")],
        VaultMode::Defaults => vec![
            ("[Tab]", "next"),
            ("[↑↓]", "change"),
            ("[Enter]", "save"),
            ("[Esc]", "cancel"),
        ],
        VaultMode::Select => vec![
            ("[Enter]", "select"),
            ("[↑↓]", "navigate"),
            ("[Esc]", "back"),
        ],
    };

    let mut spans = Vec::new();
    for (i, (key, action)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(*key, Style::default().fg(theme.accent)));
        spans.push(Span::styled(
            format!(" {action}"),
            Style::default().fg(theme.text_muted),
        ));
    }

    // Add list error if present
    if state.vault_ui.mode == VaultMode::Select
        && let Some(err) = state.vault_ui.list.error.as_ref()
    {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            err.clone(),
            Style::default().fg(theme.negative),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
