//! Amount validation for currency inputs.
//!
//! Supports parsing amounts in formats like:
//! - `123` → 12300 (cents)
//! - `123.45` → 12345 (cents)
//! - `123,45` → 12345 (cents, European format)
//! - `1.234,56` → 123456 (thousands separator with comma decimal)
//! - `1,234.56` → 123456 (thousands separator with period decimal)

use super::{ValidationResult, Validator};
use crate::text::{Locale, TextKey, t};

/// Validates amount strings for currency input.
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct AmountValidator {
    /// If `true`, the amount must be positive (> 0).
    pub require_positive: bool,
    /// If `true`, the amount must be non-negative (>= 0).
    pub allow_zero: bool,
}

#[allow(dead_code)]
impl AmountValidator {
    /// Creates a new validator that requires positive amounts.
    #[must_use]
    pub fn positive() -> Self {
        Self {
            require_positive: true,
            allow_zero: false,
        }
    }

    /// Creates a new validator that allows zero and positive amounts.
    #[must_use]
    pub fn non_negative() -> Self {
        Self {
            require_positive: false,
            allow_zero: true,
        }
    }
}

impl Validator for AmountValidator {
    fn validate(&self, value: &str) -> ValidationResult {
        validate_amount(value, self.require_positive, Locale::It)
    }
}

/// Parses an amount string into minor units (cents).
///
/// Returns `None` if the string is not a valid amount format.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(parse_amount("123"), Some(12300));
/// assert_eq!(parse_amount("123.45"), Some(12345));
/// assert_eq!(parse_amount("123,45"), Some(12345));
/// assert_eq!(parse_amount("1.234,56"), Some(123456));
/// assert_eq!(parse_amount("invalid"), None);
/// ```
#[must_use]
pub fn parse_amount(value: &str) -> Option<i64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Handle negative sign
    let (is_negative, num_str) = if let Some(rest) = trimmed.strip_prefix('-') {
        (true, rest.trim())
    } else {
        (false, trimmed)
    };

    if num_str.is_empty() {
        return None;
    }

    // Detect format by looking at separator positions
    let last_dot = num_str.rfind('.');
    let last_comma = num_str.rfind(',');

    let (integer_part, decimal_part) = match (last_dot, last_comma) {
        // No separators: integer only
        (None, None) => (num_str, None),

        // Only dots: decimal separator (not thousands when single dot)
        (Some(dot_pos), None) => {
            let after = &num_str[dot_pos + 1..];
            if !after.chars().all(|c| c.is_ascii_digit()) || after.is_empty() {
                return None;
            }
            // Single dot is always decimal separator
            (&num_str[..dot_pos], Some(after))
        }

        // Only commas: decimal separator (not thousands when single comma)
        (None, Some(comma_pos)) => {
            let after = &num_str[comma_pos + 1..];
            if !after.chars().all(|c| c.is_ascii_digit()) || after.is_empty() {
                return None;
            }
            // Single comma is always decimal separator
            (&num_str[..comma_pos], Some(after))
        }

        // Both separators: determine which is decimal
        (Some(dot_pos), Some(comma_pos)) => {
            if dot_pos > comma_pos {
                // Format: 1,234.56 (comma thousands, dot decimal)
                let clean: String = num_str[..dot_pos].chars().filter(|c| *c != ',').collect();
                let decimal = &num_str[dot_pos + 1..];
                if !decimal.chars().all(|c| c.is_ascii_digit()) {
                    return None;
                }
                return parse_with_decimal(&clean, decimal, is_negative);
            } else {
                // Format: 1.234,56 (dot thousands, comma decimal)
                let clean: String = num_str[..comma_pos].chars().filter(|c| *c != '.').collect();
                let decimal = &num_str[comma_pos + 1..];
                if !decimal.chars().all(|c| c.is_ascii_digit()) {
                    return None;
                }
                return parse_with_decimal(&clean, decimal, is_negative);
            }
        }
    };

    // Parse integer part
    let clean_integer: String = integer_part
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    if clean_integer.is_empty() {
        return None;
    }

    let integer: i64 = clean_integer.parse().ok()?;

    // Parse decimal part
    let cents = if let Some(dec) = decimal_part {
        if dec.len() == 1 {
            dec.parse::<i64>().ok()? * 10
        } else if dec.len() == 2 {
            dec.parse::<i64>().ok()?
        } else if dec.len() > 2 {
            // Truncate to 2 decimal places
            dec[..2].parse::<i64>().ok()?
        } else {
            0
        }
    } else {
        0
    };

    let result = integer.checked_mul(100)?.checked_add(cents)?;
    Some(if is_negative { -result } else { result })
}

fn parse_with_decimal(integer_str: &str, decimal_str: &str, is_negative: bool) -> Option<i64> {
    if integer_str.is_empty() {
        return None;
    }

    let integer: i64 = integer_str.parse().ok()?;
    let cents = if decimal_str.len() == 1 {
        decimal_str.parse::<i64>().ok()? * 10
    } else if decimal_str.len() >= 2 {
        decimal_str[..2].parse::<i64>().ok()?
    } else {
        0
    };

    let result = integer.checked_mul(100)?.checked_add(cents)?;
    Some(if is_negative { -result } else { result })
}

/// Validates an amount string.
///
/// # Arguments
///
/// * `value` - The string to validate
/// * `require_positive` - If true, amount must be > 0; if false, must be >= 0
/// * `locale` - Locale for error messages
#[must_use]
pub fn validate_amount(value: &str, require_positive: bool, locale: Locale) -> ValidationResult {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return ValidationResult::Valid; // Empty is not validated here; use RequiredValidator
    }

    match parse_amount(trimmed) {
        None => ValidationResult::Invalid(t(locale, TextKey::ValidationAmountInvalid).to_string()),
        Some(cents) => {
            if cents < 0 || (require_positive && cents == 0) {
                ValidationResult::Invalid(t(locale, TextKey::ValidationAmountPositive).to_string())
            } else {
                ValidationResult::Valid
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_amount_integer() {
        assert_eq!(parse_amount("123"), Some(12300));
        assert_eq!(parse_amount("0"), Some(0));
        assert_eq!(parse_amount("1"), Some(100));
    }

    #[test]
    fn parse_amount_decimal_dot() {
        assert_eq!(parse_amount("123.45"), Some(12345));
        assert_eq!(parse_amount("123.4"), Some(12340));
        assert_eq!(parse_amount("123.456"), Some(12345)); // Truncated
        assert_eq!(parse_amount("0.99"), Some(99));
    }

    #[test]
    fn parse_amount_decimal_comma() {
        assert_eq!(parse_amount("123,45"), Some(12345));
        assert_eq!(parse_amount("123,4"), Some(12340));
        assert_eq!(parse_amount("0,99"), Some(99));
    }

    #[test]
    fn parse_amount_thousands_separator() {
        // Single separator is always decimal, not thousands
        assert_eq!(parse_amount("1.234"), Some(123)); // 1.234 = 1.23 cents (truncated)
        assert_eq!(parse_amount("1,234"), Some(123)); // 1,234 = 1.23 cents (truncated)
        // Both separators: can detect thousands vs decimal
        assert_eq!(parse_amount("1.234,56"), Some(123456)); // European
        assert_eq!(parse_amount("1,234.56"), Some(123456)); // US
    }

    #[test]
    fn parse_amount_negative() {
        assert_eq!(parse_amount("-123"), Some(-12300));
        assert_eq!(parse_amount("-123.45"), Some(-12345));
        assert_eq!(parse_amount("- 50"), Some(-5000));
    }

    #[test]
    fn parse_amount_invalid() {
        assert_eq!(parse_amount(""), None);
        assert_eq!(parse_amount("abc"), None);
        assert_eq!(parse_amount("-"), None);
        assert_eq!(parse_amount("12."), None);
        assert_eq!(parse_amount(".45"), None);
    }

    #[test]
    fn validate_amount_positive() {
        assert!(validate_amount("100", true, Locale::It).is_valid());
        assert!(validate_amount("0.01", true, Locale::It).is_valid());
        assert!(validate_amount("0", true, Locale::It).is_invalid());
        assert!(validate_amount("-50", true, Locale::It).is_invalid());
    }

    #[test]
    fn validate_amount_non_negative() {
        assert!(validate_amount("100", false, Locale::It).is_valid());
        assert!(validate_amount("0", false, Locale::It).is_valid());
        assert!(validate_amount("-50", false, Locale::It).is_invalid());
    }

    #[test]
    fn validate_amount_empty_is_valid() {
        // Empty string is valid; use RequiredValidator separately
        assert!(validate_amount("", true, Locale::It).is_valid());
        assert!(validate_amount("   ", true, Locale::It).is_valid());
    }

    #[test]
    fn validator_trait() {
        let validator = AmountValidator::positive();
        assert!(validator.validate("100").is_valid());
        assert!(validator.validate("0").is_invalid());
    }
}
