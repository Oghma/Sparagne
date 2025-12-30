use crate::{Currency, EngineError};

/// Signed money amount represented as **integer minor units**
/// (currency-dependent).
///
/// Use this type for **all** monetary values in the engine (balances, caps,
/// entry amounts) to avoid floating-point drift.
///
/// The value is signed:
/// - positive = income / increase
/// - negative = expense / decrease
///
/// # Examples
///
/// ```rust
/// use engine::{Currency, Money};
///
/// let amount = Money::new(12_34);
/// assert_eq!(amount.minor(), 1234);
/// assert_eq!(amount.format(Currency::Eur), "12.34 EUR");
/// ```
///
/// Parsing from user input (accepts `.` or `,` as decimal separator; rejects >
/// 2 decimals):
///
/// ```rust
/// use engine::{Currency, Money};
///
/// assert_eq!(
///     Money::parse_major("10", Currency::Eur).unwrap().minor(),
///     1000
/// );
/// assert_eq!(
///     Money::parse_major("10,5", Currency::Eur).unwrap().minor(),
///     1050
/// );
/// assert!(Money::parse_major("12.345", Currency::Eur).is_err());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Money(i64);

impl Money {
    /// Creates a new amount from integer minor units.
    #[must_use]
    pub const fn new(cents: i64) -> Self {
        Self(cents)
    }

    /// Returns the raw value in minor units.
    #[must_use]
    pub const fn minor(self) -> i64 {
        self.0
    }

    /// Formats the amount according to `currency.minor_units()`.
    ///
    /// Output format: `<sign><major>.<minor> <CODE>`, e.g. `-12.34 EUR`.
    #[must_use]
    pub fn format(self, currency: Currency) -> String {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();

        let scale = 10u64.pow(currency.minor_units() as u32);
        if scale == 1 {
            return format!("{sign}{abs} {}", currency.code());
        }

        let major = abs / scale;
        let minor = abs % scale;
        format!(
            "{sign}{major}.{minor:0width$} {}",
            currency.code(),
            width = currency.minor_units() as usize
        )
    }

    /// Parses a major-unit decimal string into minor units according to
    /// `currency.minor_units()`.
    ///
    /// Accepts `.` or `,` as decimal separator and an optional leading `+`/`-`.
    /// Rejects more fractional digits than allowed by the currency.
    pub fn parse_major(input: &str, currency: Currency) -> Result<Money, EngineError> {
        let empty = || EngineError::InvalidAmount("empty amount".to_string());
        let invalid = || EngineError::InvalidAmount("invalid amount".to_string());
        let overflow = || EngineError::InvalidAmount("amount too large".to_string());

        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(empty());
        }

        let (is_negative, rest) = if let Some(stripped) = trimmed.strip_prefix('-') {
            (true, stripped)
        } else if let Some(stripped) = trimmed.strip_prefix('+') {
            (false, stripped)
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

        let allowed = currency.minor_units() as usize;
        let frac_raw = match frac_str {
            None | Some("") => "",
            Some(frac) => frac,
        };

        if !frac_raw.chars().all(|c| c.is_ascii_digit()) {
            return Err(invalid());
        }
        if frac_raw.len() > allowed {
            return Err(EngineError::InvalidAmount("too many decimals".to_string()));
        }

        let mut frac = frac_raw.to_string();
        while frac.len() < allowed {
            frac.push('0');
        }

        let scale: i64 = 10i64.pow(currency.minor_units() as u32);
        let frac_val: i64 = if frac.is_empty() {
            0
        } else {
            frac.parse().map_err(|_| invalid())?
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
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn format_uses_currency_minor_units() {
        assert_eq!(Money::new(0).format(Currency::Eur), "0.00 EUR");
        assert_eq!(Money::new(1).format(Currency::Eur), "0.01 EUR");
        assert_eq!(Money::new(10).format(Currency::Eur), "0.10 EUR");
        assert_eq!(Money::new(1050).format(Currency::Eur), "10.50 EUR");
        assert_eq!(Money::new(-1050).format(Currency::Eur), "-10.50 EUR");
    }

    #[test]
    fn parse_accepts_dot_or_comma() {
        assert_eq!(
            Money::parse_major("10", Currency::Eur).unwrap().minor(),
            1000
        );
        assert_eq!(
            Money::parse_major("10.5", Currency::Eur).unwrap().minor(),
            1050
        );
        assert_eq!(
            Money::parse_major("10,50", Currency::Eur).unwrap().minor(),
            1050
        );
        assert_eq!(
            Money::parse_major("-0.01", Currency::Eur).unwrap().minor(),
            -1
        );
        assert_eq!(
            Money::parse_major("+1.00", Currency::Eur).unwrap().minor(),
            100
        );
        assert_eq!(
            Money::parse_major("  2.30 ", Currency::Eur)
                .unwrap()
                .minor(),
            230
        );
    }

    #[test]
    fn parse_rejects_more_than_two_decimals() {
        assert!(Money::parse_major("12.345", Currency::Eur).is_err());
        assert!(Money::parse_major("0.001", Currency::Eur).is_err());
    }
}
