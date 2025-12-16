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
    #[error("Invalid flow: {0}")]
    InvalidFlow(String),
    #[error("Currency mismatch: {0}")]
    CurrencyMismatch(String),
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
            (Self::InvalidFlow(a), Self::InvalidFlow(b)) => a == b,
            (Self::CurrencyMismatch(a), Self::CurrencyMismatch(b)) => a == b,
            (Self::Database(a), Self::Database(b)) => a.to_string() == b.to_string(),
            _ => false,
        }
    }
}
