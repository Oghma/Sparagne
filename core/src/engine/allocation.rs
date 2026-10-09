//! Engine: allocation plan commands and the allocation queries.
//!
//! A plan is decided one period at a time, like a recurring template: a
//! period is pending while it has no row in `allocation_runs`. Unlike a
//! template, periods are decided in order, so a period older than the latest
//! decided one is never due again.
//!
//! A stub for now.

use chrono::{DateTime, FixedOffset, NaiveDate};
use rusqlite::Transaction;
use uuid::Uuid;

use crate::{
    CommandEnvelope, Core, DomainError, Result,
    allocation::{
        AllocationBase, AllocationLine, AllocationMove, AllocationPlanPatch, AllocationPlanView,
        AllocationPreview, AllocationRunView, PendingAllocation, resolve,
    },
    recurring::Schedule,
};

/// Borrowed view of `Command::ExecuteAllocation`.
pub(super) struct ExecuteSpec<'a> {
    pub plan_id: Uuid,
    pub period_date: NaiveDate,
    pub occurred_at: DateTime<FixedOffset>,
    pub total: i64,
    pub moves: &'a [AllocationMove],
    pub note: Option<&'a str>,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Returns the plan id (the command id).
pub(super) fn create_plan(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    schedule: &Schedule,
    lines: &[AllocationLine],
    now: i64,
) -> Result<Uuid> {
    let _ = (tx, env, schedule, lines, now);
    Err(not_implemented())
}

pub(super) fn update_plan(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    plan_id: Uuid,
    patch: &AllocationPlanPatch,
) -> Result<()> {
    let _ = (tx, env, plan_id, patch);
    Err(not_implemented())
}

pub(super) fn execute_allocation(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    spec: &ExecuteSpec<'_>,
    now: i64,
) -> Result<()> {
    let _ = (tx, env, spec.plan_id, spec.period_date, spec.occurred_at);
    let _ = (spec.total, spec.moves, spec.note, now);
    Err(not_implemented())
}

pub(super) fn skip_allocation(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    plan_id: Uuid,
    period_date: NaiveDate,
    now: i64,
) -> Result<()> {
    let _ = (tx, env, plan_id, period_date, now);
    Err(not_implemented())
}

pub(super) fn reopen_allocation(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    plan_id: Uuid,
    period_date: NaiveDate,
    now: i64,
) -> Result<()> {
    let _ = (tx, env, plan_id, period_date, now);
    Err(not_implemented())
}

fn not_implemented() -> DomainError {
    DomainError::InvalidCommand("not implemented".to_string())
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

impl Core {
    /// The vault's plan, if it has one.
    pub fn allocation_plan(&self, vault_id: Uuid) -> Result<Option<AllocationPlanView>> {
        let _ = vault_id;
        Ok(None)
    }

    /// The period of the plan waiting for a decision on `today`, if any: the
    /// most recent due date (up to `today` and the schedule's end) later
    /// than every decided period. `None` without a plan, for a disabled
    /// plan, or when nothing is due.
    pub fn pending_allocation(
        &self,
        vault_id: Uuid,
        today: NaiveDate,
    ) -> Result<Option<PendingAllocation>> {
        let _ = (vault_id, today);
        Ok(None)
    }

    /// The incomes the next execution shares out: live incomes with a
    /// positive leg on Unallocated, not an opening balance, dated on or after
    /// the plan's start, and recorded after the command of the plan's latest
    /// decided period (executed or skipped). Empty without a plan.
    pub fn allocation_base(&self, vault_id: Uuid) -> Result<AllocationBase> {
        let _ = vault_id;
        Ok(AllocationBase {
            total: 0,
            incomes: Vec::new(),
        })
    }

    /// `lines` worked out on `total` against the vault's envelopes as they
    /// are now. The lines need not be saved: the plan editor previews its
    /// draft with them.
    pub fn preview_allocation(
        &self,
        vault_id: Uuid,
        lines: &[AllocationLine],
        total: i64,
    ) -> Result<AllocationPreview> {
        let _ = vault_id;
        Ok(resolve(total, lines, &[], 0))
    }

    /// The plan's decided periods, the most recent first, at most `limit`.
    pub fn allocation_runs(&self, vault_id: Uuid, limit: u32) -> Result<Vec<AllocationRunView>> {
        let _ = (vault_id, limit);
        Ok(Vec::new())
    }
}
