//! Reusable form field components with validation support.
//!
//! This module provides UI components for form fields that integrate with
//! the validation system to show real-time validation feedback.

mod amount;
mod date;
mod text;

pub use amount::AmountField;
pub use date::DateField;
pub use text::TextField;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::ui::Theme;
use crate::validation::{FieldState, ValidationResult};

/// Common rendering utilities for form fields.
pub struct FormFieldRenderer;

impl FormFieldRenderer {
    /// Renders a labeled form field with validation feedback.
    ///
    /// Returns the rendered line suitable for inclusion in a form.
    #[must_use]
    pub fn render_field(
        label: &str,
        value: &str,
        state: &FieldState,
        theme: &Theme,
    ) -> Line<'static> {
        let label_style = Self::label_style(state.focused, theme);
        let value_style = Self::value_style(state.focused, theme);

        let mut spans = vec![
            Span::styled(format!("{label}: "), label_style),
            Span::styled(value.to_string(), value_style),
        ];

        // Add required indicator
        if state.required {
            spans.insert(1, Span::styled("* ", Style::default().fg(theme.error)));
        }

        // Add validation error if touched and invalid
        if state.should_show_error() {
            if let Some(error) = state.validation.error_message() {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("⚠ {error}"),
                    Style::default().fg(theme.error),
                ));
            }
        }

        Line::from(spans)
    }

    /// Renders a form field with a cursor indicator for text input.
    #[must_use]
    pub fn render_input_field(
        label: &str,
        value: &str,
        state: &FieldState,
        theme: &Theme,
    ) -> Line<'static> {
        let label_style = Self::label_style(state.focused, theme);
        let value_style = Self::value_style(state.focused, theme);

        let display_value = if state.focused {
            format!("{value}▏")
        } else {
            value.to_string()
        };

        let mut spans = vec![
            Span::styled(format!("{label}: "), label_style),
            Span::styled(display_value, value_style),
        ];

        // Add required indicator
        if state.required {
            spans.insert(1, Span::styled("* ", Style::default().fg(theme.error)));
        }

        // Add validation status indicator
        if state.touched {
            let (icon, color) = match &state.validation {
                ValidationResult::Valid => ("✓", theme.positive),
                ValidationResult::Invalid(_) => ("✗", theme.error),
                ValidationResult::Pending => ("…", theme.warning),
            };
            spans.push(Span::raw(" "));
            spans.push(Span::styled(icon.to_string(), Style::default().fg(color)));

            // Show error message
            if let ValidationResult::Invalid(msg) = &state.validation {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(msg.clone(), Style::default().fg(theme.error)));
            }
        }

        Line::from(spans)
    }

    fn label_style(focused: bool, theme: &Theme) -> Style {
        if focused {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        }
    }

    fn value_style(focused: bool, theme: &Theme) -> Style {
        if focused {
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        }
    }
}

/// Trait for form field components that can be rendered.
pub trait FormField {
    /// Renders the field to a single line.
    fn render_line(&self, theme: &Theme) -> Line<'static>;

    /// Returns the current value of the field.
    fn value(&self) -> &str;

    /// Returns the field state.
    fn state(&self) -> &FieldState;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_field_shows_label_and_value() {
        let theme = Theme::default();
        let state = FieldState::new();
        let line = FormFieldRenderer::render_field("Amount", "100.00", &state, &theme);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Amount:"));
        assert!(text.contains("100.00"));
    }

    #[test]
    fn render_field_shows_required_indicator() {
        let theme = Theme::default();
        let state = FieldState {
            required: true,
            ..Default::default()
        };
        let line = FormFieldRenderer::render_field("Amount", "", &state, &theme);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("*"));
    }

    #[test]
    fn render_field_shows_error_when_touched() {
        let theme = Theme::default();
        let state = FieldState {
            touched: true,
            validation: ValidationResult::Invalid("Test error".to_string()),
            ..Default::default()
        };
        let line = FormFieldRenderer::render_field("Amount", "abc", &state, &theme);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Test error"));
    }

    #[test]
    fn render_field_hides_error_when_not_touched() {
        let theme = Theme::default();
        let state = FieldState {
            touched: false,
            validation: ValidationResult::Invalid("Test error".to_string()),
            ..Default::default()
        };
        let line = FormFieldRenderer::render_field("Amount", "abc", &state, &theme);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(!text.contains("Test error"));
    }

    #[test]
    fn render_input_field_shows_cursor_when_focused() {
        let theme = Theme::default();
        let state = FieldState {
            focused: true,
            ..Default::default()
        };
        let line = FormFieldRenderer::render_input_field("Note", "hello", &state, &theme);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("hello▏"));
    }
}
