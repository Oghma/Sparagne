use ratatui::{style::Style, text::Span};

use crate::ui::theme::Theme;

/// A keyboard hint consisting of a key and its action.
#[derive(Debug, Clone)]
pub struct KeyHint {
    pub key: String,
    pub action: String,
}

impl KeyHint {
    pub fn new(key: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            action: action.into(),
        }
    }
}

/// Converts a list of key hints into styled spans for rendering.
pub fn hints_to_spans(hints: &[KeyHint], theme: &Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    for (i, hint) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Style::default().fg(theme.text_muted)));
        }
        spans.push(Span::styled(
            format!("[{}]", hint.key),
            Style::default().fg(theme.accent),
        ));
        spans.push(Span::styled(
            format!(" {}", hint.action),
            Style::default().fg(theme.text_muted),
        ));
    }

    spans
}

/// Creates a separator span for dividing hint groups.
pub fn hint_separator(theme: &Theme) -> Span<'static> {
    Span::styled("  │  ", Style::default().fg(theme.border))
}

/// Common hint groups for reuse across screens.
pub mod common {
    use super::KeyHint;

    /// Navigation hints for list views.
    pub fn list_navigation() -> Vec<KeyHint> {
        vec![
            KeyHint::new("↑↓", "select"),
            KeyHint::new("Enter", "detail"),
        ]
    }

    /// Hints for form editing.
    pub fn form_editing() -> Vec<KeyHint> {
        vec![
            KeyHint::new("Tab", "next"),
            KeyHint::new("Enter", "save"),
            KeyHint::new("Esc", "cancel"),
        ]
    }

    /// Hints for detail views.
    pub fn detail_view() -> Vec<KeyHint> {
        vec![KeyHint::new("b", "back"), KeyHint::new("Esc", "back")]
    }

    /// Global application shortcuts.
    pub fn global_shortcuts() -> Vec<KeyHint> {
        vec![
            KeyHint::new("Ctrl+F", "search"),
            KeyHint::new("Ctrl+P", "cmd"),
            KeyHint::new("q", "quit"),
        ]
    }

    /// Section navigation shortcuts.
    pub fn section_shortcuts() -> Vec<KeyHint> {
        vec![
            KeyHint::new("h", "home"),
            KeyHint::new("t", "txn"),
            KeyHint::new("w", "wallet"),
            KeyHint::new("a", "accounts"),
            KeyHint::new("g", "categories"),
            KeyHint::new("s", "stats"),
        ]
    }

    /// Quick add shortcuts.
    pub fn quick_add() -> Vec<KeyHint> {
        vec![KeyHint::new("n", "quick add"), KeyHint::new("N", "new txn")]
    }

    /// Item action shortcuts (uniform contract).
    pub fn item_actions() -> Vec<KeyHint> {
        vec![
            KeyHint::new("Enter", "detail"),
            KeyHint::new("e", "edit"),
            KeyHint::new("d", "delete"),
        ]
    }
}
