//! Engine: recurring template commands and the pending-periods query.

use chrono::{DateTime, FixedOffset, NaiveDate};
use rusqlite::Transaction;
use uuid::Uuid;

use crate::{CommandEnvelope, DomainError, Result, TransactionKind, recurring::Schedule};

/// Borrowed view of `Command::CreateRecurring`.
pub(super) struct RecurringSpec<'a> {
    pub kind: TransactionKind,
    pub amount: i64,
    pub wallet_id: Option<Uuid>,
    pub flow_id: Option<Uuid>,
    pub category: Option<&'a str>,
    pub note: Option<&'a str>,
    pub schedule: Schedule,
}

/// Borrowed view of `Command::UpdateRecurring`.
pub(super) struct RecurringPatch<'a> {
    pub amount: Option<i64>,
    pub wallet_id: Option<Uuid>,
    pub flow_id: Option<Uuid>,
    pub category: Option<&'a str>,
    pub note: Option<&'a str>,
    pub schedule: Option<Schedule>,
    pub enabled: Option<bool>,
}

fn unimplemented(what: &str) -> DomainError {
    DomainError::InvalidCommand(format!("{what}: not implemented"))
}

/// Returns the template id (the command id).
pub(super) fn create_recurring(
    _tx: &Transaction<'_>,
    _env: &CommandEnvelope,
    _spec: &RecurringSpec<'_>,
    _now: i64,
) -> Result<Uuid> {
    Err(unimplemented("create_recurring"))
}

pub(super) fn update_recurring(
    _tx: &Transaction<'_>,
    _env: &CommandEnvelope,
    _recurring_id: Uuid,
    _patch: &RecurringPatch<'_>,
) -> Result<()> {
    Err(unimplemented("update_recurring"))
}

pub(super) fn archive_recurring(
    _tx: &Transaction<'_>,
    _env: &CommandEnvelope,
    _recurring_id: Uuid,
    _now: i64,
) -> Result<()> {
    Err(unimplemented("archive_recurring"))
}

/// Returns the id of the created transaction (the command id).
pub(super) fn execute_recurring(
    _tx: &Transaction<'_>,
    _env: &CommandEnvelope,
    _recurring_id: Uuid,
    _period_date: NaiveDate,
    _occurred_at: DateTime<FixedOffset>,
    _now: i64,
) -> Result<Uuid> {
    Err(unimplemented("execute_recurring"))
}

pub(super) fn skip_recurring(
    _tx: &Transaction<'_>,
    _env: &CommandEnvelope,
    _recurring_id: Uuid,
    _period_date: NaiveDate,
    _now: i64,
) -> Result<()> {
    Err(unimplemented("skip_recurring"))
}
