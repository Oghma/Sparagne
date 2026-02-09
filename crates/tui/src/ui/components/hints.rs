use ratatui::{style::Style, text::Span};

use crate::{
    text::{Locale, TextKey, t},
    ui::theme::Theme,
};

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
pub fn help_hint(locale: Locale) -> KeyHint {
    KeyHint::new("?", t(locale, TextKey::HintHelp))
}

/// Common hint groups for reuse across screens.
pub mod common {
    use super::KeyHint;
    use crate::text::{Locale, TextKey, t};

    /// Hints for form editing.
    pub fn form_editing(locale: Locale) -> Vec<KeyHint> {
        vec![
            KeyHint::new("Enter", t(locale, TextKey::HintSave)),
            KeyHint::new("Esc", t(locale, TextKey::HintCancel)),
        ]
    }

    /// Hints for detail views.
    pub fn detail_view(locale: Locale) -> Vec<KeyHint> {
        vec![KeyHint::new("Esc", t(locale, TextKey::HintBack))]
    }

    /// Section navigation shortcuts.
    pub fn section_shortcuts(locale: Locale) -> Vec<KeyHint> {
        vec![
            KeyHint::new("h", t(locale, TextKey::HintHome)),
            KeyHint::new("t", t(locale, TextKey::HintTransactions)),
            KeyHint::new("a", t(locale, TextKey::HintAccounts)),
            KeyHint::new("y", t(locale, TextKey::HintAnalytics)),
            KeyHint::new("s", t(locale, TextKey::HintSettings)),
        ]
    }

    /// Quick add shortcut.
    pub fn quick_add(locale: Locale) -> KeyHint {
        KeyHint::new("n", t(locale, TextKey::HintQuickAdd))
    }
}
