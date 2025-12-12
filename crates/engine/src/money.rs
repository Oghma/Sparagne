use std::{
    fmt,
    ops::{Add, AddAssign, Neg, Sub, SubAssign},
    str::FromStr,
};

use crate::EngineError;

/// Signed money amount represented as **integer cents**.
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
/// use engine::MoneyCents;
///
/// let amount = MoneyCents::new(12_34);
/// assert_eq!(amount.cents(), 1234);
/// assert_eq!(amount.to_string(), "12.34€");
/// ```
///
/// Parsing from user input (accepts `.` or `,` as decimal separator; rejects >
/// 2 decimals):
///
/// ```rust
/// use engine::MoneyCents;
///
/// assert_eq!("10".parse::<MoneyCents>().unwrap().cents(), 1000);
/// assert_eq!("10,5".parse::<MoneyCents>().unwrap().cents(), 1050);
/// assert!("12.345".parse::<MoneyCents>().is_err());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct MoneyCents(i64);

impl MoneyCents {
    pub const ZERO: MoneyCents = MoneyCents(0);

    /// Creates a new amount from integer cents.
    #[must_use]
    pub const fn new(cents: i64) -> Self {
        Self(cents)
    }

    /// Returns the raw value in cents.
    #[must_use]
    pub const fn cents(self) -> i64 {
        self.0
    }

    /// Returns `true` if the amount is 0.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` if the amount is positive.
    #[must_use]
    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    /// Returns `true` if the amount is negative.
    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// Checked addition (returns `None` on overflow).
    #[must_use]
    pub fn checked_add(self, rhs: MoneyCents) -> Option<MoneyCents> {
        self.0.checked_add(rhs.0).map(MoneyCents)
    }

    /// Checked subtraction (returns `None` on overflow).
    #[must_use]
    pub fn checked_sub(self, rhs: MoneyCents) -> Option<MoneyCents> {
        self.0.checked_sub(rhs.0).map(MoneyCents)
    }
}

impl fmt::Display for MoneyCents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        let euros = abs / 100;
        let cents = abs % 100;
        write!(f, "{sign}{euros}.{cents:02}€")
    }
}

impl From<i64> for MoneyCents {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl From<MoneyCents> for i64 {
    fn from(value: MoneyCents) -> Self {
        value.0
    }
}

impl Add for MoneyCents {
    type Output = MoneyCents;

    fn add(self, rhs: MoneyCents) -> Self::Output {
        MoneyCents(self.0 + rhs.0)
    }
}

impl AddAssign for MoneyCents {
    fn add_assign(&mut self, rhs: MoneyCents) {
        self.0 += rhs.0;
    }
}

impl Sub for MoneyCents {
    type Output = MoneyCents;

    fn sub(self, rhs: MoneyCents) -> Self::Output {
        MoneyCents(self.0 - rhs.0)
    }
}

impl SubAssign for MoneyCents {
    fn sub_assign(&mut self, rhs: MoneyCents) {
        self.0 -= rhs.0;
    }
}

impl Neg for MoneyCents {
    type Output = MoneyCents;

    fn neg(self) -> Self::Output {
        MoneyCents(-self.0)
    }
}

impl FromStr for MoneyCents {
    type Err = EngineError;

    /// Parses a decimal string into cents.
    ///
    /// Accepts `.` or `,` as decimal separator and an optional leading `+`/`-`.
    ///
    /// Validation rules:
    /// - max 2 fractional digits (rejects `12.345`)
    /// - rejects empty/invalid strings
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let empty = || EngineError::InvalidAmount("empty amount".to_string());
        let invalid = || EngineError::InvalidAmount("invalid amount".to_string());
        let overflow = || EngineError::InvalidAmount("amount too large".to_string());

        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(empty());
        }

        let (sign, rest) = if let Some(stripped) = trimmed.strip_prefix('-') {
            (-1i64, stripped)
        } else if let Some(stripped) = trimmed.strip_prefix('+') {
            (1i64, stripped)
        } else {
            (1i64, trimmed)
        };

        let rest = rest.trim();
        if rest.is_empty() {
            return Err(empty());
        }

        let rest = rest.replace(',', ".");
        let mut parts = rest.split('.');
        let euros_str = parts
            .next()
            .ok_or_else(invalid)?;
        let cents_str = parts.next();

        if parts.next().is_some() {
            return Err(invalid());
        }

        if euros_str.is_empty() || !euros_str.chars().all(|c| c.is_ascii_digit()) {
            return Err(invalid());
        }

        let euros: i64 = euros_str
            .parse()
            .map_err(|_| invalid())?;

        let cents: i64 = match cents_str {
            None => 0,
            Some("") => 0,
            Some(frac) => {
                if !frac.chars().all(|c| c.is_ascii_digit()) {
                    return Err(invalid());
                }
                match frac.len() {
                    0 => 0,
                    1 => {
                        frac.parse::<i64>()
                            .map_err(|_| invalid())?
                            * 10
                    }
                    2 => frac
                        .parse::<i64>()
                        .map_err(|_| invalid())?,
                    _ => return Err(EngineError::InvalidAmount("too many decimals".to_string())),
                }
            }
        };

        let total = euros
            .checked_mul(100)
            .and_then(|v| v.checked_add(cents))
            .ok_or_else(overflow)?;

        let signed = if sign < 0 {
            total.checked_neg().ok_or_else(overflow)?
        } else {
            total
        };

        Ok(MoneyCents(signed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_formats_eur() {
        assert_eq!(MoneyCents::new(0).to_string(), "0.00€");
        assert_eq!(MoneyCents::new(1).to_string(), "0.01€");
        assert_eq!(MoneyCents::new(10).to_string(), "0.10€");
        assert_eq!(MoneyCents::new(1050).to_string(), "10.50€");
        assert_eq!(MoneyCents::new(-1050).to_string(), "-10.50€");
    }

    #[test]
    fn parse_accepts_dot_or_comma() {
        assert_eq!("10".parse::<MoneyCents>().unwrap().cents(), 1000);
        assert_eq!("10.5".parse::<MoneyCents>().unwrap().cents(), 1050);
        assert_eq!("10,50".parse::<MoneyCents>().unwrap().cents(), 1050);
        assert_eq!("-0.01".parse::<MoneyCents>().unwrap().cents(), -1);
        assert_eq!("+1.00".parse::<MoneyCents>().unwrap().cents(), 100);
        assert_eq!("  2.30 ".parse::<MoneyCents>().unwrap().cents(), 230);
    }

    #[test]
    fn parse_rejects_more_than_two_decimals() {
        assert!("12.345".parse::<MoneyCents>().is_err());
        assert!("0.001".parse::<MoneyCents>().is_err());
    }
}
