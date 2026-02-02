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

/// Help hint to remind users about the help overlay.
pub fn help_hint() -> KeyHint {
    KeyHint::new("?", "help")
}

/// Common hint groups for reuse across screens.
pub mod common {
    use super::KeyHint;

    /// Hints for form editing.
    pub fn form_editing() -> Vec<KeyHint> {
        vec![
            KeyHint::new("Enter", "save"),
            KeyHint::new("Esc", "cancel"),
        ]
    }

    /// Hints for detail views.
    pub fn detail_view() -> Vec<KeyHint> {
        vec![KeyHint::new("Esc", "back")]
    }

    /// Section navigation shortcuts.
    pub fn section_shortcuts() -> Vec<KeyHint> {
        vec![
            KeyHint::new("h", "home"),
            KeyHint::new("t", "txn"),
            KeyHint::new("a", "accounts"),
            KeyHint::new("y", "analytics"),
            KeyHint::new("s", "settings"),
        ]
    }

    /// Quick add shortcut.
    pub fn quick_add() -> KeyHint {
        KeyHint::new("n", "quick add")
    }
}
