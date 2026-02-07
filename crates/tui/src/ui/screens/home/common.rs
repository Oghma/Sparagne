//! Shared utilities for home screen rendering.
//!
//! Re-exports consolidated helpers from [`crate::ui::common`] and provides
//! home-specific rendering helpers.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::ui::theme::Theme;

// Re-exports from the consolidated common module.
pub(crate) use crate::ui::common::{ICON_EXPENSE, ICON_INCOME, get_currency, truncate};

/// Renders an empty state message with hint.
pub fn render_empty_state(
    frame: &mut Frame<'_>,
    area: Rect,
    message: &str,
    hint: &str,
    theme: &Theme,
) {
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(message, Style::default().fg(theme.text_muted))),
        Line::from(""),
        Line::from(Span::styled(hint, Style::default().fg(theme.text_muted))),
    ];

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}
