//! Text validation for form inputs.
//!
//! Provides validators for:
//! - Required fields (non-empty)
//! - Length constraints (min/max)

use super::{ValidationResult, Validator};
use crate::text::{Locale, TextKey, format as text_format, t};

/// Validates that a field is not empty.
#[derive(Debug, Clone, Copy, Default)]
pub struct RequiredValidator;

impl Validator for RequiredValidator {
    fn validate(&self, value: &str) -> ValidationResult {
        validate_required(value, Locale::It)
    }
}

/// Validates string length constraints.
#[derive(Debug, Clone, Copy, Default)]
pub struct LengthValidator {
    /// Minimum length (inclusive). `None` means no minimum.
    pub min: Option<usize>,
    /// Maximum length (inclusive). `None` means no maximum.
    pub max: Option<usize>,
}

impl LengthValidator {
    /// Creates a validator with both minimum and maximum length.
    #[must_use]
    pub fn range(min: usize, max: usize) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
        }
    }
}

impl Validator for LengthValidator {
    fn validate(&self, value: &str) -> ValidationResult {
        validate_length(value, self.min, self.max, Locale::It)
    }
}

/// Validates that a field is not empty.
///
/// Whitespace-only strings are considered empty.
#[must_use]
pub fn validate_required(value: &str, locale: Locale) -> ValidationResult {
    if value.trim().is_empty() {
        ValidationResult::Invalid(t(locale, TextKey::ValidationRequired).to_string())
    } else {
        ValidationResult::Valid
    }
}

/// Validates string length.
///
/// # Arguments
///
/// * `value` - The string to validate
/// * `min` - Minimum length (inclusive), or `None` for no minimum
/// * `max` - Maximum length (inclusive), or `None` for no maximum
/// * `locale` - Locale for error messages
#[must_use]
pub fn validate_length(
    value: &str,
    min: Option<usize>,
    max: Option<usize>,
    locale: Locale,
) -> ValidationResult {
    let len = value.chars().count();

    if let Some(min_len) = min
        && len < min_len
    {
        return ValidationResult::Invalid(text_format(
            locale,
            TextKey::ValidationLengthMin,
            &[("min", &min_len.to_string())],
        ));
    }

    if let Some(max_len) = max
        && len > max_len
    {
        return ValidationResult::Invalid(text_format(
            locale,
            TextKey::ValidationLengthMax,
            &[("max", &max_len.to_string())],
        ));
    }

    ValidationResult::Valid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_required_empty() {
        assert!(validate_required("", Locale::It).is_invalid());
        assert!(validate_required("   ", Locale::It).is_invalid());
        assert!(validate_required("\t\n", Locale::It).is_invalid());
    }

    #[test]
    fn validate_required_non_empty() {
        assert!(validate_required("a", Locale::It).is_valid());
        assert!(validate_required("hello", Locale::It).is_valid());
        assert!(validate_required("  text  ", Locale::It).is_valid());
    }

    #[test]
    fn validate_length_min() {
        assert!(validate_length("ab", Some(3), None, Locale::It).is_invalid());
        assert!(validate_length("abc", Some(3), None, Locale::It).is_valid());
        assert!(validate_length("abcd", Some(3), None, Locale::It).is_valid());
    }

    #[test]
    fn validate_length_max() {
        assert!(validate_length("ab", None, Some(3), Locale::It).is_valid());
        assert!(validate_length("abc", None, Some(3), Locale::It).is_valid());
        assert!(validate_length("abcd", None, Some(3), Locale::It).is_invalid());
    }

    #[test]
    fn validate_length_range() {
        assert!(validate_length("ab", Some(3), Some(5), Locale::It).is_invalid());
        assert!(validate_length("abc", Some(3), Some(5), Locale::It).is_valid());
        assert!(validate_length("abcde", Some(3), Some(5), Locale::It).is_valid());
        assert!(validate_length("abcdef", Some(3), Some(5), Locale::It).is_invalid());
    }

    #[test]
    fn validate_length_unicode() {
        // Unicode characters should be counted correctly
        assert!(validate_length("日本語", Some(3), Some(3), Locale::It).is_valid());
        assert!(validate_length("🎉🎊", Some(2), Some(2), Locale::It).is_valid());
    }

    #[test]
    fn validate_length_no_constraints() {
        assert!(validate_length("", None, None, Locale::It).is_valid());
        assert!(validate_length("anything", None, None, Locale::It).is_valid());
    }

    #[test]
    fn required_validator_trait() {
        let validator = RequiredValidator;
        assert!(validator.validate("").is_invalid());
        assert!(validator.validate("text").is_valid());
    }

    #[test]
    fn length_validator_trait() {
        let validator = LengthValidator::range(2, 5);
        assert!(validator.validate("a").is_invalid());
        assert!(validator.validate("ab").is_valid());
        assert!(validator.validate("abcdef").is_invalid());
    }
}
