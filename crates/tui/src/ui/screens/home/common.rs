//! Shared utilities for home screen rendering.

use engine::Currency;

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::ui::theme::Theme;

/// Transaction type icons
pub const ICON_INCOME: &str = "▲";
pub const ICON_EXPENSE: &str = "▼";
pub const ICON_REFUND: &str = "↩";
pub const ICON_TRANSFER: &str = "⇄";

/// Gets the currency for the current vault.
pub fn get_currency(state: &crate::app::AppState) -> Currency {
    state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur)
}

/// Maps API currency type to engine currency type.
pub fn map_currency(currency: &api_types::Currency) -> Currency {
    match currency {
        api_types::Currency::Eur => Currency::Eur,
    }
}

/// Truncates a string to the given maximum length, adding ellipsis if needed.
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_len - 1).collect::<String>())
    }
}

/// Renders an empty state message with hint.
pub fn render_empty_state(frame: &mut Frame<'_>, area: Rect, message: &str, hint: &str, theme: &Theme) {
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(message, Style::default().fg(theme.text_muted))),
        Line::from(""),
        Line::from(Span::styled(hint, Style::default().fg(theme.text_muted))),
    ];

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}
