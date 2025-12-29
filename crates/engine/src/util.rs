//! Internal helpers for model validation and conversion.
//!
//! These utilities are **not** part of the public API. They centralize
//! validation and mapping logic so the engine enforces consistent invariants.

use uuid::Uuid;

use crate::{Currency, EngineError, ResultEngine};

/// Validate flow cap/income fields for consistency and invariant safety.
pub(crate) fn validate_flow_mode_fields(
    flow_name: &str,
    max_balance: Option<i64>,
    income_balance: Option<i64>,
) -> ResultEngine<()> {
    if let Some(cap_minor) = max_balance
        && cap_minor <= 0
    {
        return Err(EngineError::InvalidFlow(format!(
            "invalid cap for flow '{flow_name}': cap must be > 0"
        )));
    }
    if income_balance.is_some() && max_balance.is_none() {
        return Err(EngineError::InvalidFlow(format!(
            "invalid FlowMode for flow '{flow_name}': income_balance requires max_balance"
        )));
    }
    if let Some(income_total_minor) = income_balance
        && income_total_minor < 0
    {
        return Err(EngineError::InvalidFlow(format!(
            "invalid FlowMode for flow '{flow_name}': income_balance must be >= 0"
        )));
    }
    if let (Some(cap_minor), Some(income_total_minor)) = (max_balance, income_balance)
        && income_total_minor > cap_minor
    {
        return Err(EngineError::InvalidFlow(format!(
            "invalid FlowMode for flow '{flow_name}': income_balance exceeds cap"
        )));
    }
    Ok(())
}

/// Parse a UUID from storage and return a labeled error on failure.
pub(crate) fn parse_uuid(value: &str, label: &str) -> ResultEngine<Uuid> {
    Uuid::parse_str(value).map_err(|_| EngineError::InvalidId(format!("invalid {label} id")))
}

/// Parse a currency code stored in the DB into a strongly typed `Currency`.
pub(crate) fn model_currency(value: &str) -> ResultEngine<Currency> {
    Currency::try_from(value)
        .map_err(|_| EngineError::InvalidAmount(format!("invalid currency: {value}")))
}

/// Ensure a stored currency matches the vault currency.
pub(crate) fn ensure_vault_currency(
    vault_currency: Currency,
    actual: Currency,
) -> ResultEngine<()> {
    if vault_currency != actual {
        return Err(EngineError::CurrencyMismatch(format!(
            "vault currency is {}, got {}",
            vault_currency.code(),
            actual.code()
        )));
    }
    Ok(())
}
