//! Engine: partial update of an existing transaction.

use chrono::{DateTime, FixedOffset};
use rusqlite::Transaction;
use uuid::Uuid;

use crate::{CommandEnvelope, DomainError, Result};

/// Borrowed view of `Command::UpdateTransaction`.
pub(super) struct TransactionPatch<'a> {
    pub transaction_id: Uuid,
    pub amount: Option<i64>,
    pub occurred_at: Option<DateTime<FixedOffset>>,
    pub category: Option<&'a str>,
    pub note: Option<&'a str>,
    pub wallet_id: Option<Uuid>,
    pub flow_id: Option<Uuid>,
    pub from_id: Option<Uuid>,
    pub to_id: Option<Uuid>,
}

pub(super) fn update_transaction(
    _tx: &Transaction<'_>,
    _env: &CommandEnvelope,
    _patch: &TransactionPatch<'_>,
    _now: i64,
) -> Result<()> {
    Err(DomainError::InvalidCommand(
        "update_transaction: not implemented".to_string(),
    ))
}
