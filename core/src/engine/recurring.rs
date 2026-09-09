//! Engine: recurring template commands and the pending-periods query.
//!
//! A template is a plan, not a transaction: it is materialized one period at a
//! time by `ExecuteRecurring` (or dismissed by `SkipRecurring`). A period is
//! pending while it has no row in `recurring_runs`, so periods missed while the
//! app was closed are backfilled for free.

use std::collections::HashSet;

use chrono::{DateTime, FixedOffset, NaiveDate};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use uuid::Uuid;

use crate::{
    CommandEnvelope, Core, DomainError, RecurringPatch, Result, TransactionKind,
    normalize_category_key,
    recurring::{PendingRecurring, RecurringRunView, RecurringView, RunOutcome, Schedule},
};

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

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Returns the template id (the command id).
pub(super) fn create_recurring(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    spec: &RecurringSpec<'_>,
    now: i64,
) -> Result<Uuid> {
    super::require_vault(tx, env.vault_id)?;
    check_kind(spec.kind)?;
    check_amount(spec.amount)?;
    spec.schedule.validate()?;
    check_wallet(tx, env.vault_id, spec.wallet_id)?;
    check_flow(tx, env.vault_id, spec.flow_id)?;
    let category = normalize_category(spec.category)?;
    let note = super::normalize_note(spec.note);
    tx.execute(
        "INSERT INTO recurring_templates
            (id, vault_id, kind, amount, wallet_id, flow_id, category, note, schedule, enabled, archived_at, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, NULL, ?10, ?11)",
        params![
            env.id,
            env.vault_id,
            spec.kind.as_str(),
            spec.amount,
            spec.wallet_id,
            spec.flow_id,
            category,
            note,
            encode_schedule(&spec.schedule)?,
            env.author,
            now,
        ],
    )?;
    Ok(env.id)
}

pub(super) fn update_recurring(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    recurring_id: Uuid,
    patch: &RecurringPatch,
) -> Result<()> {
    let current = load_template(tx, env.vault_id, recurring_id)?;
    if current.archived {
        return Err(invalid("recurring is archived"));
    }
    if patch.is_empty() {
        return Err(invalid("nothing to update"));
    }

    let amount = match patch.amount {
        Some(amount) => {
            check_amount(amount)?;
            amount
        }
        None => current.amount,
    };
    let schedule = match patch.schedule {
        Some(schedule) => {
            schedule.validate()?;
            schedule
        }
        None => current.schedule,
    };
    let wallet_id = match patch.wallet_id {
        Some(id) => {
            check_wallet(tx, env.vault_id, Some(id))?;
            Some(id)
        }
        None => current.wallet_id,
    };
    let flow_id = match patch.flow_id {
        Some(id) => {
            check_flow(tx, env.vault_id, Some(id))?;
            Some(id)
        }
        None => current.flow_id,
    };
    let category = match patch.category.as_deref() {
        Some(text) => normalize_category(Some(text))?,
        None => current.category,
    };
    let note = match patch.note.as_deref() {
        Some(text) => super::normalize_note(Some(text)),
        None => current.note,
    };
    let enabled = patch.enabled.unwrap_or(current.enabled);

    tx.execute(
        "UPDATE recurring_templates
         SET amount = ?1, wallet_id = ?2, flow_id = ?3, category = ?4, note = ?5,
             schedule = ?6, enabled = ?7
         WHERE id = ?8",
        params![
            amount,
            wallet_id,
            flow_id,
            category,
            note,
            encode_schedule(&schedule)?,
            enabled,
            recurring_id,
        ],
    )?;
    Ok(())
}

pub(super) fn archive_recurring(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    recurring_id: Uuid,
    now: i64,
) -> Result<()> {
    let current = load_template(tx, env.vault_id, recurring_id)?;
    if current.archived {
        return Err(invalid("recurring is already archived"));
    }
    tx.execute(
        "UPDATE recurring_templates SET archived_at = ?1 WHERE id = ?2",
        params![now, recurring_id],
    )?;
    Ok(())
}

pub(super) fn restore_recurring(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    recurring_id: Uuid,
) -> Result<()> {
    let current = load_template(tx, env.vault_id, recurring_id)?;
    if !current.archived {
        return Err(invalid("recurring is already active"));
    }
    tx.execute(
        "UPDATE recurring_templates SET archived_at = NULL WHERE id = ?1",
        params![recurring_id],
    )?;
    Ok(())
}

/// Returns the id of the created transaction (the command id).
pub(super) fn execute_recurring(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    recurring_id: Uuid,
    period_date: NaiveDate,
    occurred_at: DateTime<FixedOffset>,
    now: i64,
) -> Result<Uuid> {
    let template = due_template(tx, env.vault_id, recurring_id, period_date)?;
    let wallet_id = super::resolve_wallet(tx, env.vault_id, template.wallet_id)?;
    let flow_id = match template.flow_id {
        None => super::unallocated_id(tx, env.vault_id)?,
        Some(id) => {
            check_flow(tx, env.vault_id, Some(id))?;
            id
        }
    };
    let category_id = super::resolve_category(tx, env, template.category.as_deref())?;
    let signed = match template.kind {
        TransactionKind::Expense => -template.amount,
        _ => template.amount,
    };
    super::post_entry(
        tx,
        env,
        env.id,
        template.kind,
        signed,
        wallet_id,
        flow_id,
        category_id,
        template.note,
        occurred_at,
    )?;
    insert_run(
        tx,
        env,
        recurring_id,
        period_date,
        RunOutcome::Executed,
        Some(env.id),
        now,
    )?;
    Ok(env.id)
}

pub(super) fn skip_recurring(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    recurring_id: Uuid,
    period_date: NaiveDate,
    now: i64,
) -> Result<()> {
    due_template(tx, env.vault_id, recurring_id, period_date)?;
    insert_run(
        tx,
        env,
        recurring_id,
        period_date,
        RunOutcome::Skipped,
        None,
        now,
    )
}

// ---------------------------------------------------------------------------
// Read side
// ---------------------------------------------------------------------------

impl Core {
    /// Templates of a vault, oldest first.
    pub fn list_recurring(
        &self,
        vault_id: Uuid,
        include_archived: bool,
    ) -> Result<Vec<RecurringView>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {TEMPLATE_COLUMNS} FROM recurring_templates
             WHERE vault_id = ?1 AND (?2 OR archived_at IS NULL)
             ORDER BY created_at, id"
        ))?;
        let rows: Vec<TemplateRow> = stmt
            .query_map(params![vault_id, include_archived], template_row)?
            .collect::<std::result::Result<_, _>>()?;
        rows.into_iter().map(to_view).collect()
    }

    /// Enabled, non-archived templates with at least one period that has
    /// neither been executed nor skipped, `today` included.
    ///
    /// `today` comes from the caller (system timezone): the core never guesses
    /// a timezone.
    pub fn pending_recurring(
        &self,
        vault_id: Uuid,
        today: NaiveDate,
    ) -> Result<Vec<PendingRecurring>> {
        let templates = self.list_recurring(vault_id, false)?;
        let mut stmt = self
            .conn
            .prepare("SELECT period_date FROM recurring_runs WHERE recurring_id = ?1")?;
        let mut out = Vec::new();
        for template in templates {
            if !template.enabled {
                continue;
            }
            let mut due = template.schedule.due_until(today);
            if due.is_empty() {
                continue;
            }
            let handled: HashSet<NaiveDate> = stmt
                .query_map(params![template.id], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .iter()
                .map(|s| parse_date(s))
                .collect::<Result<_>>()?;
            due.retain(|date| !handled.contains(date));
            if !due.is_empty() {
                out.push(PendingRecurring { template, due });
            }
        }
        Ok(out)
    }

    /// Periods of a template the user has already decided on, oldest first.
    pub fn recurring_runs(
        &self,
        vault_id: Uuid,
        recurring_id: Uuid,
    ) -> Result<Vec<RecurringRunView>> {
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM recurring_templates WHERE id = ?1 AND vault_id = ?2)",
            params![recurring_id, vault_id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(DomainError::NotFound("recurring".to_string()));
        }
        let mut stmt = self.conn.prepare(
            "SELECT period_date, outcome, transaction_id FROM recurring_runs
             WHERE recurring_id = ?1 ORDER BY period_date",
        )?;
        let rows: Vec<(String, String, Option<Uuid>)> = stmt
            .query_map(params![recurring_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<std::result::Result<_, _>>()?;
        rows.into_iter()
            .map(|(period_date, outcome, transaction_id)| {
                Ok(RecurringRunView {
                    period_date: parse_date(&period_date)?,
                    outcome: RunOutcome::parse(&outcome)?,
                    transaction_id,
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const TEMPLATE_COLUMNS: &str =
    "id, kind, amount, wallet_id, flow_id, category, note, schedule, enabled, archived_at";

/// Raw `TEMPLATE_COLUMNS` row; turned into a [`RecurringView`] by [`to_view`].
type TemplateRow = (
    Uuid,
    String,
    i64,
    Option<Uuid>,
    Option<Uuid>,
    Option<String>,
    Option<String>,
    String,
    bool,
    Option<i64>,
);

fn template_row(r: &Row<'_>) -> rusqlite::Result<TemplateRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
        r.get(8)?,
        r.get(9)?,
    ))
}

fn to_view(row: TemplateRow) -> Result<RecurringView> {
    let (id, kind, amount, wallet_id, flow_id, category, note, schedule, enabled, archived_at) =
        row;
    Ok(RecurringView {
        id,
        kind: TransactionKind::parse(&kind)?,
        amount,
        wallet_id,
        flow_id,
        category,
        note,
        schedule: serde_json::from_str(&schedule)?,
        enabled,
        archived: archived_at.is_some(),
    })
}

fn load_template(conn: &Connection, vault_id: Uuid, recurring_id: Uuid) -> Result<RecurringView> {
    let row = conn
        .query_row(
            &format!(
                "SELECT {TEMPLATE_COLUMNS} FROM recurring_templates WHERE id = ?1 AND vault_id = ?2"
            ),
            params![recurring_id, vault_id],
            template_row,
        )
        .optional()?
        .ok_or_else(|| DomainError::NotFound("recurring".to_string()))?;
    to_view(row)
}

/// The template of a period that can still be executed or skipped.
fn due_template(
    tx: &Transaction<'_>,
    vault_id: Uuid,
    recurring_id: Uuid,
    period_date: NaiveDate,
) -> Result<RecurringView> {
    let template = load_template(tx, vault_id, recurring_id)?;
    if template.archived {
        return Err(invalid("recurring is archived"));
    }
    if !template.enabled {
        return Err(invalid("recurring is disabled"));
    }
    if !template.schedule.is_occurrence(period_date) {
        return Err(invalid("period_date is not a due date"));
    }
    let handled: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM recurring_runs WHERE recurring_id = ?1 AND period_date = ?2)",
        params![recurring_id, period_date.to_string()],
        |r| r.get(0),
    )?;
    if handled {
        return Err(DomainError::AlreadyExists(format!("run for {period_date}")));
    }
    Ok(template)
}

fn insert_run(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    recurring_id: Uuid,
    period_date: NaiveDate,
    outcome: RunOutcome,
    transaction_id: Option<Uuid>,
    now: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO recurring_runs
            (recurring_id, period_date, outcome, transaction_id, command_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            recurring_id,
            period_date.to_string(),
            outcome.as_str(),
            transaction_id,
            env.id,
            now,
        ],
    )?;
    Ok(())
}

fn check_kind(kind: TransactionKind) -> Result<()> {
    match kind {
        TransactionKind::Income | TransactionKind::Expense => Ok(()),
        _ => Err(invalid("recurring kind must be income or expense")),
    }
}

fn check_amount(amount: i64) -> Result<()> {
    if amount > 0 {
        Ok(())
    } else {
        Err(DomainError::InvalidAmount("amount must be > 0".to_string()))
    }
}

/// A wallet given on a template must exist in the vault and be active.
fn check_wallet(tx: &Transaction<'_>, vault_id: Uuid, wallet_id: Option<Uuid>) -> Result<()> {
    if wallet_id.is_some() {
        super::resolve_wallet(tx, vault_id, wallet_id)?;
    }
    Ok(())
}

/// A flow given on a template must exist in the vault and be active.
fn check_flow(tx: &Transaction<'_>, vault_id: Uuid, flow_id: Option<Uuid>) -> Result<()> {
    if let Some(id) = flow_id
        && super::load_flow(tx, vault_id, id)?.archived
    {
        return Err(invalid("flow is archived"));
    }
    Ok(())
}

/// Category text kept as typed; only its normalized key is checked here, the
/// category itself is created (or matched by alias) at execution time.
fn normalize_category(input: Option<&str>) -> Result<Option<String>> {
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    normalize_category_key(raw)?;
    Ok(Some(raw.to_string()))
}

fn encode_schedule(schedule: &Schedule) -> Result<String> {
    Ok(serde_json::to_string(schedule)?)
}

fn parse_date(value: &str) -> Result<NaiveDate> {
    value
        .parse()
        .map_err(|_| DomainError::Storage(format!("invalid period date '{value}'")))
}

fn invalid(message: &str) -> DomainError {
    DomainError::InvalidCommand(message.to_string())
}
