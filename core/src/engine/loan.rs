//! Loan commands and queries. The tables keep what the commands said (the
//! terms as columns, each event as JSON); every query works the schedule out
//! again with [`crate::loan::math`]. Validation only looks at the command
//! and the loan's own row, never at the schedule: whatever depends on it is
//! a warning of the schedule, so a rebase that reorders events never turns
//! an accepted command into a refused one.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use uuid::Uuid;

use super::{check_amount, invalid, normalize_name, normalize_note, parse_date};
use crate::{
    CommandEnvelope, Core, DomainError, Result, TransactionKind,
    loan::{
        LinkedInstallment, LoanEvent, LoanEventView, LoanInstallment, LoanOverview, LoanPatch,
        LoanSchedule, LoanScheduleRow, LoanTerms, LoanView, MAX_MONTHS, MAX_RATE_PPM, math,
    },
    recurring::{Frequency, RunOutcome, Schedule},
    store::{parse_column, query_all},
};

/// The link to `recurring_id`, when there is one, is checked before the row
/// is written, so the unique index on it never surfaces as a storage error.
pub(super) fn create_loan(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    name: &str,
    note: Option<&str>,
    terms: &LoanTerms,
    recurring_id: Option<Uuid>,
    now: i64,
) -> Result<Uuid> {
    super::require_vault(tx, env.vault_id)?;
    let name = normalize_name(name, "loan")?;
    check_terms(terms)?;
    check_contract(terms)?;
    if let Some(recurring_id) = recurring_id {
        check_link(tx, env.vault_id, env.id, recurring_id)?;
    }
    tx.execute(
        "INSERT INTO loans
            (id, vault_id, name, note, principal, annual_rate_ppm, months, first_due,
             installment, upfront_fees, installment_fee, recurring_id, archived_at,
             created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL, ?13, ?14)",
        params![
            env.id,
            env.vault_id,
            name,
            normalize_note(note),
            terms.principal,
            terms.annual_rate_ppm,
            terms.months,
            terms.first_due.to_string(),
            encode_installment(terms.installment)?,
            terms.upfront_fees,
            terms.installment_fee,
            recurring_id,
            env.author,
            now,
        ],
    )?;
    Ok(env.id)
}

/// The terms are checked as a whole once patched; the contract installment
/// against the first month's interest only when the patch touches one of
/// the three it depends on. Events the new terms leave outside the loan are
/// kept: the schedule warns about them.
pub(super) fn update_loan(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    loan_id: Uuid,
    patch: &LoanPatch,
) -> Result<()> {
    let current = active_loan(tx, env.vault_id, loan_id)?;
    if patch.is_empty() {
        return Err(invalid("nothing to update"));
    }
    let name = match patch.name.as_deref() {
        Some(name) => normalize_name(name, "loan")?,
        None => current.name,
    };
    let note = match patch.note.as_deref() {
        Some(text) => normalize_note(Some(text)),
        None => current.note,
    };
    let old = current.terms;
    let terms = LoanTerms {
        principal: patch.principal.unwrap_or(old.principal),
        annual_rate_ppm: patch.annual_rate_ppm.unwrap_or(old.annual_rate_ppm),
        months: patch.months.unwrap_or(old.months),
        first_due: patch.first_due.unwrap_or(old.first_due),
        installment: patch.installment.unwrap_or(old.installment),
        upfront_fees: patch.upfront_fees.unwrap_or(old.upfront_fees),
        installment_fee: patch.installment_fee.unwrap_or(old.installment_fee),
    };
    check_terms(&terms)?;
    if patch.principal.is_some() || patch.annual_rate_ppm.is_some() || patch.installment.is_some() {
        check_contract(&terms)?;
    }
    tx.execute(
        "UPDATE loans
         SET name = ?1, note = ?2, principal = ?3, annual_rate_ppm = ?4, months = ?5,
             first_due = ?6, installment = ?7, upfront_fees = ?8, installment_fee = ?9
         WHERE id = ?10",
        params![
            name,
            note,
            terms.principal,
            terms.annual_rate_ppm,
            terms.months,
            terms.first_due.to_string(),
            encode_installment(terms.installment)?,
            terms.upfront_fees,
            terms.installment_fee,
            loan_id,
        ],
    )?;
    Ok(())
}

/// Linking the template the loan already has, or unlinking a loan with
/// none, changes nothing and is accepted: two members doing the same never
/// refuse each other on a rebase.
pub(super) fn set_loan_recurring(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    loan_id: Uuid,
    recurring_id: Option<Uuid>,
) -> Result<()> {
    active_loan(tx, env.vault_id, loan_id)?;
    if let Some(recurring_id) = recurring_id {
        check_link(tx, env.vault_id, loan_id, recurring_id)?;
    }
    tx.execute(
        "UPDATE loans SET recurring_id = ?1 WHERE id = ?2",
        params![recurring_id, loan_id],
    )?;
    Ok(())
}

/// An archived loan keeps its template: no other loan can take it.
pub(super) fn archive_loan(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    loan_id: Uuid,
    now: i64,
) -> Result<()> {
    if load_loan(tx, env.vault_id, loan_id)?.archived {
        return Err(invalid("loan is already archived"));
    }
    tx.execute(
        "UPDATE loans SET archived_at = ?1 WHERE id = ?2",
        params![now, loan_id],
    )?;
    Ok(())
}

pub(super) fn restore_loan(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    loan_id: Uuid,
) -> Result<()> {
    if !load_loan(tx, env.vault_id, loan_id)?.archived {
        return Err(invalid("loan is already active"));
    }
    tx.execute(
        "UPDATE loans SET archived_at = NULL WHERE id = ?1",
        params![loan_id],
    )?;
    Ok(())
}

/// Returns the event's id, the command id. The date must fall inside the
/// loan as its terms draw it, from the first due date to the last; an early
/// repayment may also come before the first one.
pub(super) fn add_loan_event(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    loan_id: Uuid,
    event: &LoanEvent,
    now: i64,
) -> Result<Uuid> {
    let loan = active_loan(tx, env.vault_id, loan_id)?;
    check_event(event)?;
    check_event_date(&loan.terms, event)?;
    let kind = match event {
        LoanEvent::RateChange { .. } => "rate_change",
        LoanEvent::Prepayment { .. } => "prepayment",
    };
    tx.execute(
        "INSERT INTO loan_events (id, loan_id, date, kind, payload, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            env.id,
            loan_id,
            event.date().to_string(),
            kind,
            serde_json::to_string(event)?,
            env.author,
            now,
        ],
    )?;
    Ok(env.id)
}

pub(super) fn remove_loan_event(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    loan_id: Uuid,
    event_id: Uuid,
) -> Result<()> {
    active_loan(tx, env.vault_id, loan_id)?;
    let removed = tx.execute(
        "DELETE FROM loan_events WHERE id = ?1 AND loan_id = ?2",
        params![event_id, loan_id],
    )?;
    if removed == 0 {
        return Err(DomainError::NotFound("loan event".to_string()));
    }
    Ok(())
}

impl Core {
    /// Every loan of the vault, archived included, by name (case aside),
    /// then by id; each loan's events by date, then by id.
    pub fn loans(&self, vault_id: Uuid) -> Result<Vec<LoanView>> {
        read_loans(&self.conn, vault_id, None)
    }

    /// One loan's schedule, its events applied. `NotFound` for a loan that is
    /// not in the vault; an archived one is worked out like the others.
    pub fn loan_schedule(&self, vault_id: Uuid, loan_id: Uuid) -> Result<LoanSchedule> {
        let loan = read_loans(&self.conn, vault_id, Some(loan_id))?
            .pop()
            .ok_or_else(|| DomainError::NotFound("loan".to_string()))?;
        math::schedule(&loan.terms, &loan.events)
    }

    /// Every active loan on `today`, by name like [`Core::loans`]: what its
    /// schedule says of the installments due on or before `today` and the
    /// early repayments dated on or before it, and of what is left.
    ///
    /// `today` comes from the caller (system timezone): the core never
    /// guesses a timezone.
    pub fn loan_overview(&self, vault_id: Uuid, today: NaiveDate) -> Result<Vec<LoanOverview>> {
        read_loans(&self.conn, vault_id, None)?
            .into_iter()
            .filter(|loan| !loan.archived)
            .map(|loan| {
                let schedule = math::schedule(&loan.terms, &loan.events)?;
                let registered_count = match loan.recurring_id {
                    Some(recurring_id) => Some(registered_count(
                        &self.conn,
                        recurring_id,
                        loan.terms.first_due,
                        today,
                    )?),
                    None => None,
                };
                Ok(overview(&loan, schedule, today, registered_count))
            })
            .collect()
    }

    /// For every active loan with a linked template, the amount each due
    /// date's installment should be recorded with: the schedule's payment
    /// plus the installment fee, loan by loan in the order of
    /// [`Core::loan_overview`], each by due date.
    pub fn linked_installments(&self, vault_id: Uuid) -> Result<Vec<LinkedInstallment>> {
        let mut out = Vec::new();
        for loan in read_loans(&self.conn, vault_id, None)? {
            let Some(recurring_id) = loan.recurring_id.filter(|_| !loan.archived) else {
                continue;
            };
            let schedule = math::schedule(&loan.terms, &loan.events)?;
            out.extend(schedule.rows.iter().map(|row| LinkedInstallment {
                recurring_id,
                due_date: row.due_date,
                amount: row.payment.saturating_add(row.fee),
            }));
        }
        Ok(out)
    }

    /// The schedule of terms and events not saved yet, for the forms. The
    /// terms are refused as `CreateLoan` refuses them, and each event's own
    /// values as `AddLoanEvent` does; its date is not checked, the schedule
    /// warns about an event the loan never reaches.
    pub fn preview_loan(
        &self,
        terms: &LoanTerms,
        events: &[LoanEventView],
    ) -> Result<LoanSchedule> {
        check_terms(terms)?;
        check_contract(terms)?;
        for view in events {
            check_event(&view.event)?;
        }
        math::schedule(terms, events)
    }
}

const LOAN_COLUMNS: &str = "id, name, note, principal, annual_rate_ppm, months, first_due, \
     installment, upfront_fees, installment_fee, recurring_id, archived_at, created_by";

/// A [`LoanView`] from a [`LOAN_COLUMNS`] row, its events still empty.
fn loan_view(r: &Row<'_>) -> rusqlite::Result<LoanView> {
    Ok(LoanView {
        id: r.get(0)?,
        name: r.get(1)?,
        note: r.get(2)?,
        terms: LoanTerms {
            principal: r.get(3)?,
            annual_rate_ppm: r.get(4)?,
            months: r.get(5)?,
            first_due: parse_column(r, 6, parse_date)?,
            installment: parse_column(r, 7, |text| serde_json::from_str(text))?,
            upfront_fees: r.get(8)?,
            installment_fee: r.get(9)?,
        },
        events: Vec::new(),
        recurring_id: r.get(10)?,
        archived: r.get::<_, Option<i64>>(11)?.is_some(),
        created_by: r.get(12)?,
    })
}

/// The vault's loans with their events, or only `only` when given. Events
/// are sorted here rather than by SQLite, which would compare the dates as
/// text.
fn read_loans(conn: &Connection, vault_id: Uuid, only: Option<Uuid>) -> Result<Vec<LoanView>> {
    let mut loans = query_all(
        conn,
        &format!(
            "SELECT {LOAN_COLUMNS} FROM loans
             WHERE vault_id = ?1 AND (?2 IS NULL OR id = ?2)
             ORDER BY lower(name), id"
        ),
        params![vault_id, only],
        loan_view,
    )?;
    let rows = query_all(
        conn,
        "SELECT e.loan_id, e.id, e.payload
         FROM loan_events e JOIN loans l ON l.id = e.loan_id
         WHERE l.vault_id = ?1 AND (?2 IS NULL OR l.id = ?2)",
        params![vault_id, only],
        |r| {
            Ok((
                r.get::<_, Uuid>(0)?,
                LoanEventView {
                    id: r.get(1)?,
                    event: parse_column(r, 2, |text| serde_json::from_str(text))?,
                },
            ))
        },
    )?;
    let mut events: HashMap<Uuid, Vec<LoanEventView>> = HashMap::new();
    for (loan_id, view) in rows {
        events.entry(loan_id).or_default().push(view);
    }
    for loan in &mut loans {
        if let Some(mut list) = events.remove(&loan.id) {
            list.sort_by_key(|view| (view.event.date(), view.id));
            loan.events = list;
        }
    }
    Ok(loans)
}

/// A loan of the vault, its events left out: a command of one vault never
/// reaches the loan of another.
fn load_loan(conn: &Connection, vault_id: Uuid, loan_id: Uuid) -> Result<LoanView> {
    conn.query_row(
        &format!("SELECT {LOAN_COLUMNS} FROM loans WHERE id = ?1 AND vault_id = ?2"),
        params![loan_id, vault_id],
        loan_view,
    )
    .optional()?
    .ok_or_else(|| DomainError::NotFound("loan".to_string()))
}

/// [`load_loan`], refused when archived: an archived loan is only restored.
fn active_loan(conn: &Connection, vault_id: Uuid, loan_id: Uuid) -> Result<LoanView> {
    let loan = load_loan(conn, vault_id, loan_id)?;
    if loan.archived {
        return Err(invalid("loan is archived"));
    }
    Ok(loan)
}

/// What a template must be to record `loan_id`'s installments: of the
/// vault, an expense, active, and linked by no other loan, archived ones
/// included, as the unique index on `loans.recurring_id` says.
fn check_link(
    tx: &Transaction<'_>,
    vault_id: Uuid,
    loan_id: Uuid,
    recurring_id: Uuid,
) -> Result<()> {
    let (kind, archived): (TransactionKind, bool) = tx
        .query_row(
            "SELECT kind, archived_at IS NOT NULL FROM recurring_templates
             WHERE id = ?1 AND vault_id = ?2",
            params![recurring_id, vault_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| DomainError::NotFound("recurring".to_string()))?;
    if kind != TransactionKind::Expense {
        return Err(invalid("a loan links an expense recurring"));
    }
    if archived {
        return Err(invalid("recurring is archived"));
    }
    let taken: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM loans WHERE recurring_id = ?1 AND id IS NOT ?2)",
        params![recurring_id, loan_id],
        |r| r.get(0),
    )?;
    if taken {
        return Err(invalid("recurring is linked to another loan"));
    }
    Ok(())
}

/// The terms on their own: a principal, a rate and a length in range, fees
/// never negative, a contract installment above zero and a last due date
/// the calendar has.
fn check_terms(terms: &LoanTerms) -> Result<()> {
    if terms.principal <= 0 {
        return Err(DomainError::InvalidAmount(
            "principal must be > 0".to_string(),
        ));
    }
    check_rate(terms.annual_rate_ppm)?;
    if !(1..=MAX_MONTHS).contains(&terms.months) {
        return Err(invalid(&format!("months must be 1..={MAX_MONTHS}")));
    }
    check_fee(terms.upfront_fees, "upfront fees")?;
    check_fee(terms.installment_fee, "installment fee")?;
    check_installment(terms.installment)?;
    if last_nominal_due(terms).is_none() {
        return Err(invalid("the loan ends past the last date"));
    }
    Ok(())
}

/// A contract installment must pay more than the first month's interest,
/// which depends on the terms alone; later on, an installment that no
/// longer covers the interest is the schedule's warning.
fn check_contract(terms: &LoanTerms) -> Result<()> {
    let LoanInstallment::Contract { amount } = terms.installment else {
        return Ok(());
    };
    if amount <= math::monthly_interest(terms.principal, terms.annual_rate_ppm)? {
        return Err(DomainError::InvalidAmount(
            "the installment must be above the first month's interest".to_string(),
        ));
    }
    Ok(())
}

fn check_rate(annual_rate_ppm: u32) -> Result<()> {
    if annual_rate_ppm > MAX_RATE_PPM {
        return Err(DomainError::InvalidAmount(format!(
            "annual rate must be at most {MAX_RATE_PPM} ppm"
        )));
    }
    Ok(())
}

fn check_fee(fee: i64, label: &str) -> Result<()> {
    if fee < 0 {
        return Err(DomainError::InvalidAmount(format!("{label} must be >= 0")));
    }
    Ok(())
}

fn check_installment(installment: LoanInstallment) -> Result<()> {
    match installment {
        LoanInstallment::Computed => Ok(()),
        LoanInstallment::Contract { amount } => check_amount(amount),
    }
}

/// An event's own values: an amount above zero, a penalty never negative, a
/// rate in range.
fn check_event(event: &LoanEvent) -> Result<()> {
    match *event {
        LoanEvent::RateChange {
            annual_rate_ppm,
            installment,
            ..
        } => {
            check_rate(annual_rate_ppm)?;
            check_installment(installment)
        }
        LoanEvent::Prepayment { amount, fee, .. } => {
            check_amount(amount)?;
            check_fee(fee, "penalty")
        }
    }
}

/// An event dated inside the loan as `terms` draw it: from the first due
/// date to the last, any day before the first one too for an early
/// repayment.
fn check_event_date(terms: &LoanTerms, event: &LoanEvent) -> Result<()> {
    let date = event.date();
    let before_first = date < terms.first_due && matches!(event, LoanEvent::RateChange { .. });
    if before_first || last_nominal_due(terms).is_none_or(|last| date > last) {
        return Err(invalid("the event's date is outside the loan"));
    }
    Ok(())
}

/// The due date of the terms' last installment before any event: the day
/// of `first_due` `months − 1` months later, as a monthly template on that
/// day gives it (the month's last day when it is shorter). `None` past the
/// calendar's end.
fn last_nominal_due(terms: &LoanTerms) -> Option<NaiveDate> {
    let day = u8::try_from(terms.first_due.day()).ok()?;
    let steps = usize::try_from(terms.months.checked_sub(1)?).ok()?;
    Schedule {
        frequency: Frequency::Monthly { day },
        interval: 1,
        start_date: terms.first_due,
        end_date: None,
    }
    .occurrences()
    .nth(steps)
}

/// The same JSON as the command's `installment`.
fn encode_installment(installment: LoanInstallment) -> Result<String> {
    Ok(serde_json::to_string(&installment)?)
}

/// The executed periods of `recurring_id` from `first_due` to `today` whose
/// transaction is still live. Dates are compared here rather than by
/// SQLite, which would compare them as text.
fn registered_count(
    conn: &Connection,
    recurring_id: Uuid,
    first_due: NaiveDate,
    today: NaiveDate,
) -> Result<u32> {
    let dates = query_all(
        conn,
        "SELECT r.period_date
         FROM recurring_runs r JOIN transactions t ON t.id = r.transaction_id
         WHERE r.recurring_id = ?1 AND r.outcome = ?2 AND t.voided_at IS NULL",
        params![recurring_id, RunOutcome::Executed],
        |r| parse_column(r, 0, parse_date),
    )?;
    let count = dates
        .iter()
        .filter(|date| (first_due..=today).contains(*date))
        .count();
    Ok(u32::try_from(count).unwrap_or(u32::MAX))
}

/// `loan` on `today`, from its `schedule`.
fn overview(
    loan: &LoanView,
    schedule: LoanSchedule,
    today: NaiveDate,
    registered_count: Option<u32>,
) -> LoanOverview {
    let paid = schedule
        .rows
        .iter()
        .take_while(|row| row.due_date <= today)
        .count();
    let (done, left) = schedule.rows.split_at(paid);
    let interest = |rows: &[LoanScheduleRow]| {
        rows.iter()
            .fold(0_i64, |sum, row| sum.saturating_add(row.interest))
    };
    let residual = residual_on(loan, &schedule, paid, today);
    let next = left.first();
    LoanOverview {
        loan_id: loan.id,
        residual,
        paid_count: u32::try_from(paid).unwrap_or(u32::MAX),
        total_count: u32::try_from(schedule.rows.len()).unwrap_or(u32::MAX),
        next_due: next.map(|row| row.due_date),
        next_payment: next.map(|row| row.payment.saturating_add(row.fee)),
        capital_repaid: loan.terms.principal.saturating_sub(residual),
        interest_paid: interest(done),
        interest_left: interest(left),
        end_date: schedule.rows.last().map(|row| row.due_date),
        taeg_ppm: schedule.taeg_ppm,
        total_cost: schedule.total_cost,
        registered_count,
        warnings: schedule.warnings,
    }
}

/// The debt on `today`, once the first `paid` installments are paid. A
/// row's residual counts every event applied after it, up to the next due
/// date, and some of them may come after `today`: the last paid row is
/// worked out again from the one before, with only the early repayments
/// dated on or before `today`, each clipped to the residual as the schedule
/// clips it.
fn residual_on(loan: &LoanView, schedule: &LoanSchedule, paid: usize, today: NaiveDate) -> i64 {
    let repayments: HashMap<Uuid, (NaiveDate, i64)> = loan
        .events
        .iter()
        .filter_map(|view| match view.event {
            LoanEvent::Prepayment { date, amount, .. } => Some((view.id, (date, amount))),
            LoanEvent::RateChange { .. } => None,
        })
        .collect();
    let repay = |mut residual: i64, event_ids: &[Uuid]| {
        for id in event_ids {
            if let Some(&(date, amount)) = repayments.get(id)
                && date <= today
            {
                residual = residual.saturating_sub(amount.min(residual));
            }
        }
        residual
    };
    let first = || repay(loan.terms.principal, &schedule.event_ids_before_first);
    let Some(last) = paid.checked_sub(1) else {
        return first();
    };
    let before = match last.checked_sub(1) {
        Some(previous) => schedule.rows[previous].residual,
        None => first(),
    };
    let row = &schedule.rows[last];
    repay(before.saturating_sub(row.capital), &row.event_ids)
}
