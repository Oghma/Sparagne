//! Category name normalization.
//!
//! `display` is what the user sees (trimmed, single spaces); `key` is the
//! lookup key (NFKD, no diacritics, lowercase alphanumerics, punctuation as a
//! single space).

use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

use crate::{DomainError, Result};

/// Normalized key of the system category that collects unassigned entries.
pub const UNCATEGORIZED_KEY: &str = "uncategorized";
/// Normalized key of the system category used for opening balances.
pub const OPENING_KEY: &str = "opening";

/// Trim and collapse whitespace. Empty is an error.
pub fn normalize_category_display(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DomainError::InvalidName(
            "category name must not be empty".to_string(),
        ));
    }
    Ok(trimmed.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Lookup key: `"Caffè"` -> `"caffe"`, `"Auto-Moto"` -> `"auto moto"`.
pub fn normalize_category_key(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DomainError::InvalidName(
            "category name must not be empty".to_string(),
        ));
    }
    let mut out = String::new();
    let mut prev_space = false;
    for ch in trimmed.nfkd() {
        if is_combining_mark(ch) {
            continue;
        }
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_space = false;
        } else if !out.is_empty() && !prev_space {
            out.push(' ');
            prev_space = true;
        }
    }
    let normalized = out.trim();
    if normalized.is_empty() {
        return Err(DomainError::InvalidName(
            "category name must not be empty".to_string(),
        ));
    }
    Ok(normalized.to_string())
}

/// `(display, key)` for a user-provided category name; system keys are
/// reserved.
pub fn validate_category_name(name: &str) -> Result<(String, String)> {
    let display = normalize_category_display(name)?;
    let key = normalize_category_key(&display)?;
    if key == UNCATEGORIZED_KEY || key == OPENING_KEY {
        return Err(DomainError::InvalidName(
            "category name is reserved".to_string(),
        ));
    }
    Ok((display, key))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn key_strips_diacritics_and_punctuation() {
        assert_eq!(normalize_category_key("  Caffè ").unwrap(), "caffe");
        assert_eq!(normalize_category_key("Auto-Moto").unwrap(), "auto moto");
        assert_eq!(normalize_category_key("spesa!!!").unwrap(), "spesa");
        assert!(normalize_category_key("!!!").is_err());
    }

    #[test]
    fn display_collapses_spaces() {
        assert_eq!(normalize_category_display("  a   b ").unwrap(), "a b");
    }

    #[test]
    fn reserved_names_are_rejected() {
        assert!(validate_category_name("Uncategorized").is_err());
        assert!(validate_category_name("opening").is_err());
        assert!(validate_category_name("Spesa").is_ok());
    }
}
