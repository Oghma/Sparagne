//! Domain errors with stable codes.

use thiserror::Error;

/// Error returned by every core operation.
///
/// `code()` is stable and meant to reach the UI unchanged; the message is
/// for logs and developers.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("insufficient funds in flow '{0}'")]
    InsufficientFunds(String),
    #[error("cap reached on flow '{0}'")]
    MaxBalanceReached(String),
    #[error("{0} not found")]
    NotFound(String),
    #[error("'{0}' already exists")]
    AlreadyExists(String),
    #[error("invalid amount: {0}")]
    InvalidAmount(String),
    #[error("invalid name: {0}")]
    InvalidName(String),
    #[error("invalid flow: {0}")]
    InvalidFlow(String),
    #[error("currency mismatch: {0}")]
    CurrencyMismatch(String),
    #[error("invalid command: {0}")]
    InvalidCommand(String),
    #[error("invalid cursor: {0}")]
    InvalidCursor(String),
    #[error("storage error: {0}")]
    Storage(String),
}

impl DomainError {
    /// Stable snake_case code for clients.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InsufficientFunds(_) => "insufficient_funds",
            Self::MaxBalanceReached(_) => "max_balance_reached",
            Self::NotFound(_) => "not_found",
            Self::AlreadyExists(_) => "already_exists",
            Self::InvalidAmount(_) => "invalid_amount",
            Self::InvalidName(_) => "invalid_name",
            Self::InvalidFlow(_) => "invalid_flow",
            Self::CurrencyMismatch(_) => "currency_mismatch",
            Self::InvalidCommand(_) => "invalid_command",
            Self::InvalidCursor(_) => "invalid_cursor",
            Self::Storage(_) => "storage_error",
        }
    }
}

impl From<rusqlite::Error> for DomainError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Storage(err.to_string())
    }
}

impl From<serde_json::Error> for DomainError {
    fn from(err: serde_json::Error) -> Self {
        Self::Storage(format!("payload: {err}"))
    }
}
