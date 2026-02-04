//! Date validation for form inputs.
//!
//! Supports parsing dates in multiple formats:
//! - `YYYY-MM-DD` (ISO format)
//! - `YYYY-MM-DD HH:MM` (ISO with time)
//! - `DD/MM/YYYY` (European format)
//! - `DD/MM/YYYY HH:MM` (European with time)
//! - `DD-MM-YYYY` (European with dashes)
//! - `DD-MM-YYYY HH:MM` (European with dashes and time)

use super::ValidationResult;
use crate::text::{Locale, TextKey, t};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, TimeZone};
use chrono_tz::Tz;

/// Supported date input formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateFormat {
    /// `YYYY-MM-DD`
    Iso,
    /// `YYYY-MM-DD HH:MM`
    IsoTime,
    /// `DD/MM/YYYY`
    European,
    /// `DD/MM/YYYY HH:MM`
    EuropeanTime,
    /// `DD-MM-YYYY`
    EuropeanDash,
    /// `DD-MM-YYYY HH:MM`
    EuropeanDashTime,
}

impl DateFormat {
    /// Returns all supported formats in order of preference.
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[
            Self::IsoTime,
            Self::Iso,
            Self::EuropeanTime,
            Self::European,
            Self::EuropeanDashTime,
            Self::EuropeanDash,
        ]
    }

    /// Returns the format string for parsing.
    #[must_use]
    pub fn pattern(self) -> &'static str {
        match self {
            Self::Iso => "%Y-%m-%d",
            Self::IsoTime => "%Y-%m-%d %H:%M",
            Self::European => "%d/%m/%Y",
            Self::EuropeanTime => "%d/%m/%Y %H:%M",
            Self::EuropeanDash => "%d-%m-%Y",
            Self::EuropeanDashTime => "%d-%m-%Y %H:%M",
        }
    }

    /// Returns whether this format includes time.
    #[must_use]
    pub fn has_time(self) -> bool {
        matches!(
            self,
            Self::IsoTime | Self::EuropeanTime | Self::EuropeanDashTime
        )
    }
}

/// Result of parsing a date string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDate {
    /// The parsed datetime with timezone.
    pub datetime: DateTime<FixedOffset>,
    /// The format that was used to parse.
    pub format: DateFormat,
}

/// Attempts to parse a date string using multiple formats.
///
/// Returns `Some(ParsedDate)` if any format matches, `None` otherwise.
///
/// # Arguments
///
/// * `value` - The string to parse
/// * `formats` - Formats to try, in order
/// * `timezone` - Timezone for dates without timezone info
#[must_use]
pub fn parse_date(value: &str, formats: &[DateFormat], timezone: Tz) -> Option<ParsedDate> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    for format in formats {
        if let Some(datetime) = try_parse_format(trimmed, *format, timezone) {
            return Some(ParsedDate {
                datetime,
                format: *format,
            });
        }
    }

    None
}

fn try_parse_format(
    value: &str,
    format: DateFormat,
    timezone: Tz,
) -> Option<DateTime<FixedOffset>> {
    if format.has_time() {
        // Parse as datetime
        let naive = NaiveDateTime::parse_from_str(value, format.pattern()).ok()?;
        let local = timezone.from_local_datetime(&naive).single()?;
        Some(local.fixed_offset())
    } else {
        // Parse as date only, use midnight
        let naive_date = NaiveDate::parse_from_str(value, format.pattern()).ok()?;
        let naive_datetime = naive_date.and_hms_opt(0, 0, 0)?;
        let local = timezone.from_local_datetime(&naive_datetime).single()?;
        Some(local.fixed_offset())
    }
}

/// Validates a date string.
///
/// # Arguments
///
/// * `value` - The string to validate
/// * `formats` - Formats to try
/// * `timezone` - Timezone for parsing
/// * `locale` - Locale for error messages
#[must_use]
pub fn validate_date(
    value: &str,
    formats: &[DateFormat],
    timezone: Tz,
    locale: Locale,
) -> ValidationResult {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return ValidationResult::Valid; // Empty is not validated here; use RequiredValidator
    }

    match parse_date(trimmed, formats, timezone) {
        Some(_) => ValidationResult::Valid,
        None => ValidationResult::Invalid(t(locale, TextKey::ValidationDateInvalid).to_string()),
    }
}

/// A form field for date input with real-time validation.
///
/// Supports multiple date formats and provides visual feedback.
#[derive(Debug, Clone)]
pub struct DateField {
    /// Current input value.
    pub value: String,
    /// Timezone for parsing dates.
    pub timezone: Tz,
    /// Locale for error messages.
    pub locale: Locale,
    /// Whether the field is required.
    pub required: bool,
    /// Cached validation result.
    validation: ValidationResult,
}

impl Default for DateField {
    fn default() -> Self {
        Self {
            value: String::new(),
            timezone: chrono_tz::Europe::Rome,
            locale: Locale::It,
            required: false,
            validation: ValidationResult::Valid,
        }
    }
}

impl DateField {
    /// Creates a new empty date field.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new date field with an initial value.
    #[must_use]
    pub fn with_value(value: impl Into<String>) -> Self {
        let value = value.into();
        let mut field = Self {
            value,
            ..Default::default()
        };
        field.revalidate();
        field
    }

    /// Pushes a character to the input.
    pub fn push(&mut self, ch: char) {
        self.value.push(ch);
        self.revalidate();
    }

    /// Removes the last character from the input.
    pub fn pop(&mut self) {
        self.value.pop();
        self.revalidate();
    }

    /// Revalidates the current value.
    fn revalidate(&mut self) {
        let trimmed = self.value.trim();
        if trimmed.is_empty() {
            if self.required {
                self.validation =
                    ValidationResult::Invalid(t(self.locale, TextKey::ValidationDateRequired).to_string());
            } else {
                self.validation = ValidationResult::Valid;
            }
            return;
        }

        self.validation = validate_date(trimmed, DateFormat::all(), self.timezone, self.locale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rome() -> Tz {
        chrono_tz::Europe::Rome
    }

    #[test]
    fn parse_date_iso() {
        let parsed = parse_date("2024-03-15", DateFormat::all(), rome())
            .unwrap_or_else(|| panic!("expected ISO date to parse"));
        assert_eq!(parsed.format, DateFormat::Iso);
        assert_eq!(parsed.datetime.format("%Y-%m-%d").to_string(), "2024-03-15");
    }

    #[test]
    fn parse_date_iso_time() {
        let parsed = parse_date("2024-03-15 14:30", DateFormat::all(), rome())
            .unwrap_or_else(|| panic!("expected ISO date time to parse"));
        assert_eq!(parsed.format, DateFormat::IsoTime);
        assert_eq!(parsed.datetime.format("%H:%M").to_string(), "14:30");
    }

    #[test]
    fn parse_date_european() {
        let parsed = parse_date("15/03/2024", DateFormat::all(), rome())
            .unwrap_or_else(|| panic!("expected European date to parse"));
        assert_eq!(parsed.format, DateFormat::European);
        assert_eq!(parsed.datetime.format("%Y-%m-%d").to_string(), "2024-03-15");
    }

    #[test]
    fn parse_date_european_time() {
        let parsed = parse_date("15/03/2024 14:30", DateFormat::all(), rome())
            .unwrap_or_else(|| panic!("expected European date time to parse"));
        assert_eq!(parsed.format, DateFormat::EuropeanTime);
    }

    #[test]
    fn parse_date_european_dash() {
        let parsed = parse_date("15-03-2024", DateFormat::all(), rome())
            .unwrap_or_else(|| panic!("expected European dash date to parse"));
        assert_eq!(parsed.format, DateFormat::EuropeanDash);
    }

    #[test]
    fn parse_date_invalid() {
        assert!(parse_date("", DateFormat::all(), rome()).is_none());
        assert!(parse_date("not a date", DateFormat::all(), rome()).is_none());
        assert!(parse_date("2024-13-45", DateFormat::all(), rome()).is_none()); // Invalid month/day
        assert!(parse_date("32/01/2024", DateFormat::all(), rome()).is_none()); // Invalid day
    }

    #[test]
    fn validate_date_valid() {
        assert!(validate_date("2024-03-15", DateFormat::all(), rome(), Locale::It).is_valid());
        assert!(validate_date("15/03/2024", DateFormat::all(), rome(), Locale::It).is_valid());
        assert!(
            validate_date("2024-03-15 14:30", DateFormat::all(), rome(), Locale::It).is_valid()
        );
    }

    #[test]
    fn validate_date_invalid() {
        assert!(validate_date("invalid", DateFormat::all(), rome(), Locale::It).is_invalid());
        assert!(validate_date("2024-13-45", DateFormat::all(), rome(), Locale::It).is_invalid());
    }

    #[test]
    fn validate_date_empty_is_valid() {
        assert!(validate_date("", DateFormat::all(), rome(), Locale::It).is_valid());
        assert!(validate_date("   ", DateFormat::all(), rome(), Locale::It).is_valid());
    }

}
