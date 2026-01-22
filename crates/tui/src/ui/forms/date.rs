//! Date form field with multi-format validation.

use ratatui::text::Line;

use crate::{
    ui::Theme,
    validation::{DateFormat, DateValidator, FieldState, ValidationResult, Validator},
};

use super::{FormField, FormFieldRenderer};

/// A form field for entering dates with flexible format support.
///
/// Supports multiple date formats including ISO and European styles.
#[derive(Debug, Clone)]
pub struct DateField {
    /// The label to display for this field.
    pub label: String,
    /// The current field state including value and validation.
    pub state: FieldState,
    /// Formats to accept for parsing.
    pub formats: Vec<DateFormat>,
    /// Timezone for parsing dates.
    pub timezone: chrono_tz::Tz,
}

impl DateField {
    /// Creates a new date field with the given label.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            state: FieldState {
                required: true,
                ..Default::default()
            },
            formats: DateFormat::all().to_vec(),
            timezone: chrono_tz::Europe::Rome,
        }
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

    /// Sets the timezone for parsing.
    #[must_use]
    pub fn with_timezone(mut self, tz: chrono_tz::Tz) -> Self {
        self.timezone = tz;
        self
    }

    /// Validates the current value and updates the state.
    pub fn validate(&mut self) {
        // Check required first
        if self.state.required && self.state.value.trim().is_empty() {
            self.state.validation = ValidationResult::Invalid("Data obbligatoria".to_string());
            return;
        }

        let validator = DateValidator {
            formats: self.formats.clone(),
            timezone: self.timezone,
        };

        self.state.validation = validator.validate(&self.state.value);
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

    /// Returns a hint about accepted formats.
    #[must_use]
    pub fn format_hint(&self) -> &'static str {
        "YYYY-MM-DD o DD/MM/YYYY"
    }
}

impl FormField for DateField {
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
    fn new_date_field() {
        let field = DateField::new("Date");
        assert_eq!(field.label, "Date");
        assert!(field.state.required);
    }

    #[test]
    fn validate_empty_required() {
        let mut field = DateField::new("Date");
        field.validate();
        assert!(field.state.validation.is_invalid());
    }

    #[test]
    fn validate_iso_format() {
        let mut field = DateField::new("Date").with_value("2024-03-15");
        field.validate();
        assert!(field.state.validation.is_valid());
    }

    #[test]
    fn validate_european_format() {
        let mut field = DateField::new("Date").with_value("15/03/2024");
        field.validate();
        assert!(field.state.validation.is_valid());
    }

    #[test]
    fn validate_with_time() {
        let mut field = DateField::new("Date").with_value("2024-03-15 14:30");
        field.validate();
        assert!(field.state.validation.is_valid());
    }

    #[test]
    fn validate_invalid_format() {
        let mut field = DateField::new("Date").with_value("not a date");
        field.validate();
        assert!(field.state.validation.is_invalid());
    }

    #[test]
    fn validate_optional_empty() {
        let mut field = DateField::new("Date").required(false);
        field.validate();
        assert!(field.state.validation.is_valid());
    }
}
