//! Internal helpers for model validation and conversion.
//!
//! These utilities are **not** part of the public API. They centralize
//! validation and mapping logic so the engine enforces consistent invariants.

use chrono::{DateTime, Utc};

use crate::{Currency, EngineError, ResultEngine};

/// Trims and validates a required name field.
pub(crate) fn normalize_required_name(value: &str, label: &str) -> ResultEngine<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EngineError::InvalidName(format!(
            "{label} name must not be empty"
        )));
    }
    Ok(trimmed.to_string())
}

/// Trims optional text, returning `None` if empty.
pub(crate) fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

/// Applies an optional patch to an existing optional text field.
pub(crate) fn apply_optional_text_patch(
    existing: Option<String>,
    patch: Option<&str>,
) -> Option<String> {
    match patch {
        None => existing,
        Some(value) => normalize_optional_text(Some(value)),
    }
}

/// Applies an optional patch to an existing datetime field.
pub(crate) fn apply_optional_datetime_patch(
    existing: DateTime<Utc>,
    patch: Option<DateTime<Utc>>,
) -> DateTime<Utc> {
    patch.unwrap_or(existing)
}

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
