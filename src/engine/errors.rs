//! The module contains the error the engine can throw.
//!
//! The errors are:
//!
//! - [`MaxBalanceReached`] thrown when a [`CashFlow`] or [`Wallet`] has reached
//!     max balance.
//! - [`KeyNotFound`] thrown when an item are not found.
//!
//!  [`MaxBalanceReached`]: EngineError::MaxBalanceReached
//!  [`KeyNotFound`]: EngineError::KeyNotFound
//!  [`CashFlow`]: super::cash_flows::CashFlow
use thiserror::Error;

/// Engine custom errors.
#[derive(Error, Debug, PartialEq)]
pub enum EngineError {
    #[error("Max balance reached!")]
    MaxBalanceReached(String),
    #[error("\"{0}\" key not found!")]
    KeyNotFound(String),
}
