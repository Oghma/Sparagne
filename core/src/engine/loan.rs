//! Loan commands and queries. The tables keep what the commands said (the
//! terms as columns, each event as JSON); every query works the schedule out
//! again with [`crate::loan::math`]. Validation only looks at the command
//! and the loan's own row, never at the schedule: whatever depends on it is
//! a warning of the schedule, so a rebase that reorders events never turns
//! an accepted command into a refused one.

use chrono::NaiveDate;
use rusqlite::Transaction;
use uuid::Uuid;

use crate::{
    CommandEnvelope, Core, DomainError, Result,
    loan::{
        LinkedInstallment, LoanEvent, LoanEventView, LoanOverview, LoanPatch, LoanSchedule,
        LoanTerms, LoanView, math,
    },
};

fn not_implemented() -> DomainError {
    DomainError::InvalidCommand("not implemented".to_string())
}

pub(super) fn create_loan(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    name: &str,
    note: Option<&str>,
    terms: &LoanTerms,
    recurring_id: Option<Uuid>,
    now: i64,
) -> Result<Uuid> {
    let _ = (tx, env, name, note, terms, recurring_id, now);
    Err(not_implemented())
}

pub(super) fn update_loan(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    loan_id: Uuid,
    patch: &LoanPatch,
) -> Result<()> {
    let _ = (tx, env, loan_id, patch);
    Err(not_implemented())
}

pub(super) fn set_loan_recurring(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    loan_id: Uuid,
    recurring_id: Option<Uuid>,
) -> Result<()> {
    let _ = (tx, env, loan_id, recurring_id);
    Err(not_implemented())
}

pub(super) fn archive_loan(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    loan_id: Uuid,
    now: i64,
) -> Result<()> {
    let _ = (tx, env, loan_id, now);
    Err(not_implemented())
}

pub(super) fn restore_loan(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    loan_id: Uuid,
) -> Result<()> {
    let _ = (tx, env, loan_id);
    Err(not_implemented())
}

/// Returns the event's id, the command id.
pub(super) fn add_loan_event(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    loan_id: Uuid,
    event: &LoanEvent,
    now: i64,
) -> Result<Uuid> {
    let _ = (tx, env, loan_id, event, now);
    Err(not_implemented())
}

pub(super) fn remove_loan_event(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    loan_id: Uuid,
    event_id: Uuid,
) -> Result<()> {
    let _ = (tx, env, loan_id, event_id);
    Err(not_implemented())
}

impl Core {
    /// Every loan of the vault, archived included, by name.
    pub fn loans(&self, vault_id: Uuid) -> Result<Vec<LoanView>> {
        let _ = vault_id;
        Ok(Vec::new())
    }

    /// One loan's schedule, its events applied.
    pub fn loan_schedule(&self, vault_id: Uuid, loan_id: Uuid) -> Result<LoanSchedule> {
        let _ = (vault_id, loan_id);
        Err(not_implemented())
    }

    /// Every active loan on `today`, by name.
    pub fn loan_overview(&self, vault_id: Uuid, today: NaiveDate) -> Result<Vec<LoanOverview>> {
        let _ = (vault_id, today);
        Ok(Vec::new())
    }

    /// For every active loan with a linked template, the amount each due
    /// date's installment should be recorded with.
    pub fn linked_installments(&self, vault_id: Uuid) -> Result<Vec<LinkedInstallment>> {
        let _ = vault_id;
        Ok(Vec::new())
    }

    /// The schedule of terms and events not saved yet, for the forms.
    pub fn preview_loan(
        &self,
        terms: &LoanTerms,
        events: &[LoanEventView],
    ) -> Result<LoanSchedule> {
        math::schedule(terms, events)
    }
}
