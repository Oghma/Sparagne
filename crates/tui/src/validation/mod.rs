//! Form validation utilities for the TUI.
//!
//! This module provides reusable validation functions for form inputs.
//! All validators are pure functions that return a [`ValidationResult`].

mod amount;
mod date;
mod text;

pub use amount::AmountValidator;
pub use date::DateField;
pub use text::{LengthValidator, RequiredValidator};

/// Result of validating a form field value.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ValidationResult {
    /// Input is valid.
    #[default]
    Valid,
    /// Input is invalid with an error message.
    Invalid(String),
}

impl ValidationResult {
    /// Returns `true` if the validation passed.
    #[cfg(test)]
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
            Self::Valid => None,
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

impl FieldState {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_result_is_invalid() {
        assert!(!ValidationResult::Valid.is_invalid());
        assert!(ValidationResult::Invalid("error".to_string()).is_invalid());
    }

    #[test]
    fn validation_result_error_message() {
        assert_eq!(ValidationResult::Valid.error_message(), None);
        assert_eq!(
            ValidationResult::Invalid("test error".to_string()).error_message(),
            Some("test error")
        );
    }
}
