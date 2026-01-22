//! Text form field with length validation.

use ratatui::text::Line;

use crate::{
    ui::Theme,
    validation::{FieldState, LengthValidator, RequiredValidator, ValidationResult, Validator},
};

use super::{FormField, FormFieldRenderer};

/// A form field for entering text with optional length constraints.
#[derive(Debug, Clone)]
pub struct TextField {
    /// The label to display for this field.
    pub label: String,
    /// The current field state including value and validation.
    pub state: FieldState,
    /// Minimum length constraint.
    pub min_length: Option<usize>,
    /// Maximum length constraint.
    pub max_length: Option<usize>,
}

impl TextField {
    /// Creates a new text field with the given label.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            state: FieldState::default(),
            min_length: None,
            max_length: None,
        }
    }

    /// Sets whether the field is required.
    #[must_use]
    pub fn required(mut self, required: bool) -> Self {
        self.state.required = required;
        self
    }

    /// Sets the minimum length constraint.
    #[must_use]
    pub fn min_length(mut self, min: usize) -> Self {
        self.min_length = Some(min);
        self
    }

    /// Sets the maximum length constraint.
    #[must_use]
    pub fn max_length(mut self, max: usize) -> Self {
        self.max_length = Some(max);
        self
    }

    /// Sets both minimum and maximum length constraints.
    #[must_use]
    pub fn length_range(mut self, min: usize, max: usize) -> Self {
        self.min_length = Some(min);
        self.max_length = Some(max);
        self
    }

    /// Sets the initial value.
    #[must_use]
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.state.value = value.into();
        self
    }

    /// Validates the current value and updates the state.
    pub fn validate(&mut self) {
        // Check required first
        if self.state.required {
            let result = RequiredValidator.validate(&self.state.value);
            if result.is_invalid() {
                self.state.validation = result;
                return;
            }
        }

        // Check length constraints
        if self.min_length.is_some() || self.max_length.is_some() {
            let validator = LengthValidator {
                min: self.min_length,
                max: self.max_length,
            };
            let result = validator.validate(&self.state.value);
            if result.is_invalid() {
                self.state.validation = result;
                return;
            }
        }

        self.state.validation = ValidationResult::Valid;
    }

    /// Updates the field value and triggers validation.
    pub fn set_value(&mut self, value: impl Into<String>) {
        self.state.value = value.into();
        self.state.touched = true;
        self.validate();
    }

    /// Appends a character to the value.
    pub fn push(&mut self, c: char) {
        self.state.value.push(c);
        self.state.touched = true;
        self.validate();
    }

    /// Removes the last character from the value.
    pub fn pop(&mut self) {
        self.state.value.pop();
        self.validate();
    }

    /// Clears the field.
    pub fn clear(&mut self) {
        self.state.clear();
    }

    /// Returns the current character count.
    #[must_use]
    pub fn char_count(&self) -> usize {
        self.state.value.chars().count()
    }
}

impl FormField for TextField {
    fn render_line(&self, theme: &Theme) -> Line<'static> {
        FormFieldRenderer::render_input_field(&self.label, &self.state.value, &self.state, theme)
    }

    fn value(&self) -> &str {
        &self.state.value
    }

    fn state(&self) -> &FieldState {
        &self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_text_field() {
        let field = TextField::new("Name");
        assert_eq!(field.label, "Name");
        assert!(!field.state.required);
    }

    #[test]
    fn validate_empty_optional() {
        let mut field = TextField::new("Note");
        field.validate();
        assert!(field.state.validation.is_valid());
    }

    #[test]
    fn validate_empty_required() {
        let mut field = TextField::new("Name").required(true);
        field.validate();
        assert!(field.state.validation.is_invalid());
    }

    #[test]
    fn validate_min_length() {
        let mut field = TextField::new("Name").min_length(3).with_value("ab");
        field.validate();
        assert!(field.state.validation.is_invalid());

        field.set_value("abc");
        assert!(field.state.validation.is_valid());
    }

    #[test]
    fn validate_max_length() {
        let mut field = TextField::new("Code").max_length(5).with_value("abcdef");
        field.validate();
        assert!(field.state.validation.is_invalid());

        field.set_value("abcde");
        assert!(field.state.validation.is_valid());
    }

    #[test]
    fn char_count_works() {
        let field = TextField::new("Note").with_value("hello");
        assert_eq!(field.char_count(), 5);

        let field_unicode = TextField::new("Note").with_value("日本語");
        assert_eq!(field_unicode.char_count(), 3);
    }

    #[test]
    fn push_and_pop() {
        let mut field = TextField::new("Note");
        field.push('a');
        field.push('b');
        assert_eq!(field.state.value, "ab");

        field.pop();
        assert_eq!(field.state.value, "a");
    }
}
