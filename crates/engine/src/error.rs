//! The module contains the error the engine can throw.
//!
//! The errors are:
//!
//! - [`MaxBalanceReached`] thrown when a [`CashFlow`] has reached max balance.
//! - [`KeyNotFound`] thrown when an item are not found.
//!
//!  [`MaxBalanceReached`]: EngineError::MaxBalanceReached
//!  [`KeyNotFound`]: EngineError::KeyNotFound
//!  [`CashFlow`]: super::cash_flows::CashFlow
use sea_orm::DbErr;
use thiserror::Error;

/// Engine custom errors.
#[derive(Error, Debug)]
pub enum EngineError {
    #[error("Max balance reached!")]
    MaxBalanceReached(String),
    #[error("Insufficient funds: {0}")]
    InsufficientFunds(String),
    #[error("\"{0}\" key not found!")]
    KeyNotFound(String),
    #[error("\"{0}\" already present!")]
    ExistingKey(String),
    #[error("Invalid amount: {0}")]
    InvalidAmount(String),
    /// Name parsing/validation failed.
    #[error("Invalid name: {0}")]
    InvalidName(String),
    /// ID parsing/validation failed.
    #[error("Invalid id: {0}")]
    InvalidId(String),
    /// Cursor parsing/validation failed.
    #[error("Invalid cursor: {0}")]
    InvalidCursor(String),
    #[error("Invalid flow: {0}")]
    InvalidFlow(String),
    /// Role parsing/validation failed.
    #[error("Invalid role: {0}")]
    InvalidRole(String),
    #[error("Currency mismatch: {0}")]
    CurrencyMismatch(String),
    #[error("Invalid recurring template: {0}")]
    InvalidRecurring(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error(transparent)]
    Database(#[from] DbErr),
}

impl PartialEq for EngineError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::MaxBalanceReached(a), Self::MaxBalanceReached(b)) => a == b,
            (Self::InsufficientFunds(a), Self::InsufficientFunds(b)) => a == b,
            (Self::KeyNotFound(a), Self::KeyNotFound(b)) => a == b,
            (Self::ExistingKey(a), Self::ExistingKey(b)) => a == b,
            (Self::InvalidAmount(a), Self::InvalidAmount(b)) => a == b,
            (Self::InvalidName(a), Self::InvalidName(b)) => a == b,
            (Self::InvalidId(a), Self::InvalidId(b)) => a == b,
            (Self::InvalidCursor(a), Self::InvalidCursor(b)) => a == b,
            (Self::InvalidFlow(a), Self::InvalidFlow(b)) => a == b,
            (Self::InvalidRole(a), Self::InvalidRole(b)) => a == b,
            (Self::CurrencyMismatch(a), Self::CurrencyMismatch(b)) => a == b,
            (Self::InvalidRecurring(a), Self::InvalidRecurring(b)) => a == b,
            (Self::Forbidden(a), Self::Forbidden(b)) => a == b,
            (Self::Database(a), Self::Database(b)) => a.to_string() == b.to_string(),
            _ => false,
        }
    }
}

impl EngineError {
    /// Common error message constants to avoid duplication across the codebase.
    pub(crate) const VAULT_NOT_FOUND: &'static str = "vault not exists";
    pub(crate) const FLOW_NOT_FOUND: &'static str = "flow not exists";
    pub(crate) const WALLET_NOT_FOUND: &'static str = "wallet not exists";
    pub(crate) const CATEGORY_NOT_FOUND: &'static str = "category not exists";
    pub(crate) const USER_NOT_FOUND: &'static str = "user not exists";
    pub(crate) const RECURRING_NOT_FOUND: &'static str = "recurring template not exists";
}
