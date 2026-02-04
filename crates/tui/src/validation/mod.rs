//! Form validation utilities for the TUI.
//!
//! This module provides reusable validation functions for form inputs.
//! All validators are pure functions that return a [`ValidationResult`].

mod amount;
mod date;
mod text;

pub use amount::AmountValidator;
pub use date::{DateField, DateFormat, DateValidator};
pub use text::{LengthValidator, RequiredValidator};

/// Result of validating a form field value.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ValidationResult {
    /// Input is valid.
    #[default]
    Valid,
    /// Input is invalid with an error message.
    Invalid(String),
    /// Validation in progress (for async validation, future use).
    Pending,
}

impl ValidationResult {
    /// Returns `true` if the validation passed.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid)
    }

    /// Returns `true` if the validation failed.
    #[must_use]
    pub fn is_invalid(&self) -> bool {
        matches!(self, Self::Invalid(_))
    }

    /// Returns the error message if validation failed.
    #[must_use]
    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Invalid(msg) => Some(msg),
            _ => None,
        }
    }
}

/// Trait for validators that can check a string value.
pub trait Validator {
    /// Validates the given value and returns a result.
    fn validate(&self, value: &str) -> ValidationResult;
}

/// State of a form field including its validation status.
#[derive(Debug, Clone, Default)]
pub struct FieldState {
    /// Current value of the field.
    pub value: String,
    /// Validation result for the current value.
    pub validation: ValidationResult,
    /// Whether the user has interacted with this field.
    pub touched: bool,
    /// Whether the field is currently focused.
    pub focused: bool,
    /// Whether the field is required.
    pub required: bool,
}

impl FieldState {
    /// Creates a new empty field state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if the field should display an error.
    ///
    /// Errors are shown only if the field has been touched and is invalid.
    #[must_use]
    pub fn should_show_error(&self) -> bool {
        self.touched && self.validation.is_invalid()
    }

    /// Clears the field value and validation state.
    pub fn clear(&mut self) {
        self.value.clear();
        self.validation = ValidationResult::Valid;
        self.touched = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_result_is_valid() {
        assert!(ValidationResult::Valid.is_valid());
        assert!(!ValidationResult::Invalid("error".to_string()).is_valid());
        assert!(!ValidationResult::Pending.is_valid());
    }

    #[test]
    fn validation_result_is_invalid() {
        assert!(!ValidationResult::Valid.is_invalid());
        assert!(ValidationResult::Invalid("error".to_string()).is_invalid());
        assert!(!ValidationResult::Pending.is_invalid());
    }

    #[test]
    fn validation_result_error_message() {
        assert_eq!(ValidationResult::Valid.error_message(), None);
        assert_eq!(
            ValidationResult::Invalid("test error".to_string()).error_message(),
            Some("test error")
        );
        assert_eq!(ValidationResult::Pending.error_message(), None);
    }

    #[test]
    fn field_state_should_show_error() {
        let mut field = FieldState::new();
        assert!(!field.should_show_error());

        field.touched = true;
        assert!(!field.should_show_error());

        field.validation = ValidationResult::Invalid("error".to_string());
        assert!(field.should_show_error());

        field.touched = false;
        assert!(!field.should_show_error());
    }

    #[test]
    fn field_state_clear() {
        let mut field = FieldState {
            value: "test".to_string(),
            validation: ValidationResult::Invalid("error".to_string()),
            touched: true,
            focused: true,
            required: true,
        };

        field.clear();

        assert!(field.value.is_empty());
        assert!(field.validation.is_valid());
        assert!(!field.touched);
        assert!(field.focused); // focused is not cleared
        assert!(field.required); // required is not cleared
    }
}
