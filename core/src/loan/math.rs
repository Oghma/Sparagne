//! The arithmetic of a loan, integers only: amounts are minor units in `i64`,
//! intermediate values `i128`, powers taken on the discount factor
//! `1 / (1 + i)` in fixed point so they never overflow, and every operation
//! checked. Nothing here reads the database.

use chrono::NaiveDate;

use super::{LoanEventView, LoanSchedule, LoanTerms};
use crate::{DomainError, Result};

fn not_implemented() -> DomainError {
    DomainError::InvalidCommand("not implemented".to_string())
}

// The two helpers below are the schedule's own: the prep leaves them unused
// until `schedule` is written, which must drop these `allow`s.
#[allow(dead_code)]
/// The interest one month accrues on `residual` at the yearly
/// `annual_rate_ppm`: `residual × rate / 12`, rounded half up to the cent.
pub(crate) fn monthly_interest(residual: i64, annual_rate_ppm: u32) -> Result<i64> {
    let _ = (residual, annual_rate_ppm);
    Err(not_implemented())
}

/// The French installment that repays `principal` in `months` at
/// `annual_rate_ppm`, rounded half up to the cent; `ceil(principal /
/// months)` at a zero rate.
pub(crate) fn installment(principal: i64, annual_rate_ppm: u32, months: u32) -> Result<i64> {
    let _ = (principal, annual_rate_ppm, months);
    Err(not_implemented())
}

#[allow(dead_code)]
/// The due dates of `months` installments from `first_due`, monthly on the
/// same day, the month's last day when it is shorter (as
/// `recurring::Schedule` with `Monthly { day }`).
pub(crate) fn due_dates(first_due: NaiveDate, months: u32) -> Vec<NaiveDate> {
    let _ = (first_due, months);
    Vec::new()
}

/// The whole schedule of `terms` with `events` applied: for each due date,
/// the installment is paid, then the events dated in `[due, next due)` apply
/// (repayments before rate changes, then by id), then the installment is
/// worked out again once if one of them asks for it. Stops at a zero
/// residual or after `terms.months` rows; never fails on what the events
/// do, it warns.
pub(crate) fn schedule(terms: &LoanTerms, events: &[LoanEventView]) -> Result<LoanSchedule> {
    let _ = (terms, events);
    Err(not_implemented())
}

/// The yearly TAN, in millionths, at which `installment` repays `principal`
/// in `months`: bisection on the monthly rate. Refused when `installment ×
/// months < principal`.
pub(crate) fn solve_rate(principal: i64, installment: i64, months: u32) -> Result<u32> {
    let _ = (principal, installment, months);
    Err(not_implemented())
}

/// The TAN of a loan whose TAEG is `taeg_ppm` and that has no fees:
/// `12 × ((1 + TAEG)^(1/12) − 1)`.
pub(crate) fn tan_from_taeg(taeg_ppm: u32) -> Result<u32> {
    let _ = taeg_ppm;
    Err(not_implemented())
}
