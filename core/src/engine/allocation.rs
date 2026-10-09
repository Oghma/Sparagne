//! Engine: allocation plan commands and the allocation queries.
//!
//! A plan is decided one period at a time, like a recurring template: a
//! period is pending while it has no row in `allocation_runs`. Unlike a
//! template, periods are decided in order, so a period older than the latest
//! decided one is never due again, and deciding the most recent due period
//! closes the older ones still waiting.
//!
//! The plan holds rules, the commands carry amounts: `ExecuteAllocation`
//! brings the moves [`resolve`] worked out when the user confirmed them, and
//! this module only checks them, so a replay never depends on what the plan
//! says by then. Checks on the plan itself are kept to what a replay can never
//! trip over: an envelope archived or uncapped since is the resolver's
//! business, not a refusal. The transfers of an executed period carry the
//! run's `command_id`, which is how `ReopenAllocation` and
//! [`Core::allocation_runs`] find them again.

use std::collections::HashSet;

use chrono::{DateTime, FixedOffset, NaiveDate};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use uuid::Uuid;

use crate::{
    CommandEnvelope, Core, DomainError, Result, TransactionKind, TransactionView,
    allocation::{
        AllocationBase, AllocationLine, AllocationMove, AllocationPlanPatch, AllocationPlanView,
        AllocationPreview, AllocationRule, AllocationRunView, FULL_PERCENT_BP, PendingAllocation,
        RunMove, resolve,
    },
    category::{OPENING_KEY, UNCATEGORIZED_KEY},
    query::{TRANSACTION_VIEW_COLUMNS, load_legs, transaction_view},
    recurring::{RunOutcome, Schedule},
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
    super::require_vault(tx, env.vault_id)?;
    let taken: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM allocation_plans WHERE vault_id = ?1)",
        params![env.vault_id],
        |r| r.get(0),
    )?;
    if taken {
        return Err(DomainError::AlreadyExists("allocation plan".to_string()));
    }
    schedule.validate()?;
    check_lines(tx, env.vault_id, lines)?;
    tx.execute(
        "INSERT INTO allocation_plans (id, vault_id, schedule, lines, enabled, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)",
        params![
            env.id,
            env.vault_id,
            encode_schedule(schedule)?,
            encode_lines(lines)?,
            env.author,
            now,
        ],
    )?;
    Ok(env.id)
}

/// Decided periods stay as they are: a run records what was done, whatever
/// the plan says afterwards.
pub(super) fn update_plan(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    plan_id: Uuid,
    patch: &AllocationPlanPatch,
) -> Result<()> {
    let current = load_plan(tx, env.vault_id, plan_id)?;
    if patch.is_empty() {
        return Err(invalid("nothing to update"));
    }
    let schedule = match patch.schedule {
        Some(schedule) => {
            schedule.validate()?;
            schedule
        }
        None => current.schedule,
    };
    let lines = match patch.lines.as_deref() {
        Some(lines) => {
            check_lines(tx, env.vault_id, lines)?;
            lines.to_vec()
        }
        None => current.lines,
    };
    let enabled = patch.enabled.unwrap_or(current.enabled);
    tx.execute(
        "UPDATE allocation_plans SET schedule = ?1, lines = ?2, enabled = ?3 WHERE id = ?4",
        params![
            encode_schedule(&schedule)?,
            encode_lines(&lines)?,
            enabled,
            plan_id
        ],
    )?;
    Ok(())
}

/// One `transfer_flow` from Unallocated per move, then the run. The moves may
/// name envelopes the plan no longer has: they are what the user confirmed. A
/// move the envelope's cap refuses refuses the whole command.
pub(super) fn execute_allocation(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    spec: &ExecuteSpec<'_>,
    now: i64,
) -> Result<()> {
    due_plan(tx, env.vault_id, spec.plan_id, spec.period_date)?;
    let unallocated = super::unallocated_id(tx, env.vault_id)?;
    check_moves(spec.moves, unallocated)?;
    check_total(spec.total, spec.moves)?;

    let category = super::system_category_id(tx, env.vault_id, UNCATEGORIZED_KEY)?;
    let note = super::normalize_note(spec.note);
    for movement in spec.moves {
        super::post_transfer_flow(
            tx,
            env,
            move_id(env.id, movement.flow_id),
            movement.amount,
            unallocated,
            movement.flow_id,
            category,
            note.clone(),
            spec.occurred_at,
        )?;
    }
    insert_run(
        tx,
        env,
        spec.plan_id,
        spec.period_date,
        RunOutcome::Executed,
        spec.total,
        now,
    )
}

pub(super) fn skip_allocation(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    plan_id: Uuid,
    period_date: NaiveDate,
    now: i64,
) -> Result<()> {
    due_plan(tx, env.vault_id, plan_id, period_date)?;
    insert_run(tx, env, plan_id, period_date, RunOutcome::Skipped, 0, now)
}

/// Only the latest decided period can be reopened, so the runs left are
/// always a prefix of the plan's history. The transfers still live are voided
/// like `VoidTransaction` does it, by their current legs, and the ones already
/// voided are left alone. A disabled plan can be reopened too: undoing never
/// needs the plan to be running.
pub(super) fn reopen_allocation(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    plan_id: Uuid,
    period_date: NaiveDate,
    now: i64,
) -> Result<()> {
    load_plan(tx, env.vault_id, plan_id)?;
    let runs = decided_runs(tx, plan_id)?;
    let run = runs
        .iter()
        .find(|run| run.period_date == period_date)
        .ok_or_else(|| DomainError::NotFound(format!("run for {period_date}")))?;
    if runs
        .first()
        .is_some_and(|latest| latest.period_date > period_date)
    {
        return Err(invalid("only the latest decided period can be reopened"));
    }
    if run.outcome == RunOutcome::Executed {
        let live: Vec<Uuid> = {
            let mut stmt = tx.prepare(
                "SELECT id FROM transactions
                 WHERE vault_id = ?1 AND command_id = ?2 AND voided_at IS NULL
                 ORDER BY rowid",
            )?;
            stmt.query_map(params![env.vault_id, run.command_id], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?
        };
        for transaction_id in live {
            super::void_live_transaction(tx, env, transaction_id, now)?;
        }
    }
    tx.execute(
        "DELETE FROM allocation_runs WHERE plan_id = ?1 AND period_date = ?2",
        params![plan_id, period_date.to_string()],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

impl Core {
    /// The vault's plan, if it has one.
    pub fn allocation_plan(&self, vault_id: Uuid) -> Result<Option<AllocationPlanView>> {
        vault_plan(&self.conn, vault_id)
    }

    /// The period of the plan waiting for a decision on `today`, if any: the
    /// most recent due date (up to `today` and the schedule's end) later
    /// than every decided period, with how many older ones are due too.
    /// `None` without a plan, for a disabled plan, or when nothing is due.
    ///
    /// `today` comes from the caller (system timezone): the core never
    /// guesses a timezone.
    pub fn pending_allocation(
        &self,
        vault_id: Uuid,
        today: NaiveDate,
    ) -> Result<Option<PendingAllocation>> {
        let Some(plan) = vault_plan(&self.conn, vault_id)? else {
            return Ok(None);
        };
        if !plan.enabled {
            return Ok(None);
        }
        let latest = decided_runs(&self.conn, plan.id)?
            .first()
            .map(|run| run.period_date);
        // `occurrences` is unbounded without an end date: `today` stops it.
        let mut due = plan
            .schedule
            .occurrences()
            .take_while(|date| *date <= today)
            .skip_while(|date| latest.is_some_and(|decided| *date <= decided));
        let Some(mut period_date) = due.next() else {
            return Ok(None);
        };
        let mut missed: u32 = 0;
        for date in due {
            missed = missed.saturating_add(1);
            period_date = date;
        }
        Ok(Some(PendingAllocation {
            plan_id: plan.id,
            period_date,
            missed,
        }))
    }

    /// The incomes the next execution shares out: live incomes with a
    /// positive leg on Unallocated, not an opening balance, dated on or after
    /// the plan's start (in their own offset), and recorded after the command
    /// of the plan's latest decided period (executed or skipped). Empty
    /// without a plan.
    ///
    /// "Recorded after" compares positions in the log: the income's command
    /// against the run's. A sync rebuild renumbers the log, but both sides
    /// are read here, so they move together. Each income counts once: the
    /// run that follows it moves the base past it. Refunds, opening balances,
    /// incomes put straight into an envelope and transfers into Unallocated
    /// never count.
    pub fn allocation_base(&self, vault_id: Uuid) -> Result<AllocationBase> {
        let Some(plan) = vault_plan(&self.conn, vault_id)? else {
            return Ok(AllocationBase {
                total: 0,
                incomes: Vec::new(),
            });
        };
        let after_seq = match decided_runs(&self.conn, plan.id)?.first() {
            Some(run) => Some(command_seq(&self.conn, run.command_id)?),
            None => None,
        };
        let unallocated = unallocated_flow(&self.conn, vault_id)?;
        let mut stmt = self.conn.prepare(&base_sql())?;
        let mut incomes: Vec<TransactionView> = stmt
            .query_map(
                params![
                    vault_id,
                    unallocated,
                    TransactionKind::Income.as_str(),
                    OPENING_KEY,
                    after_seq
                ],
                transaction_view,
            )?
            .collect::<std::result::Result<_, _>>()?;
        // The date where the income was recorded, in its own offset.
        let start = plan.schedule.start_date;
        incomes.retain(|income| income.occurred_at.date_naive() >= start);
        load_legs(&self.conn, &mut incomes)?;
        let total = incomes
            .iter()
            .fold(0_i64, |total, income| total.saturating_add(income.amount));
        Ok(AllocationBase { total, incomes })
    }

    /// `lines` worked out on `total` against the vault's envelopes as they
    /// are now, archived ones included, and Unallocated's balance. The lines
    /// need not be saved: the plan editor previews its draft with them.
    pub fn preview_allocation(
        &self,
        vault_id: Uuid,
        lines: &[AllocationLine],
        total: i64,
    ) -> Result<AllocationPreview> {
        let snapshot = self.snapshot(vault_id)?;
        let unallocated = snapshot
            .flows
            .iter()
            .find(|flow| flow.is_unallocated)
            .map_or(0, |flow| flow.balance);
        Ok(resolve(total, lines, &snapshot.flows, unallocated))
    }

    /// The plan's decided periods, the most recent first, at most `limit`.
    /// The moves of an executed period are its transfers as they are now,
    /// in the order they were written: an edited one shows its new amount,
    /// a deleted one is flagged `voided`. Empty without a plan.
    pub fn allocation_runs(&self, vault_id: Uuid, limit: u32) -> Result<Vec<AllocationRunView>> {
        let Some(plan) = vault_plan(&self.conn, vault_id)? else {
            return Ok(Vec::new());
        };
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        let mut stmt = self.conn.prepare(MOVES_SQL)?;
        decided_runs(&self.conn, plan.id)?
            .into_iter()
            .take(limit)
            .map(|run| {
                let moves = match run.outcome {
                    RunOutcome::Skipped => Vec::new(),
                    RunOutcome::Executed => stmt
                        .query_map(params![vault_id, run.command_id], |r| {
                            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                        })?
                        .map(|row| {
                            let (transaction_id, amount, voided, flow_id): (
                                Uuid,
                                i64,
                                bool,
                                Option<Uuid>,
                            ) = row?;
                            let flow_id = flow_id.ok_or_else(|| {
                                DomainError::Storage(format!(
                                    "allocation transfer {transaction_id} has no destination"
                                ))
                            })?;
                            Ok(RunMove {
                                flow_id,
                                amount,
                                transaction_id,
                                voided,
                            })
                        })
                        .collect::<Result<_>>()?,
                };
                Ok(AllocationRunView {
                    period_date: run.period_date,
                    outcome: run.outcome,
                    total: run.total,
                    moves,
                    created_by: run.created_by,
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The base's incomes as [`transaction_view`] reads them, oldest first: `?1`
/// the vault, `?2` its Unallocated, `?3` the income kind, `?4` the opening
/// category's key and `?5` the log seq they must come after (NULL before any
/// decision).
///
/// The `+` before `l.target_kind` is there on purpose, keep it: it takes the
/// column off `ix_legs_target`, which SQLite would otherwise pick to walk
/// every flow leg of the database for each income, instead of the income's
/// own two legs through the primary key. A test holds the plan in place.
fn base_sql() -> String {
    format!(
        "SELECT {TRANSACTION_VIEW_COLUMNS}
         FROM transactions t JOIN categories c ON c.id = t.category_id
         WHERE t.vault_id = ?1 AND t.kind = ?3 AND t.voided_at IS NULL
           AND NOT (c.is_system = 1 AND c.name_norm = ?4)
           AND EXISTS (
               SELECT 1 FROM legs l
               WHERE l.transaction_id = t.id AND +l.target_kind = 'flow'
                 AND l.target_id = ?2 AND l.amount > 0
           )
           AND (?5 IS NULL OR (SELECT seq FROM commands WHERE id = t.command_id) > ?5)
         ORDER BY t.occurred_at, t.id"
    )
}

/// The transfers of an executed run, in the order they were written, with
/// the envelope each one brings money to: `?1` the vault, `?2` the run's
/// command. The `+` keeps the legs on their primary key, as in [`base_sql`].
const MOVES_SQL: &str = "
    SELECT t.id, t.amount, t.voided_at IS NOT NULL,
           (SELECT l.target_id FROM legs l
            WHERE l.transaction_id = t.id AND +l.target_kind = 'flow' AND l.amount > 0
            ORDER BY l.ordinal LIMIT 1)
    FROM transactions t
    WHERE t.vault_id = ?1 AND t.command_id = ?2
    ORDER BY t.rowid";

/// The id of the vault's Unallocated.
fn unallocated_flow(conn: &Connection, vault_id: Uuid) -> Result<Uuid> {
    conn.query_row(
        "SELECT id FROM flows WHERE vault_id = ?1 AND system_kind = 'unallocated'",
        params![vault_id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| DomainError::InvalidFlow("missing Unallocated flow".to_string()))
}

const PLAN_COLUMNS: &str = "id, schedule, lines, enabled, created_by";

/// Raw `PLAN_COLUMNS` row; turned into an [`AllocationPlanView`] by
/// [`to_view`].
type PlanRow = (Uuid, String, String, bool, String);

fn plan_row(r: &Row<'_>) -> rusqlite::Result<PlanRow> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
}

fn to_view(row: PlanRow) -> Result<AllocationPlanView> {
    let (id, schedule, lines, enabled, created_by) = row;
    Ok(AllocationPlanView {
        id,
        schedule: serde_json::from_str(&schedule)?,
        lines: serde_json::from_str(&lines)?,
        enabled,
        created_by,
    })
}

/// The plan of a vault, if it has one.
fn vault_plan(conn: &Connection, vault_id: Uuid) -> Result<Option<AllocationPlanView>> {
    conn.query_row(
        &format!("SELECT {PLAN_COLUMNS} FROM allocation_plans WHERE vault_id = ?1"),
        params![vault_id],
        plan_row,
    )
    .optional()?
    .map(to_view)
    .transpose()
}

/// A plan by id, only when it belongs to `vault_id`: a command of one vault
/// never reaches the plan of another.
fn load_plan(conn: &Connection, vault_id: Uuid, plan_id: Uuid) -> Result<AllocationPlanView> {
    let row = conn
        .query_row(
            &format!("SELECT {PLAN_COLUMNS} FROM allocation_plans WHERE id = ?1 AND vault_id = ?2"),
            params![plan_id, vault_id],
            plan_row,
        )
        .optional()?
        .ok_or_else(|| DomainError::NotFound("allocation plan".to_string()))?;
    to_view(row)
}

/// The plan of a period that can still be executed or skipped: enabled, the
/// period one of its dates, not decided yet and later than every decided one.
fn due_plan(
    tx: &Transaction<'_>,
    vault_id: Uuid,
    plan_id: Uuid,
    period_date: NaiveDate,
) -> Result<AllocationPlanView> {
    let plan = load_plan(tx, vault_id, plan_id)?;
    if !plan.enabled {
        return Err(invalid("allocation plan is disabled"));
    }
    if !plan.schedule.is_occurrence(period_date) {
        return Err(invalid("period_date is not a due date"));
    }
    let runs = decided_runs(tx, plan_id)?;
    if runs.iter().any(|run| run.period_date == period_date) {
        return Err(DomainError::AlreadyExists(format!("run for {period_date}")));
    }
    if runs
        .first()
        .is_some_and(|latest| latest.period_date > period_date)
    {
        return Err(invalid("a later period is already decided"));
    }
    Ok(plan)
}

/// One row of `allocation_runs`.
struct RunRow {
    period_date: NaiveDate,
    outcome: RunOutcome,
    total: i64,
    command_id: Uuid,
    created_by: String,
}

/// The decided periods of a plan, the most recent first. Sorted here rather
/// than by SQLite, which would compare the dates as text.
fn decided_runs(conn: &Connection, plan_id: Uuid) -> Result<Vec<RunRow>> {
    let mut stmt = conn.prepare(
        "SELECT period_date, outcome, total, command_id, created_by
         FROM allocation_runs WHERE plan_id = ?1",
    )?;
    let rows: Vec<(String, String, i64, Uuid, String)> = stmt
        .query_map(params![plan_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut runs = rows
        .into_iter()
        .map(|(period_date, outcome, total, command_id, created_by)| {
            Ok(RunRow {
                period_date: parse_date(&period_date)?,
                outcome: RunOutcome::parse(&outcome)?,
                total,
                command_id,
                created_by,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    runs.sort_by_key(|run| std::cmp::Reverse(run.period_date));
    Ok(runs)
}

/// Where a command sits in its vault's log.
fn command_seq(conn: &Connection, command_id: Uuid) -> Result<i64> {
    conn.query_row(
        "SELECT seq FROM commands WHERE id = ?1",
        params![command_id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| DomainError::Storage(format!("no log row for command {command_id}")))
}

fn insert_run(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    plan_id: Uuid,
    period_date: NaiveDate,
    outcome: RunOutcome,
    total: i64,
    now: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO allocation_runs
            (plan_id, period_date, outcome, total, command_id, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            plan_id,
            period_date.to_string(),
            outcome.as_str(),
            total,
            env.id,
            env.author,
            now,
        ],
    )?;
    Ok(())
}

/// What a plan's lines must be on every device that replays them: not empty,
/// each envelope of the vault once, never Unallocated, each rule in range.
/// An archived or uncapped envelope is allowed: a concurrent `UpdateFlow` must
/// not make the plan fail on a rebuild, and the resolver gives such a line
/// nothing.
fn check_lines(tx: &Transaction<'_>, vault_id: Uuid, lines: &[AllocationLine]) -> Result<()> {
    if lines.is_empty() {
        return Err(invalid("the plan needs at least one line"));
    }
    let mut seen = HashSet::with_capacity(lines.len());
    for line in lines {
        check_rule(line.rule)?;
        if super::load_flow(tx, vault_id, line.flow_id)?.is_unallocated {
            return Err(DomainError::InvalidFlow(
                "Unallocated cannot be a line of the plan".to_string(),
            ));
        }
        if !seen.insert(line.flow_id) {
            return Err(invalid("an envelope appears twice in the plan"));
        }
    }
    Ok(())
}

fn check_rule(rule: AllocationRule) -> Result<()> {
    match rule {
        AllocationRule::Fixed { amount } if amount <= 0 => {
            Err(DomainError::InvalidAmount("amount must be > 0".to_string()))
        }
        AllocationRule::Percent { basis_points }
            if !(1..=FULL_PERCENT_BP).contains(&basis_points) =>
        {
            Err(DomainError::InvalidAmount(format!(
                "percent must be 1..={FULL_PERCENT_BP} basis points"
            )))
        }
        _ => Ok(()),
    }
}

/// One move per envelope, since the transfer ids derive from the envelope;
/// never Unallocated; each amount positive.
fn check_moves(moves: &[AllocationMove], unallocated: Uuid) -> Result<()> {
    if moves.is_empty() {
        return Err(invalid("nothing to allocate"));
    }
    let mut seen = HashSet::with_capacity(moves.len());
    for movement in moves {
        if movement.flow_id == unallocated {
            return Err(DomainError::InvalidFlow(
                "cannot allocate to Unallocated".to_string(),
            ));
        }
        if !seen.insert(movement.flow_id) {
            return Err(invalid("an envelope appears twice in the moves"));
        }
        if movement.amount <= 0 {
            return Err(DomainError::InvalidAmount("amount must be > 0".to_string()));
        }
    }
    Ok(())
}

/// The moves add up to no more than the total they were worked out on.
fn check_total(total: i64, moves: &[AllocationMove]) -> Result<()> {
    if total < 0 {
        return Err(DomainError::InvalidAmount("total must be >= 0".to_string()));
    }
    let sum = moves
        .iter()
        .try_fold(0_i64, |sum, movement| sum.checked_add(movement.amount));
    if sum.is_none_or(|sum| sum > total) {
        return Err(DomainError::InvalidAmount(
            "moves add up to more than the total".to_string(),
        ));
    }
    Ok(())
}

/// The id of the transfer that brings a move to `flow_id`.
fn move_id(command_id: Uuid, flow_id: Uuid) -> Uuid {
    super::derived_id(command_id, &format!("allocation:{flow_id}"))
}

fn encode_schedule(schedule: &Schedule) -> Result<String> {
    Ok(serde_json::to_string(schedule)?)
}

/// The same JSON as the command's `lines`.
fn encode_lines(lines: &[AllocationLine]) -> Result<String> {
    Ok(serde_json::to_string(lines)?)
}

fn parse_date(value: &str) -> Result<NaiveDate> {
    value
        .parse()
        .map_err(|_| DomainError::Storage(format!("invalid period date '{value}'")))
}

fn invalid(message: &str) -> DomainError {
    DomainError::InvalidCommand(message.to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// The `detail` column of `EXPLAIN QUERY PLAN` for `sql`, step by step.
    fn query_plan(core: &Core, sql: &str, args: impl rusqlite::Params) -> Vec<String> {
        let mut stmt = core
            .conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap();
        stmt.query_map(args, |r| r.get(3))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap()
    }

    /// The steps that read the legs (aliased `l`).
    fn leg_steps(plan: &[String]) -> Vec<&String> {
        plan.iter()
            .filter(|step| step.contains(" l ") || step.ends_with(" l"))
            .collect()
    }

    /// Every query that looks up a transaction's flow legs reaches them
    /// through the primary key, by transaction, never by walking every flow
    /// leg through `ix_legs_target`. A timing would be flaky; the plan is not.
    #[test]
    fn the_legs_of_a_transaction_are_found_by_its_id() {
        let core = Core::open_in_memory().unwrap();
        let id = Uuid::nil();
        let plans = [
            query_plan(
                &core,
                &base_sql(),
                params![id, id, "income", OPENING_KEY, None::<i64>],
            ),
            query_plan(&core, MOVES_SQL, params![id, id]),
        ];
        for plan in plans {
            let legs = leg_steps(&plan);
            assert!(!legs.is_empty(), "{plan:?}");
            for step in legs {
                assert!(step.starts_with("SEARCH l "), "{plan:?}");
                assert!(step.contains("(transaction_id=?"), "{plan:?}");
            }
            assert!(
                plan.iter().all(|step| !step.contains("ix_legs_target")),
                "{plan:?}"
            );
        }
    }
}
