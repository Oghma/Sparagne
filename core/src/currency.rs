//! Currency codes and minor units.

use serde::{Deserialize, Serialize};

use crate::DomainError;

/// ISO-like currency code. One currency per vault.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    #[default]
    Eur,
}

impl Currency {
    /// Canonical code, e.g. `EUR`.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Currency::Eur => "EUR",
        }
    }

    /// Fraction digits used when converting major <-> minor units.
    #[must_use]
    pub const fn minor_units(self) -> u8 {
        match self {
            Currency::Eur => 2,
        }
    }
}

impl core::fmt::Display for Currency {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.code())
    }
}

impl TryFrom<&str> for Currency {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.trim().to_ascii_uppercase().as_str() {
            "EUR" => Ok(Currency::Eur),
            other => Err(DomainError::CurrencyMismatch(format!(
                "unsupported currency: {other}"
            ))),
        }
    }
}
