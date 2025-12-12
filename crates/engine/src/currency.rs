use serde::{Deserialize, Serialize};

use crate::EngineError;

/// ISO-like currency code used by a vault and its money values.
///
/// Today Sparagne is effectively mono-currency (default `EUR`), but the engine models currency
/// explicitly to keep the data model future-proof.
///
/// ## Minor units
///
/// The engine stores monetary values as an `i64` number of **minor units** (see `Money`).
/// `minor_units()` returns how many decimal digits are used when converting between:
/// - major units (human input/output, e.g. `10.50 EUR`)
/// - minor units (stored integers, e.g. `1050`)
///
/// Example: EUR has 2 minor units, so `10.50 EUR` ⇄ `1050`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    #[default]
    Eur,
}

impl Currency {
    /// Canonical currency code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Currency::Eur => "EUR",
        }
    }

    /// Number of fraction digits used when formatting/parsing amounts.
    ///
    /// Example: EUR uses 2 fraction digits (cents).
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
    type Error = EngineError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.trim().to_ascii_uppercase().as_str() {
            "EUR" => Ok(Currency::Eur),
            other => Err(EngineError::CurrencyMismatch(format!(
                "unsupported currency: {other}"
            ))),
        }
    }
}
