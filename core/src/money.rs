//! Money parsing and formatting (integer minor units).

use crate::{Currency, DomainError};

/// Signed amount in integer minor units. Positive = increase.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Money(i64);

impl Money {
    #[must_use]
    pub const fn new(minor: i64) -> Self {
        Self(minor)
    }

    #[must_use]
    pub const fn minor(self) -> i64 {
        self.0
    }

    /// `<sign><major>.<minor> <CODE>`, e.g. `-12.34 EUR`. No grouping, no
    /// locale: clients format for display with the system formatter.
    #[must_use]
    pub fn format(self, currency: Currency) -> String {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        let scale = 10u64.pow(u32::from(currency.minor_units()));
        if scale == 1 {
            return format!("{sign}{abs} {}", currency.code());
        }
        let major = abs / scale;
        let minor = abs % scale;
        format!(
            "{sign}{major}.{minor:0width$} {}",
            currency.code(),
            width = usize::from(currency.minor_units())
        )
    }

    /// Parses `[+|-] digits [ (.|,) up to minor_units digits ]`.
    ///
    /// No thousands separators, no symbols, no leading separator. More
    /// fraction digits than the currency allows is an error, never rounded.
    pub fn parse_major(input: &str, currency: Currency) -> Result<Money, DomainError> {
        let empty = || DomainError::InvalidAmount("empty amount".to_string());
        let invalid = || DomainError::InvalidAmount("invalid amount".to_string());
        let overflow = || DomainError::InvalidAmount("amount too large".to_string());

        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(empty());
        }
        let (is_negative, rest) = if let Some(s) = trimmed.strip_prefix('-') {
            (true, s)
        } else if let Some(s) = trimmed.strip_prefix('+') {
            (false, s)
        } else {
            (false, trimmed)
        };
        let rest = rest.trim();
        if rest.is_empty() {
            return Err(empty());
        }
        let rest = rest.replace(',', ".");
        let mut parts = rest.split('.');
        let major_str = parts.next().ok_or_else(invalid)?;
        let frac_str = parts.next();
        if parts.next().is_some() {
            return Err(invalid());
        }
        if major_str.is_empty() || !major_str.chars().all(|c| c.is_ascii_digit()) {
            return Err(invalid());
        }
        let major: i64 = major_str.parse().map_err(|_| invalid())?;

        let allowed = usize::from(currency.minor_units());
        let frac_raw = frac_str.unwrap_or("");
        if !frac_raw.chars().all(|c| c.is_ascii_digit()) {
            return Err(invalid());
        }
        if frac_raw.len() > allowed {
            return Err(DomainError::InvalidAmount("too many decimals".to_string()));
        }
        let scale: i64 = 10i64.pow(u32::from(currency.minor_units()));
        let frac_val: i64 = if frac_raw.is_empty() {
            0
        } else {
            let padded = format!("{frac_raw:0<width$}", width = allowed);
            padded.parse().map_err(|_| invalid())?
        };
        let total = major
            .checked_mul(scale)
            .and_then(|v| v.checked_add(frac_val))
            .ok_or_else(overflow)?;
        let signed = if is_negative {
            total.checked_neg().ok_or_else(overflow)?
        } else {
            total
        };
        Ok(Money(signed))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn format_uses_currency_minor_units() {
        assert_eq!(Money::new(0).format(Currency::Eur), "0.00 EUR");
        assert_eq!(Money::new(1).format(Currency::Eur), "0.01 EUR");
        assert_eq!(Money::new(1050).format(Currency::Eur), "10.50 EUR");
        assert_eq!(Money::new(-1050).format(Currency::Eur), "-10.50 EUR");
    }

    #[test]
    fn parse_accepts_dot_or_comma_and_sign() {
        let p = |s| Money::parse_major(s, Currency::Eur).unwrap().minor();
        assert_eq!(p("10"), 1000);
        assert_eq!(p("10.5"), 1050);
        assert_eq!(p("10,50"), 1050);
        assert_eq!(p("-0.01"), -1);
        assert_eq!(p("+1.00"), 100);
        assert_eq!(p("  2.30 "), 230);
    }

    #[test]
    fn parse_rejects_bad_input() {
        for s in [
            "12.345", "0.001", ".5", "1.000,50", "1,000.50", "abc", "", "+-5",
        ] {
            assert!(Money::parse_major(s, Currency::Eur).is_err(), "{s}");
        }
    }
}
