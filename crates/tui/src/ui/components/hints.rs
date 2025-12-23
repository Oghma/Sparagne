use ratatui::{
    style::Style,
    text::Span,
};

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
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            hint.key.clone(),
            Style::default().fg(theme.accent),
        ));
        spans.push(Span::raw(format!(" {}", hint.action)));
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

    /// Hints for CRUD operations.
    pub fn crud_operations() -> Vec<KeyHint> {
        vec![
            KeyHint::new("c", "create"),
            KeyHint::new("e", "edit"),
            KeyHint::new("d", "delete"),
        ]
    }

    /// Global application shortcuts.
    pub fn global_shortcuts() -> Vec<KeyHint> {
        vec![
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
            KeyHint::new("f", "flow"),
            KeyHint::new("v", "vault"),
            KeyHint::new("s", "stats"),
        ]
    }
}

/// Trait for providing context-aware hints.
pub trait HintProvider {
    /// Returns the keyboard hints for the current context.
    fn hints(&self) -> Vec<KeyHint>;
}

