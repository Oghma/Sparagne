//! Amount form field with currency validation.

use crate::app::TextInputField;
use crate::validation::{AmountValidator, FieldState, ValidationResult, Validator};

/// A form field for entering monetary amounts.
///
/// Provides real-time validation and formatting feedback for currency input.
#[derive(Debug, Clone)]
pub struct AmountField {
    /// The label to display for this field.
    pub label: String,
    /// The current field state including value and validation.
    pub state: FieldState,
    /// Whether to require a positive amount.
    pub require_positive: bool,
}

impl AmountField {
    /// Creates a new amount field with the given label.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            state: FieldState {
                required: true,
                ..Default::default()
            },
            require_positive: true,
        }
    }

    /// Sets whether the amount must be positive.
    #[must_use]
    pub fn require_positive(mut self, require: bool) -> Self {
        self.require_positive = require;
        self
    }

    /// Sets whether the field is required.
    #[must_use]
    pub fn required(mut self, required: bool) -> Self {
        self.state.required = required;
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
        let validator = AmountValidator {
            require_positive: self.require_positive,
        };

        // Check required first
        if self.state.required && self.state.value.trim().is_empty() {
            self.state.validation = ValidationResult::Invalid("Importo obbligatorio".to_string());
            return;
        }

        self.state.validation = validator.validate(&self.state.value);
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

    /// Returns the current value of the field.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.state.value
    }
}

impl TextInputField for AmountField {
    fn push(&mut self, c: char) {
        self.state.value.push(c);
        self.state.touched = true;
        self.validate();
    }

    fn pop(&mut self) {
        self.state.value.pop();
        self.validate();
    }

    fn value(&self) -> &str {
        &self.state.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_amount_field() {
        let field = AmountField::new("Amount");
        assert_eq!(field.label, "Amount");
        assert!(field.state.required);
        assert!(field.require_positive);
    }

    #[test]
    fn validate_empty_required() {
        let mut field = AmountField::new("Amount");
        field.validate();
        assert!(field.state.validation.is_invalid());
    }

    #[test]
    fn validate_valid_amount() {
        let mut field = AmountField::new("Amount").with_value("100.50");
        field.validate();
        assert!(field.state.validation.is_valid());
    }

    #[test]
    fn validate_invalid_format() {
        let mut field = AmountField::new("Amount").with_value("abc");
        field.validate();
        assert!(field.state.validation.is_invalid());
    }

    #[test]
    fn push_updates_value() {
        let mut field = AmountField::new("Amount");
        field.push('1');
        field.push('0');
        field.push('0');
        assert_eq!(field.state.value, "100");
        assert!(field.state.touched);
    }

    #[test]
    fn pop_removes_last_char() {
        let mut field = AmountField::new("Amount").with_value("100");
        field.pop();
        assert_eq!(field.state.value, "10");
    }
}
