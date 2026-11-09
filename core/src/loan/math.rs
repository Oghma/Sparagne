//! The arithmetic of a loan, integers only: amounts are minor units in `i64`,
//! intermediate values `i128`, powers taken on the discount factor
//! `1 / (1 + i)` in fixed point so they never overflow, and every operation
//! checked. Nothing here reads the database.
//!
//! The fixed point is [`SCALE`], 10^18. A discount factor and its powers lie
//! in `[0, 1]`, an annuity factor `v + … + v^n` in `[0, 600]`; a product
//! takes the larger factor's integer part and fraction apart, so nothing
//! above 10^36 is ever formed, and an amount (at most 100 M€, 10^10 cents)
//! times a factor stays below 10^32: far inside `i128`. Each product is
//! rounded half up: the annuity factor, taken by square-and-multiply in
//! about 20 products, comes out within a few parts in 10^17, so an
//! installment of up to 100 M€ over up to 600 months at up to 50% is within
//! 10^-8 of a cent of the exact `P·i / (1 − v^n)` before its rounding to the
//! cent (6.4·10^-9 at worst against exact rationals over 4,320 cases, none
//! rounded the other way).

use std::cmp::Ordering;

use chrono::{Datelike, NaiveDate};
use uuid::Uuid;

use super::{
    LoanEvent, LoanEventView, LoanInstallment, LoanSchedule, LoanScheduleRow, LoanTerms,
    LoanWarning, MAX_MONTHS, MAX_RATE_PPM, PrepaymentMode,
};
use crate::{
    DomainError, Result,
    recurring::{Frequency, Schedule},
};

/// One in fixed point: discount factors and their powers are multiples of
/// 10^-18.
const SCALE: i128 = 1_000_000_000_000_000_000;
/// A yearly rate in millionths over twelve months: the monthly rate is
/// `annual_rate_ppm / MONTHLY_RATE_DENOMINATOR`.
const MONTHLY_RATE_DENOMINATOR: i128 = 12_000_000;
/// A rate in millionths.
const PPM: i128 = 1_000_000;

fn invalid(message: &str) -> DomainError {
    DomainError::InvalidCommand(message.to_string())
}

fn too_large() -> DomainError {
    invalid("the loan's amounts are too large to work out")
}

/// `numerator / denominator` rounded half up (towards +∞ on a tie), for a
/// positive `denominator`.
fn div_round(numerator: i128, denominator: i128) -> Result<i128> {
    if denominator <= 0 {
        return Err(too_large());
    }
    numerator
        .checked_mul(2)
        .and_then(|n| n.checked_add(denominator))
        .zip(denominator.checked_mul(2))
        .and_then(|(n, d)| n.checked_div_euclid(d))
        .ok_or_else(too_large)
}

/// The product of two non-negative fixed-point numbers, rounded half up;
/// `b` is at most a few units, `a` may be large: its integer part multiplies
/// `b` exactly and only its fraction goes through the rounding, so `a × b`
/// is never formed.
fn mul_fixed(a: i128, b: i128) -> Result<i128> {
    let whole = (a / SCALE).checked_mul(b).ok_or_else(too_large)?;
    let fraction = (a % SCALE).checked_mul(b).ok_or_else(too_large)?;
    whole
        .checked_add(div_round(fraction, SCALE)?)
        .ok_or_else(too_large)
}

/// `base^exp` in fixed point by square-and-multiply: about `2·log2(exp)`
/// products, each rounded half up.
fn pow_fixed(base: i128, exp: u32) -> Result<i128> {
    let mut result = SCALE;
    let mut square = base;
    let mut exp = exp;
    while exp > 0 {
        if exp & 1 == 1 {
            result = mul_fixed(result, square)?;
        }
        exp >>= 1;
        if exp > 0 {
            square = mul_fixed(square, square)?;
        }
    }
    Ok(result)
}

fn to_i64(value: i128) -> Result<i64> {
    i64::try_from(value).map_err(|_| too_large())
}

fn add(a: i64, b: i64) -> Result<i64> {
    a.checked_add(b).ok_or_else(too_large)
}

fn sub(a: i64, b: i64) -> Result<i64> {
    a.checked_sub(b).ok_or_else(too_large)
}

fn check_rate(annual_rate_ppm: u32) -> Result<()> {
    if annual_rate_ppm > MAX_RATE_PPM {
        return Err(invalid("the yearly rate must be at most 50%"));
    }
    Ok(())
}

fn check_months(months: u32) -> Result<()> {
    if !(1..=MAX_MONTHS).contains(&months) {
        return Err(invalid("a loan has 1 to 600 monthly installments"));
    }
    Ok(())
}

fn check_installment(installment: LoanInstallment) -> Result<()> {
    match installment {
        LoanInstallment::Contract { amount } if amount <= 0 => {
            Err(invalid("the installment must be greater than zero"))
        }
        _ => Ok(()),
    }
}

fn check_terms(terms: &LoanTerms) -> Result<()> {
    if terms.principal <= 0 {
        return Err(invalid("the amount borrowed must be greater than zero"));
    }
    check_rate(terms.annual_rate_ppm)?;
    check_months(terms.months)?;
    check_installment(terms.installment)?;
    if terms.upfront_fees < 0 || terms.installment_fee < 0 {
        return Err(invalid("a loan's fees cannot be negative"));
    }
    Ok(())
}

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
            if amount <= 0 {
                return Err(invalid("an early repayment must be greater than zero"));
            }
            if fee < 0 {
                return Err(invalid("an early repayment's penalty cannot be negative"));
            }
            Ok(())
        }
    }
}

/// The interest one month accrues on `residual` at the yearly
/// `annual_rate_ppm`: `residual × rate / 12`, rounded half up to the cent.
pub(crate) fn monthly_interest(residual: i64, annual_rate_ppm: u32) -> Result<i64> {
    let accrued = i128::from(residual)
        .checked_mul(i128::from(annual_rate_ppm))
        .ok_or_else(too_large)?;
    to_i64(div_round(accrued, MONTHLY_RATE_DENOMINATOR)?)
}

/// The French installment that repays `principal` in `months` at
/// `annual_rate_ppm`, rounded half up to the cent; `ceil(principal /
/// months)` at a zero rate.
pub(crate) fn installment(principal: i64, annual_rate_ppm: u32, months: u32) -> Result<i64> {
    if principal <= 0 {
        return Err(invalid("the amount borrowed must be greater than zero"));
    }
    check_rate(annual_rate_ppm)?;
    check_months(months)?;
    if annual_rate_ppm == 0 {
        // `principal > 0`, so this is `ceil(principal / months)`.
        return principal
            .checked_sub(1)
            .and_then(|less| less.checked_div(i64::from(months)))
            .and_then(|whole| whole.checked_add(1))
            .ok_or_else(too_large);
    }
    let (factor, _) = annuity(monthly_discount(annual_rate_ppm)?, months)?;
    let scaled = i128::from(principal)
        .checked_mul(SCALE)
        .ok_or_else(too_large)?;
    to_i64(div_round(scaled, factor)?)
}

/// `v = 1 / (1 + i) = 12 000 000 / (12 000 000 + ppm)`, in fixed point.
fn monthly_discount(annual_rate_ppm: u32) -> Result<i128> {
    div_round(
        SCALE
            .checked_mul(MONTHLY_RATE_DENOMINATOR)
            .ok_or_else(too_large)?,
        MONTHLY_RATE_DENOMINATOR
            .checked_add(i128::from(annual_rate_ppm))
            .ok_or_else(too_large)?,
    )
}

/// The annuity factor `a_n = v + v² + … + v^n` and `v^n`, in fixed point:
/// what one a month for `months` months is worth today, and what one in
/// `months` months is. `P·i / (1 − v^n)` is `P / a_n`, since `a_n = (1 −
/// v^n) / i`; the sum keeps every digit where `1 − v^n` would lose them to
/// cancellation at a small rate. Worked out by square-and-multiply on the
/// pair `(a_m, v^m)`, from the top bit of `months`: `a_2m = a_m + v^m ·
/// a_m`, `v^2m = v^m · v^m`, then for a set bit `a_(m+1) = v · (1 + a_m)`,
/// `v^(m+1) = v^m · v`.
fn annuity(discount: i128, months: u32) -> Result<(i128, i128)> {
    let mut factor: i128 = 0;
    let mut power = SCALE;
    for bit in (0..u32::BITS - months.leading_zeros()).rev() {
        factor = factor
            .checked_add(mul_fixed(factor, power)?)
            .ok_or_else(too_large)?;
        power = mul_fixed(power, power)?;
        if (months >> bit) & 1 == 1 {
            factor = mul_fixed(factor.checked_add(SCALE).ok_or_else(too_large)?, discount)?;
            power = mul_fixed(power, discount)?;
        }
    }
    Ok((factor, power))
}

/// The due dates of `months` installments from `first_due`, monthly on the
/// same day, the month's last day when it is shorter (as
/// `recurring::Schedule` with `Monthly { day }`).
pub(crate) fn due_dates(first_due: NaiveDate, months: u32) -> Vec<NaiveDate> {
    let Ok(day) = u8::try_from(first_due.day()) else {
        return Vec::new();
    };
    let monthly = Schedule {
        frequency: Frequency::Monthly { day },
        interval: 1,
        start_date: first_due,
        end_date: None,
    };
    monthly
        .occurrences()
        .take(usize::try_from(months).unwrap_or(usize::MAX))
        .collect()
}

/// Where the schedule stands between two installments.
#[derive(Clone, Copy)]
struct Plan {
    residual: i64,
    annual_rate_ppm: u32,
    installment: i64,
    /// `installment` is a contract amount below the formula: the last
    /// installment carries what it leaves.
    balloon: bool,
    /// The number of the plan's last installment, which pays what is left:
    /// the loan's last one, or where the installment last worked out again
    /// put it.
    end: u32,
}

/// What one installment pays.
struct Payment {
    payment: i64,
    interest: i64,
    capital: i64,
    /// The installment does not cover the interest: only the interest is
    /// paid, the debt stays.
    below_interest: bool,
}

impl Plan {
    /// Installment `number`: the plan's last one, or one the installment
    /// would overpay, pays the whole residual with its interest; one that
    /// does not cover the interest pays the interest alone.
    fn pay(&self, number: u32) -> Result<Payment> {
        let interest = monthly_interest(self.residual, self.annual_rate_ppm)?;
        let owed = add(self.residual, interest)?;
        Ok(if number >= self.end || self.installment >= owed {
            Payment {
                payment: owed,
                interest,
                capital: self.residual,
                below_interest: false,
            }
        } else if self.installment <= interest {
            Payment {
                payment: interest,
                interest,
                capital: 0,
                below_interest: true,
            }
        } else {
            Payment {
                payment: self.installment,
                interest,
                capital: sub(self.installment, interest)?,
                below_interest: false,
            }
        })
    }

    /// How many installments the plan has left after installment `paid`:
    /// the ones its installment and rate take to repay the residual, up to
    /// its last one.
    fn installments_left(&self, paid: u32) -> Result<u32> {
        let mut plan = *self;
        for number in paid.saturating_add(1)..=self.end {
            let payment = plan.pay(number)?;
            if payment.below_interest {
                break;
            }
            plan.residual = sub(plan.residual, payment.capital)?;
            if plan.residual == 0 {
                return Ok(number - paid);
            }
        }
        Ok(self.end.saturating_sub(paid))
    }
}

/// An event with the window it applies in: `0` before the first
/// installment, `k` right after installment `k`.
struct Ordered<'a> {
    window: usize,
    id: Uuid,
    event: &'a LoanEvent,
}

/// Prepayments come before rate changes in the same window.
const fn rank(event: &LoanEvent) -> u8 {
    match event {
        LoanEvent::Prepayment { .. } => 0,
        LoanEvent::RateChange { .. } => 1,
    }
}

/// The schedule without its totals.
#[derive(Default)]
struct Run {
    rows: Vec<LoanScheduleRow>,
    event_ids_before_first: Vec<Uuid>,
    total_prepaid: i64,
    total_penalties: i64,
    warnings: Vec<LoanWarning>,
}

impl Run {
    /// Applies the events of one window, in order, after installment `paid`
    /// (`0` before the first), then works the installment out again once if
    /// one of them asks for it, over the installments the plan had left: a
    /// rate change keeps their number, a lower installment keeps the end.
    /// Returns the ids applied; the events that find no debt left warn that
    /// they come after the end.
    fn apply(&mut self, plan: &mut Plan, window: &[Ordered<'_>], paid: u32) -> Result<Vec<Uuid>> {
        let before = *plan;
        let mut asked: Option<LoanInstallment> = None;
        let mut applied = Vec::new();
        for ordered in window {
            if plan.residual == 0 {
                self.warnings.push(LoanWarning::EventAfterEnd {
                    event_id: ordered.id,
                });
                continue;
            }
            match *ordered.event {
                LoanEvent::Prepayment {
                    amount, mode, fee, ..
                } => {
                    let repaid = amount.min(plan.residual);
                    if amount > plan.residual {
                        self.warnings.push(LoanWarning::PrepaymentClipped {
                            event_id: ordered.id,
                        });
                    }
                    plan.residual = sub(plan.residual, repaid)?;
                    self.total_prepaid = add(self.total_prepaid, repaid)?;
                    self.total_penalties = add(self.total_penalties, fee)?;
                    if mode == PrepaymentMode::Lower {
                        asked = Some(LoanInstallment::Computed);
                    }
                }
                LoanEvent::RateChange {
                    annual_rate_ppm,
                    installment,
                    ..
                } => {
                    plan.annual_rate_ppm = annual_rate_ppm;
                    asked = Some(installment);
                }
            }
            applied.push(ordered.id);
        }
        if let Some(asked) = asked
            && plan.residual > 0
        {
            let remaining = before.installments_left(paid)?;
            let formula = installment(plan.residual, plan.annual_rate_ppm, remaining)?;
            (plan.installment, plan.balloon) = match asked {
                LoanInstallment::Computed => (formula, false),
                LoanInstallment::Contract { amount } => (amount, amount < formula),
            };
            plan.end = paid.checked_add(remaining).ok_or_else(too_large)?;
        }
        Ok(applied)
    }

    /// The schedule of `terms` over `dates`, one installment each, with
    /// `events` applied.
    fn generate(terms: &LoanTerms, dates: &[NaiveDate], events: &[LoanEventView]) -> Result<Self> {
        let months = u32::try_from(dates.len()).map_err(|_| too_large())?;
        let mut ordered: Vec<Ordered<'_>> = events
            .iter()
            .map(|view| Ordered {
                window: dates.partition_point(|due| *due <= view.event.date()),
                id: view.id,
                event: &view.event,
            })
            .collect();
        ordered.sort_by_key(|o| (o.window, rank(o.event), o.id));
        let windows = |window: usize| {
            let start = ordered.partition_point(|o| o.window < window);
            let end = ordered.partition_point(|o| o.window <= window);
            &ordered[start..end]
        };

        let formula = installment(terms.principal, terms.annual_rate_ppm, months)?;
        let (first, balloon) = match terms.installment {
            LoanInstallment::Computed => (formula, false),
            LoanInstallment::Contract { amount } => (amount, amount < formula),
        };
        let mut plan = Plan {
            residual: terms.principal,
            annual_rate_ppm: terms.annual_rate_ppm,
            installment: first,
            balloon,
            end: months,
        };
        let mut run = Self::default();
        run.event_ids_before_first = run.apply(&mut plan, windows(0), 0)?;

        let mut below = false;
        let mut last_window = 0;
        for (number, due_date) in (1..=months).zip(dates.iter().copied()) {
            if plan.residual == 0 {
                break;
            }
            let paid = plan.pay(number)?;
            if paid.below_interest && !below {
                run.warnings
                    .push(LoanWarning::InstallmentBelowInterest { number });
            }
            below = paid.below_interest;
            if number >= plan.end && plan.balloon && paid.payment > plan.installment {
                run.warnings.push(LoanWarning::Balloon {
                    amount: sub(paid.payment, plan.installment)?,
                });
            }
            let annual_rate_ppm = plan.annual_rate_ppm;
            plan.residual = sub(plan.residual, paid.capital)?;
            last_window = usize::try_from(number).map_err(|_| too_large())?;
            let event_ids = run.apply(&mut plan, windows(last_window), number)?;
            run.rows.push(LoanScheduleRow {
                number,
                due_date,
                payment: paid.payment,
                interest: paid.interest,
                capital: paid.capital,
                fee: terms.installment_fee,
                residual: plan.residual,
                annual_rate_ppm,
                event_ids,
            });
        }
        let after = ordered.partition_point(|o| o.window <= last_window);
        run.warnings.extend(
            ordered[after..]
                .iter()
                .map(|o| LoanWarning::EventAfterEnd { event_id: o.id }),
        );
        Ok(run)
    }
}

/// The whole schedule of `terms` with `events` applied: for each due date,
/// the installment is paid, then the events dated in `[due, next due)` apply
/// (repayments before rate changes, then by id), then the installment is
/// worked out again once if one of them asks for it. Stops at a zero
/// residual or after `terms.months` rows; never fails on what the events
/// do, it warns. Refuses only terms or events out of their bounds; amounts
/// up to 100 M€ never overflow. A loan whose due dates would run past the
/// calendar's last year ends on the last date it has.
pub(crate) fn schedule(terms: &LoanTerms, events: &[LoanEventView]) -> Result<LoanSchedule> {
    check_terms(terms)?;
    for view in events {
        check_event(&view.event)?;
    }
    let dates = due_dates(terms.first_due, terms.months);
    let run = Run::generate(terms, &dates, events)?;
    let taeg_ppm = if events.is_empty() {
        taeg(terms, &run)
    } else {
        taeg(terms, &Run::generate(terms, &dates, &[])?)
    };

    let mut payments: i64 = 0;
    let mut total_interest: i64 = 0;
    for row in &run.rows {
        payments = add(payments, row.payment)?;
        total_interest = add(total_interest, row.interest)?;
    }
    let total_fees = i64::try_from(run.rows.len())
        .ok()
        .and_then(|rows| terms.installment_fee.checked_mul(rows))
        .ok_or_else(too_large)?;
    let total_cost = [
        terms.upfront_fees,
        total_fees,
        run.total_prepaid,
        run.total_penalties,
    ]
    .into_iter()
    .try_fold(payments, add)
    .and_then(|paid| sub(paid, terms.principal))?;

    Ok(LoanSchedule {
        rows: run.rows,
        event_ids_before_first: run.event_ids_before_first,
        total_interest,
        total_fees,
        total_prepaid: run.total_prepaid,
        total_penalties: run.total_penalties,
        total_cost,
        taeg_ppm,
        warnings: run.warnings,
    })
}

/// The TAEG of the terms alone, `run` being their schedule without events:
/// the monthly `j` at which the installments with their fee, one a month
/// from a month after the signing, are worth the principal less the upfront
/// fees, as `(1 + j)^12 − 1`. `None` when the installment never covers the
/// interest (the terms never amortize), when no such `j` exists (fees that
/// take the whole principal), or when the TAEG does not fit a `u32`.
fn taeg(terms: &LoanTerms, run: &Run) -> Option<u32> {
    if run
        .warnings
        .iter()
        .any(|w| matches!(w, LoanWarning::InstallmentBelowInterest { .. }))
    {
        return None;
    }
    let received = terms.principal.checked_sub(terms.upfront_fees)?;
    let flows = run
        .rows
        .iter()
        .map(|row| row.payment.checked_add(terms.installment_fee))
        .collect::<Option<Vec<i64>>>()?;
    let discount = discount_solving(&flows, received).ok()?;
    let yearly = pow_fixed(discount, 12).ok().filter(|w| *w > 0)?;
    let ppm = SCALE
        .checked_sub(yearly)
        .and_then(|gap| gap.checked_mul(PPM))
        .and_then(|n| div_round(n, yearly).ok())?;
    u32::try_from(ppm).ok()
}

/// `Σ flows[k] · w^(k+1)`, `w` in fixed point, the result in minor units
/// times [`SCALE`]. A run of equal flows is one annuity, deferred by the
/// flows before it: a schedule's flows, all equal but the last, take two.
fn present_value(flows: &[i64], discount: i128) -> Result<i128> {
    let mut deferral = SCALE;
    let mut total: i128 = 0;
    for run in flows.chunk_by(|a, b| a == b) {
        let Some(&flow) = run.first() else {
            continue;
        };
        let length = u32::try_from(run.len()).map_err(|_| too_large())?;
        let (factor, power) = annuity(discount, length)?;
        total = i128::from(flow)
            .checked_mul(mul_fixed(factor, deferral)?)
            .and_then(|value| total.checked_add(value))
            .ok_or_else(too_large)?;
        deferral = mul_fixed(deferral, power)?;
    }
    Ok(total)
}

/// The discount factor `w = 1 / (1 + j)`, in fixed point, at which `flows`,
/// one a month from a month on, are worth `amount`: bisection on `w` in
/// `[0, 1]`, where the present value grows with `w` (a bisection on `j`
/// would need an upper bound). Refused when the flows do not add up to
/// `amount` or `amount` is not positive.
fn discount_solving(flows: &[i64], amount: i64) -> Result<i128> {
    if amount <= 0 {
        return Err(invalid("the amount must be greater than zero"));
    }
    let goal = i128::from(amount)
        .checked_mul(SCALE)
        .ok_or_else(too_large)?;
    let undiscounted = present_value(flows, SCALE)?;
    if undiscounted < goal {
        return Err(invalid("the payments do not repay the amount"));
    }
    if undiscounted == goal {
        return Ok(SCALE);
    }
    // present_value(low) <= goal < present_value(high)
    let (mut low, mut high) = (0, SCALE);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if present_value(flows, middle)? <= goal {
            low = middle;
        } else {
            high = middle;
        }
    }
    Ok(low)
}

/// The yearly rate in millionths, `12 × j`, of the discount factor `w =
/// 1 / (1 + j)`: `12 000 000 × (1 − w) / w`, refused above 50%.
fn yearly_rate(discount: i128) -> Result<u32> {
    let above = || invalid("the yearly rate would be above 50%");
    if discount <= 0 {
        return Err(above());
    }
    let ppm = SCALE
        .checked_sub(discount)
        .and_then(|gap| gap.checked_mul(MONTHLY_RATE_DENOMINATOR))
        .ok_or_else(too_large)
        .and_then(|n| div_round(n, discount))?;
    u32::try_from(ppm)
        .ok()
        .filter(|ppm| *ppm <= MAX_RATE_PPM)
        .ok_or_else(above)
}

/// The yearly TAN, in millionths, at which `installment` repays `principal`
/// in `months`: bisection on the monthly rate. Refused when `installment ×
/// months < principal`, and when the rate would be above `MAX_RATE_PPM`.
pub(crate) fn solve_rate(principal: i64, installment: i64, months: u32) -> Result<u32> {
    if principal <= 0 {
        return Err(invalid("the amount borrowed must be greater than zero"));
    }
    if installment <= 0 {
        return Err(invalid("the installment must be greater than zero"));
    }
    check_months(months)?;
    let paid = i128::from(installment)
        .checked_mul(i128::from(months))
        .ok_or_else(too_large)?;
    match paid.cmp(&i128::from(principal)) {
        Ordering::Less => Err(invalid(
            "the installments add up to less than the amount borrowed",
        )),
        Ordering::Equal => Ok(0),
        Ordering::Greater => {
            // installment × a_n(w) = principal: bisection on w in [0, 1]
            // for the annuity factor principal / installment, which grows
            // with w.
            let goal = div_round(
                i128::from(principal)
                    .checked_mul(SCALE)
                    .ok_or_else(too_large)?,
                i128::from(installment),
            )?;
            // annuity(low).0 <= goal < annuity(high).0
            let (mut low, mut high) = (0, SCALE);
            while high - low > 1 {
                let middle = low + (high - low) / 2;
                if annuity(middle, months)?.0 <= goal {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            yearly_rate(low)
        }
    }
}

/// The TAN of a loan whose TAEG is `taeg_ppm` and that has no fees:
/// `12 × ((1 + TAEG)^(1/12) − 1)`. Refused when it would be above
/// `MAX_RATE_PPM` (a TAEG above about 63.2%).
pub(crate) fn tan_from_taeg(taeg_ppm: u32) -> Result<u32> {
    // w^12 = 1 / (1 + TAEG), w = 1 / (1 + j): bisection on w in [0, 1],
    // where w^12 grows with w.
    let goal = div_round(
        SCALE.checked_mul(PPM).ok_or_else(too_large)?,
        PPM.checked_add(i128::from(taeg_ppm))
            .ok_or_else(too_large)?,
    )?;
    if goal == SCALE {
        return Ok(0);
    }
    // pow_fixed(low, 12) <= goal < pow_fixed(high, 12)
    let (mut low, mut high) = (0, SCALE);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if pow_fixed(middle, 12)? <= goal {
            low = middle;
        } else {
            high = middle;
        }
    }
    yearly_rate(low)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::collections::HashMap;

    use chrono::{Months, TimeDelta};

    use super::*;

    /// The largest amount the engine accepts, 100 M€.
    const MAX_AMOUNT: i64 = 10_000_000_000;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn terms(principal: i64, annual_rate_ppm: u32, months: u32) -> LoanTerms {
        LoanTerms {
            principal,
            annual_rate_ppm,
            months,
            first_due: date(2026, 1, 31),
            installment: LoanInstallment::Computed,
            upfront_fees: 0,
            installment_fee: 0,
        }
    }

    fn contract(mut terms: LoanTerms, amount: i64) -> LoanTerms {
        terms.installment = LoanInstallment::Contract { amount };
        terms
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn prepayment(n: u128, date: NaiveDate, amount: i64, mode: PrepaymentMode) -> LoanEventView {
        LoanEventView {
            id: id(n),
            event: LoanEvent::Prepayment {
                date,
                amount,
                mode,
                fee: 0,
            },
        }
    }

    fn rate_change(
        n: u128,
        date: NaiveDate,
        annual_rate_ppm: u32,
        installment: LoanInstallment,
    ) -> LoanEventView {
        LoanEventView {
            id: id(n),
            event: LoanEvent::RateChange {
                date,
                annual_rate_ppm,
                installment,
            },
        }
    }

    /// The due date of installment `number` of `terms`.
    fn due(terms: &LoanTerms, number: u32) -> NaiveDate {
        *due_dates(terms.first_due, number).last().unwrap()
    }

    fn plus_days(day: NaiveDate, days: i64) -> NaiveDate {
        day.checked_add_signed(TimeDelta::try_days(days).unwrap())
            .unwrap()
    }

    fn run(terms: &LoanTerms, events: &[LoanEventView]) -> LoanSchedule {
        let schedule = schedule(terms, events).unwrap();
        assert_invariants(terms, events, &schedule);
        schedule
    }

    /// What every schedule keeps, whatever its terms and events.
    fn assert_invariants(terms: &LoanTerms, events: &[LoanEventView], s: &LoanSchedule) {
        let dates = due_dates(terms.first_due, terms.months);
        assert!(s.rows.len() <= dates.len());
        let by_id: HashMap<Uuid, LoanEvent> = events.iter().map(|e| (e.id, e.event)).collect();

        // Every event is applied once, in its window, or warned after the end.
        let mut seen: Vec<Uuid> = s.event_ids_before_first.clone();
        for event_id in &s.event_ids_before_first {
            assert!(by_id[event_id].date() < dates[0]);
        }
        for (index, row) in s.rows.iter().enumerate() {
            for event_id in &row.event_ids {
                let day = by_id[event_id].date();
                assert!(day >= dates[index]);
                if let Some(next) = dates.get(index + 1) {
                    assert!(day < *next);
                }
            }
            let ranks: Vec<(u8, Uuid)> = row
                .event_ids
                .iter()
                .map(|event_id| (rank(&by_id[event_id]), *event_id))
                .collect();
            assert!(ranks.is_sorted(), "repayments first, then by id");
            seen.extend(&row.event_ids);
        }
        for warning in &s.warnings {
            match warning {
                LoanWarning::EventAfterEnd { event_id } => seen.push(*event_id),
                LoanWarning::PrepaymentClipped { event_id } => {
                    assert!(matches!(by_id[event_id], LoanEvent::Prepayment { .. }));
                    assert!(!s.warnings.contains(&LoanWarning::EventAfterEnd {
                        event_id: *event_id
                    }));
                }
                LoanWarning::Balloon { amount } => assert!(*amount > 0),
                LoanWarning::InstallmentBelowInterest { number } => {
                    assert!((1..=terms.months).contains(number));
                }
            }
        }
        seen.sort();
        let mut all: Vec<Uuid> = events.iter().map(|e| e.id).collect();
        all.sort();
        assert_eq!(seen, all, "every event applied once or after the end");

        // Each row: due date, split, rate, residual.
        let mut rate = terms.annual_rate_ppm;
        for event_id in &s.event_ids_before_first {
            if let LoanEvent::RateChange {
                annual_rate_ppm, ..
            } = by_id[event_id]
            {
                rate = annual_rate_ppm;
            }
        }
        let mut previous: Option<i64> = s
            .event_ids_before_first
            .is_empty()
            .then_some(terms.principal);
        let mut capital = 0;
        for (index, row) in s.rows.iter().enumerate() {
            assert_eq!(row.number, u32::try_from(index + 1).unwrap());
            assert_eq!(row.due_date, dates[index]);
            assert_eq!(row.annual_rate_ppm, rate);
            assert!(row.interest >= 0 && row.capital >= 0 && row.residual >= 0);
            assert_eq!(row.payment, row.interest + row.capital);
            assert_eq!(row.fee, terms.installment_fee);
            if let Some(before) = previous {
                assert_eq!(row.interest, monthly_interest(before, rate).unwrap());
                assert!(row.capital <= before);
                assert!(row.residual <= before - row.capital);
            }
            previous = Some(row.residual);
            capital += row.capital;
            for event_id in &row.event_ids {
                if let LoanEvent::RateChange {
                    annual_rate_ppm, ..
                } = by_id[event_id]
                {
                    rate = annual_rate_ppm;
                }
            }
        }
        // The installment holds between events, the last one aside.
        for pair in s.rows.windows(2).take(s.rows.len().saturating_sub(2)) {
            if pair[0].event_ids.is_empty() {
                assert_eq!(pair[0].payment, pair[1].payment, "row {}", pair[1].number);
            }
        }

        // The debt is repaid exactly once.
        assert_eq!(capital + s.total_prepaid, terms.principal);
        match s.rows.last() {
            Some(last) => assert_eq!(last.residual, 0),
            None => assert_eq!(s.total_prepaid, terms.principal),
        }

        // Totals.
        let rows = i64::try_from(s.rows.len()).unwrap();
        assert_eq!(
            s.total_interest,
            s.rows.iter().map(|r| r.interest).sum::<i64>()
        );
        assert_eq!(s.total_fees, terms.installment_fee * rows);
        let payments: i64 = s.rows.iter().map(|r| r.payment).sum();
        assert_eq!(
            s.total_cost,
            payments + terms.upfront_fees + s.total_fees + s.total_prepaid + s.total_penalties
                - terms.principal
        );
        assert_eq!(
            s.total_cost,
            s.total_interest + terms.upfront_fees + s.total_fees + s.total_penalties
        );
    }

    #[test]
    fn monthly_interest_is_rounded_half_up() {
        assert_eq!(monthly_interest(20_000_000, 30_000).unwrap(), 50_000);
        assert_eq!(monthly_interest(1_000_000, 75_000).unwrap(), 6_250);
        // 1 × 6% / 12 = 0.005 € exactly: half a cent goes up.
        assert_eq!(monthly_interest(1, 6_000_000).unwrap(), 1);
        assert_eq!(monthly_interest(1, 5_999_999).unwrap(), 0);
        assert_eq!(monthly_interest(0, 500_000).unwrap(), 0);
        assert_eq!(monthly_interest(12_345, 0).unwrap(), 0);
        assert_eq!(monthly_interest(MAX_AMOUNT, 500_000).unwrap(), 416_666_667);
    }

    /// principal, TAN, months, upfront fees, installment fee → installment,
    /// last payment, rows, total interest, TAEG; worked out with exact
    /// rationals (Python `fractions`/`decimal`), not with this module.
    #[allow(clippy::type_complexity)]
    const REFERENCE: [(i64, u32, u32, i64, i64, i64, i64, usize, i64, Option<u32>); 21] = [
        // 200 000 € at 3% over 25 years, 10 000 € at 7.5% over 5.
        (
            20_000_000,
            30_000,
            300,
            0,
            0,
            94_842,
            94_960,
            300,
            8_452_718,
            Some(30_416),
        ),
        (
            1_000_000,
            75_000,
            60,
            0,
            0,
            20_038,
            20_035,
            60,
            202_277,
            Some(77_633),
        ),
        (1, 1, 1, 0, 0, 1, 1, 1, 0, Some(0)),
        // 0.04 cent rounds to nothing: the debt never goes down.
        (1, 500_000, 600, 0, 0, 0, 1, 600, 0, None),
        (100, 1, 600, 0, 0, 0, 100, 600, 0, None),
        (
            1_000_000_000,
            1,
            1,
            0,
            0,
            1_000_000_083,
            1_000_000_083,
            1,
            83,
            Some(1),
        ),
        // Exactly 500 000 062.500 001 and 500 000 437.500 043 cents: a
        // fixed point on 1 − v^n would round both down.
        (
            1_000_000_000,
            1,
            2,
            0,
            0,
            500_000_063,
            500_000_062,
            2,
            125,
            Some(1),
        ),
        (
            1_000_000_000,
            7,
            2,
            0,
            0,
            500_000_438,
            500_000_437,
            2,
            875,
            Some(7),
        ),
        // At 50% over 50 years the first capital share is under a cent.
        (
            1_000_000_000,
            500_000,
            600,
            0,
            0,
            41_666_667,
            1_041_666_667,
            600,
            25_000_000_200,
            None,
        ),
        (
            999_999_999,
            499_999,
            599,
            0,
            0,
            41_666_583,
            1_041_666_582,
            599,
            24_958_283_217,
            None,
        ),
        (
            1_000_000_000,
            150_000,
            600,
            0,
            0,
            12_507_247,
            12_473_356,
            600,
            6_504_314_309,
            Some(160_755),
        ),
        (12_345, 12_345, 12, 0, 0, 1_036, 1_031, 12, 82, Some(12_313)),
        (
            50_000_000,
            45_000,
            360,
            0,
            0,
            253_343,
            253_093,
            360,
            41_203_230,
            Some(45_940),
        ),
        (
            3_000_000,
            89_900,
            84,
            0,
            0,
            48_252,
            48_251,
            84,
            1_053_167,
            Some(93_698),
        ),
        (
            25_000_000,
            39_000,
            240,
            0,
            0,
            150_181,
            150_186,
            240,
            11_043_445,
            Some(39_705),
        ),
        (
            15_000_000,
            31_250,
            300,
            0,
            0,
            72_111,
            72_000,
            300,
            6_633_189,
            Some(31_702),
        ),
        (
            983_258_730,
            220_502,
            349,
            0,
            0,
            18_099_008,
            18_110_000,
            349,
            5_333_306_054,
            Some(244_210),
        ),
        // A zero rate: ceil(P / n), the last one smaller.
        (1_000, 0, 3, 0, 0, 334, 332, 3, 0, Some(0)),
        (
            999_999_999,
            0,
            600,
            0,
            0,
            1_666_667,
            1_666_466,
            600,
            0,
            Some(0),
        ),
        // Fees in the TAEG.
        (
            1_000_000,
            75_000,
            60,
            30_000,
            200,
            20_038,
            20_035,
            60,
            202_277,
            Some(96_166),
        ),
        (
            20_000_000,
            30_000,
            300,
            350_000,
            150,
            94_842,
            94_960,
            300,
            8_452_718,
            Some(32_227),
        ),
    ];

    #[test]
    fn schedules_match_the_exact_reference_table() {
        for (principal, rate, months, upfront, fee, regular, last, rows, interest, taeg) in
            REFERENCE
        {
            let case = (principal, rate, months);
            assert_eq!(
                installment(principal, rate, months).unwrap(),
                regular,
                "{case:?}"
            );
            let mut loan = terms(principal, rate, months);
            loan.upfront_fees = upfront;
            loan.installment_fee = fee;
            let s = run(&loan, &[]);
            assert_eq!(s.rows.len(), rows, "{case:?}");
            assert_eq!(s.rows.last().unwrap().payment, last, "{case:?}");
            assert_eq!(s.total_interest, interest, "{case:?}");
            assert_eq!(s.taeg_ppm, taeg, "{case:?}");
        }
    }

    #[test]
    fn a_mortgage_and_a_personal_loan_match_the_bank() {
        let mortgage = run(&terms(20_000_000, 30_000, 300), &[]);
        assert_eq!(mortgage.rows[0].payment, 94_842);
        assert_eq!(mortgage.rows[0].interest, 50_000);
        assert_eq!(mortgage.rows[0].capital, 44_842);
        assert_eq!(mortgage.rows[299].payment, 94_960);
        assert_eq!(mortgage.total_interest, 8_452_718);
        assert!(mortgage.warnings.is_empty());

        let personal = run(&terms(1_000_000, 75_000, 60), &[]);
        assert_eq!(personal.rows.len(), 60);
        assert!(personal.rows[..59].iter().all(|r| r.payment == 20_038));
        assert_eq!(personal.rows[59].payment, 20_035);
        assert_eq!(personal.total_interest, 202_277);
        assert_eq!(personal.total_cost, 202_277);
        // TAEG without fees 7.763%, and back to the 7.5% TAN.
        assert_eq!(personal.taeg_ppm, Some(77_633));
        assert_eq!(tan_from_taeg(77_633).unwrap(), 75_000);
    }

    #[test]
    fn installment_at_a_zero_rate_is_the_ceiling() {
        assert_eq!(installment(1_000, 0, 3).unwrap(), 334);
        assert_eq!(installment(900, 0, 3).unwrap(), 300);
        assert_eq!(installment(1, 0, 600).unwrap(), 1);
        let s = run(&terms(1_000, 0, 3), &[]);
        let payments: Vec<i64> = s.rows.iter().map(|r| r.payment).collect();
        assert_eq!(payments, [334, 334, 332]);
        // The ceiling can repay before the last month: 10 cents in 6.
        assert_eq!(run(&terms(10, 0, 6), &[]).rows.len(), 5);
    }

    #[test]
    fn installment_refuses_out_of_bounds() {
        for (principal, rate, months) in [
            (0, 30_000, 12),
            (-1, 30_000, 12),
            (1_000, 30_000, 0),
            (1_000, 30_000, 601),
            (1_000, 500_001, 12),
        ] {
            assert!(matches!(
                installment(principal, rate, months),
                Err(DomainError::InvalidCommand(_))
            ));
        }
    }

    #[test]
    fn the_largest_amounts_never_overflow() {
        for rate in [0, 1, 30_000, 499_999, 500_000] {
            for months in [1, 2, 299, 600] {
                let r = installment(MAX_AMOUNT, rate, months).unwrap();
                assert!(i128::from(r) * i128::from(months) >= i128::from(MAX_AMOUNT));
                let mut loan = terms(MAX_AMOUNT, rate, months);
                loan.upfront_fees = MAX_AMOUNT;
                loan.installment_fee = MAX_AMOUNT;
                let events = [
                    LoanEventView {
                        id: id(1),
                        event: LoanEvent::Prepayment {
                            date: loan.first_due,
                            amount: MAX_AMOUNT,
                            mode: PrepaymentMode::Lower,
                            fee: MAX_AMOUNT,
                        },
                    },
                    rate_change(2, date(2025, 1, 1), 500_000, LoanInstallment::Computed),
                    rate_change(
                        3,
                        loan.first_due,
                        1,
                        LoanInstallment::Contract { amount: MAX_AMOUNT },
                    ),
                ];
                run(&loan, &events);
                run(&loan, &events[1..]);
                run(&contract(loan, 1), &events[1..]);
                run(&contract(loan, 1), &[]);
                run(&contract(loan, MAX_AMOUNT), &[]);
                // The upfront fees take the whole principal: no TAEG.
                assert_eq!(run(&loan, &[]).taeg_ppm, None);
            }
        }
    }

    #[test]
    fn due_dates_keep_the_day_and_clamp_to_the_month_end() {
        assert_eq!(
            due_dates(date(2026, 1, 31), 5),
            [
                date(2026, 1, 31),
                date(2026, 2, 28),
                date(2026, 3, 31),
                date(2026, 4, 30),
                date(2026, 5, 31)
            ]
        );
        assert_eq!(
            due_dates(date(2027, 12, 30), 4),
            [
                date(2027, 12, 30),
                date(2028, 1, 30),
                date(2028, 2, 29),
                date(2028, 3, 30)
            ]
        );
        assert_eq!(
            due_dates(date(2026, 11, 15), 3),
            [date(2026, 11, 15), date(2026, 12, 15), date(2027, 1, 15)]
        );
        let all = due_dates(date(2026, 1, 31), 600);
        assert_eq!(all.len(), 600);
        assert_eq!(all[599], date(2075, 12, 31));
        assert!(due_dates(date(2026, 1, 31), 0).is_empty());
    }

    #[test]
    fn a_loan_running_past_the_calendar_ends_on_its_last_date() {
        let first_due = NaiveDate::MAX.checked_sub_months(Months::new(3)).unwrap();
        let dates = due_dates(first_due, 12);
        assert!((1..12).contains(&dates.len()));
        let mut loan = terms(1_000_000, 75_000, 12);
        loan.first_due = first_due;
        let s = run(&loan, &[]);
        assert_eq!(s.rows.len(), dates.len());
        assert_eq!(s.rows.last().unwrap().residual, 0);
    }

    #[test]
    fn a_contract_installment_above_the_formula_ends_early() {
        let s = run(&contract(terms(1_000_000, 75_000, 60), 25_000), &[]);
        assert!(s.rows.len() < 60);
        let (last, rest) = s.rows.split_last().unwrap();
        assert!(rest.iter().all(|r| r.payment == 25_000));
        assert!(last.payment <= 25_000);
        assert!(s.warnings.is_empty());
    }

    #[test]
    fn a_contract_installment_below_the_formula_leaves_a_balloon() {
        let s = run(&contract(terms(1_000_000, 75_000, 60), 15_000), &[]);
        assert_eq!(s.rows.len(), 60);
        let last = s.rows[59].payment;
        assert!(last > 15_000);
        assert_eq!(
            s.warnings,
            [LoanWarning::Balloon {
                amount: last - 15_000
            }]
        );
    }

    #[test]
    fn a_contract_installment_equal_to_the_formula_has_no_balloon() {
        // The last installment is 1.18 € above the others, from the
        // rounding: not a balloon.
        let computed = run(&terms(20_000_000, 30_000, 300), &[]);
        let same = run(&contract(terms(20_000_000, 30_000, 300), 94_842), &[]);
        assert_eq!(same.rows, computed.rows);
        assert!(same.warnings.is_empty());
    }

    #[test]
    fn an_installment_below_the_interest_warns_and_still_ends() {
        let loan = terms(1_000_000, 75_000, 60);
        let s = run(
            &loan,
            &[rate_change(
                1,
                due(&loan, 6),
                500_000,
                LoanInstallment::Contract { amount: 1_000 },
            )],
        );
        assert_eq!(s.rows.len(), 60);
        let stuck = s.rows[5].residual;
        assert!(
            s.rows[6..59]
                .iter()
                .all(|r| r.capital == 0 && r.residual == stuck && r.payment == r.interest)
        );
        assert_eq!(s.rows[59].capital, stuck);
        assert_eq!(
            s.warnings[0],
            LoanWarning::InstallmentBelowInterest { number: 7 }
        );
        assert_eq!(
            s.warnings[1],
            LoanWarning::Balloon {
                amount: s.rows[59].payment - 1_000
            }
        );
        assert_eq!(s.warnings.len(), 2);
    }

    #[test]
    fn terms_that_never_cover_the_interest_have_no_taeg() {
        let s = run(&contract(terms(1_000_000, 75_000, 60), 6_000), &[]);
        assert_eq!(
            s.warnings[0],
            LoanWarning::InstallmentBelowInterest { number: 1 }
        );
        assert_eq!(s.rows.len(), 60);
        assert_eq!(s.taeg_ppm, None);
    }

    #[test]
    fn the_taeg_counts_the_fees_and_ignores_the_events() {
        let mut loan = terms(1_000_000, 75_000, 60);
        loan.upfront_fees = 30_000;
        loan.installment_fee = 200;
        let alone = run(&loan, &[]);
        assert_eq!(alone.taeg_ppm, Some(96_166));
        let events = [
            prepayment(1, due(&loan, 12), 300_000, PrepaymentMode::Shorten),
            rate_change(2, due(&loan, 24), 120_000, LoanInstallment::Computed),
        ];
        assert_eq!(run(&loan, &events).taeg_ppm, Some(96_166));
        loan.upfront_fees = loan.principal;
        assert_eq!(run(&loan, &[]).taeg_ppm, None);
    }

    #[test]
    fn a_prepayment_before_the_first_installment_lowers_the_amount_borrowed() {
        let loan = terms(20_000_000, 30_000, 300);
        let before = date(2026, 1, 10);
        let shorten = run(
            &loan,
            &[prepayment(1, before, 5_000_000, PrepaymentMode::Shorten)],
        );
        assert_eq!(shorten.event_ids_before_first, [id(1)]);
        assert_eq!(
            shorten.rows[0].interest,
            monthly_interest(15_000_000, 30_000).unwrap()
        );
        assert_eq!(shorten.rows[0].payment, 94_842);
        assert!(shorten.rows.len() < 300);

        let lower = run(
            &loan,
            &[prepayment(1, before, 5_000_000, PrepaymentMode::Lower)],
        );
        assert_eq!(lower.event_ids_before_first, [id(1)]);
        assert_eq!(
            lower.rows[0].payment,
            installment(15_000_000, 30_000, 300).unwrap()
        );
        assert_eq!(lower.rows.len(), 300);
    }

    #[test]
    fn a_prepayment_of_the_whole_debt_before_the_first_installment_leaves_no_rows() {
        let loan = terms(1_000_000, 75_000, 60);
        let s = run(
            &loan,
            &[
                prepayment(1, date(2026, 1, 1), 1_000_000, PrepaymentMode::Shorten),
                rate_change(2, date(2026, 1, 2), 60_000, LoanInstallment::Computed),
                prepayment(3, due(&loan, 3), 1_000, PrepaymentMode::Lower),
            ],
        );
        assert!(s.rows.is_empty());
        assert_eq!(s.event_ids_before_first, [id(1)]);
        assert_eq!(s.total_prepaid, 1_000_000);
        assert_eq!(s.total_cost, 0);
        assert_eq!(
            s.warnings,
            [
                LoanWarning::EventAfterEnd { event_id: id(2) },
                LoanWarning::EventAfterEnd { event_id: id(3) },
            ]
        );
        // The TAEG is the terms', events aside.
        assert_eq!(s.taeg_ppm, Some(77_633));
    }

    #[test]
    fn shorten_keeps_the_installment_and_ends_earlier() {
        let loan = terms(1_000_000, 75_000, 60);
        let s = run(
            &loan,
            &[prepayment(
                1,
                due(&loan, 12),
                300_000,
                PrepaymentMode::Shorten,
            )],
        );
        assert_eq!(s.rows[11].event_ids, [id(1)]);
        assert_eq!(s.total_prepaid, 300_000);
        assert!(s.rows.len() < 60);
        let (last, rest) = s.rows.split_last().unwrap();
        assert!(rest.iter().all(|r| r.payment == 20_038));
        assert!(last.payment <= 20_038);
        assert!(s.warnings.is_empty());
    }

    #[test]
    fn lower_keeps_the_end_and_lowers_the_installment() {
        let loan = terms(1_000_000, 75_000, 60);
        let s = run(
            &loan,
            &[prepayment(
                1,
                due(&loan, 12),
                300_000,
                PrepaymentMode::Lower,
            )],
        );
        assert_eq!(s.rows.len(), 60);
        let lowered = installment(s.rows[11].residual, 75_000, 48).unwrap();
        assert!(lowered < 20_038);
        assert!(s.rows[12..59].iter().all(|r| r.payment == lowered));
        assert!(s.warnings.is_empty());
    }

    #[test]
    fn a_rate_change_applies_to_the_installments_due_after_its_date() {
        let loan = terms(20_000_000, 30_000, 300);
        let on = run(
            &loan,
            &[rate_change(
                1,
                due(&loan, 24),
                45_000,
                LoanInstallment::Computed,
            )],
        );
        assert_eq!(on.rows[23].annual_rate_ppm, 30_000);
        assert_eq!(on.rows[23].event_ids, [id(1)]);
        assert_eq!(on.rows[24].annual_rate_ppm, 45_000);
        assert_eq!(
            on.rows[24].payment,
            installment(on.rows[23].residual, 45_000, 276).unwrap()
        );
        // The number of installments stays.
        assert_eq!(on.rows.len(), 300);

        let day_before = plus_days(due(&loan, 24), -1);
        let before = run(
            &loan,
            &[rate_change(
                1,
                day_before,
                45_000,
                LoanInstallment::Computed,
            )],
        );
        assert_eq!(before.rows[22].event_ids, [id(1)]);
        assert_eq!(before.rows[23].annual_rate_ppm, 45_000);
        assert_eq!(
            before.rows[23].payment,
            installment(before.rows[22].residual, 45_000, 277).unwrap()
        );
        assert_eq!(before.rows.len(), 300);
    }

    #[test]
    fn a_rate_change_with_the_banks_amount_takes_it() {
        let loan = terms(20_000_000, 30_000, 300);
        let s = run(
            &loan,
            &[rate_change(
                1,
                due(&loan, 24),
                45_000,
                LoanInstallment::Contract { amount: 120_000 },
            )],
        );
        assert_eq!(s.rows[24].payment, 120_000);
        assert!(s.rows.len() < 300);
        assert!(s.warnings.is_empty());
    }

    #[test]
    fn prepayments_apply_before_rate_changes_and_then_by_id() {
        let loan = terms(1_000_000, 75_000, 60);
        let window = due(&loan, 12);
        let s = run(
            &loan,
            &[
                rate_change(1, window, 60_000, LoanInstallment::Computed),
                prepayment(3, plus_days(window, 10), 100_000, PrepaymentMode::Lower),
                prepayment(2, plus_days(window, 20), 50_000, PrepaymentMode::Shorten),
            ],
        );
        assert_eq!(s.rows[11].event_ids, [id(2), id(3), id(1)]);
        let base = run(&loan, &[]);
        assert_eq!(s.rows[11].residual, base.rows[11].residual - 150_000);
        assert_eq!(
            s.rows[12].payment,
            installment(s.rows[11].residual, 60_000, 48).unwrap()
        );
        assert_eq!(s.rows.len(), 60);
    }

    #[test]
    fn a_banks_amount_wins_over_a_lower_prepayment_in_the_same_window() {
        let loan = terms(20_000_000, 30_000, 300);
        let s = run(
            &loan,
            &[
                rate_change(
                    1,
                    due(&loan, 24),
                    45_000,
                    LoanInstallment::Contract { amount: 90_000 },
                ),
                prepayment(2, due(&loan, 24), 1_000_000, PrepaymentMode::Lower),
            ],
        );
        assert_eq!(s.rows[23].event_ids, [id(2), id(1)]);
        assert_eq!(s.rows[24].payment, 90_000);
    }

    #[test]
    fn a_prepayment_larger_than_the_debt_is_clipped_and_ends_the_loan() {
        let loan = terms(1_000_000, 75_000, 60);
        let s = run(
            &loan,
            &[
                LoanEventView {
                    id: id(1),
                    event: LoanEvent::Prepayment {
                        date: due(&loan, 10),
                        amount: 2_000_000,
                        mode: PrepaymentMode::Shorten,
                        fee: 5_000,
                    },
                },
                rate_change(
                    2,
                    plus_days(due(&loan, 10), 1),
                    60_000,
                    LoanInstallment::Computed,
                ),
                prepayment(3, due(&loan, 20), 1_000, PrepaymentMode::Lower),
            ],
        );
        let base = run(&loan, &[]);
        assert_eq!(s.rows.len(), 10);
        assert_eq!(s.rows[9].event_ids, [id(1)]);
        assert_eq!(s.total_prepaid, base.rows[9].residual);
        assert_eq!(s.total_penalties, 5_000);
        assert_eq!(
            s.warnings,
            [
                LoanWarning::PrepaymentClipped { event_id: id(1) },
                LoanWarning::EventAfterEnd { event_id: id(2) },
                LoanWarning::EventAfterEnd { event_id: id(3) },
            ]
        );
    }

    #[test]
    fn a_prepayment_of_exactly_the_residual_is_not_clipped() {
        let loan = terms(1_000_000, 75_000, 60);
        let left = run(&loan, &[]).rows[9].residual;
        let s = run(
            &loan,
            &[prepayment(1, due(&loan, 10), left, PrepaymentMode::Lower)],
        );
        assert_eq!(s.rows.len(), 10);
        assert_eq!(s.total_prepaid, left);
        assert!(s.warnings.is_empty());
    }

    #[test]
    fn events_after_the_last_installment_warn() {
        let loan = terms(1_000_000, 75_000, 60);
        let s = run(
            &loan,
            &[
                prepayment(1, due(&loan, 60), 1_000, PrepaymentMode::Shorten),
                rate_change(2, date(2040, 1, 1), 60_000, LoanInstallment::Computed),
            ],
        );
        assert_eq!(s.rows, run(&loan, &[]).rows);
        assert_eq!(s.total_prepaid, 0);
        assert_eq!(
            s.warnings,
            [
                LoanWarning::EventAfterEnd { event_id: id(1) },
                LoanWarning::EventAfterEnd { event_id: id(2) },
            ]
        );
    }

    #[test]
    fn an_event_after_a_shortened_end_warns() {
        let loan = terms(1_000_000, 75_000, 60);
        let s = run(
            &loan,
            &[
                prepayment(1, due(&loan, 12), 500_000, PrepaymentMode::Shorten),
                rate_change(2, due(&loan, 50), 60_000, LoanInstallment::Computed),
            ],
        );
        assert!(s.rows.len() < 50);
        assert_eq!(s.warnings, [LoanWarning::EventAfterEnd { event_id: id(2) }]);
    }

    #[test]
    fn a_rate_change_after_a_shortening_keeps_the_shortened_end() {
        let loan = terms(20_000_000, 30_000, 300);
        let shortened = prepayment(1, due(&loan, 12), 5_000_000, PrepaymentMode::Shorten);
        let end = run(&loan, &[shortened]).rows.len();
        assert!(end < 300);
        let s = run(
            &loan,
            &[
                shortened,
                rate_change(2, due(&loan, 36), 45_000, LoanInstallment::Computed),
            ],
        );
        assert_eq!(s.rows.len(), end);
        let s = run(
            &loan,
            &[
                shortened,
                prepayment(2, due(&loan, 36), 1_000_000, PrepaymentMode::Lower),
            ],
        );
        assert_eq!(s.rows.len(), end);
    }

    #[test]
    fn the_totals_add_up() {
        let mut loan = terms(20_000_000, 30_000, 300);
        loan.upfront_fees = 350_000;
        loan.installment_fee = 150;
        let s = run(
            &loan,
            &[LoanEventView {
                id: id(1),
                event: LoanEvent::Prepayment {
                    date: due(&loan, 60),
                    amount: 2_000_000,
                    mode: PrepaymentMode::Shorten,
                    fee: 20_000,
                },
            }],
        );
        let rows = i64::try_from(s.rows.len()).unwrap();
        assert_eq!(s.total_fees, 150 * rows);
        assert_eq!(s.total_prepaid, 2_000_000);
        assert_eq!(s.total_penalties, 20_000);
        assert_eq!(
            s.total_cost,
            s.total_interest + 350_000 + 150 * rows + 20_000
        );
    }

    #[test]
    fn schedule_refuses_terms_and_events_out_of_bounds() {
        let good = terms(1_000_000, 75_000, 60);
        let mut bad_terms = Vec::new();
        for change in 0..7 {
            let mut loan = good;
            match change {
                0 => loan.principal = 0,
                1 => loan.annual_rate_ppm = 500_001,
                2 => loan.months = 0,
                3 => loan.months = 601,
                4 => loan.installment = LoanInstallment::Contract { amount: 0 },
                5 => loan.upfront_fees = -1,
                _ => loan.installment_fee = -1,
            }
            bad_terms.push(loan);
        }
        for loan in bad_terms {
            assert!(matches!(
                schedule(&loan, &[]),
                Err(DomainError::InvalidCommand(_))
            ));
        }
        let day = due(&good, 3);
        for event in [
            LoanEvent::Prepayment {
                date: day,
                amount: 0,
                mode: PrepaymentMode::Shorten,
                fee: 0,
            },
            LoanEvent::Prepayment {
                date: day,
                amount: 1,
                mode: PrepaymentMode::Shorten,
                fee: -1,
            },
            LoanEvent::RateChange {
                date: day,
                annual_rate_ppm: 500_001,
                installment: LoanInstallment::Computed,
            },
            LoanEvent::RateChange {
                date: day,
                annual_rate_ppm: 30_000,
                installment: LoanInstallment::Contract { amount: -5 },
            },
        ] {
            let events = [LoanEventView { id: id(1), event }];
            assert!(matches!(
                schedule(&good, &events),
                Err(DomainError::InvalidCommand(_))
            ));
        }
    }

    #[test]
    fn solve_rate_finds_the_rate_of_an_installment() {
        // Exact references, the IRR of the constant installments.
        assert_eq!(solve_rate(20_000_000, 94_842, 300).unwrap(), 30_000);
        assert_eq!(solve_rate(1_000_000, 20_038, 60).unwrap(), 75_001);
        assert_eq!(solve_rate(1_000_000, 20_000, 60).unwrap(), 74_201);
        assert_eq!(solve_rate(25_000_000, 150_000, 240).unwrap(), 38_862);
        assert_eq!(solve_rate(100_000, 100_001, 1).unwrap(), 120);
        assert_eq!(solve_rate(MAX_AMOUNT, 60_000_000, 600).unwrap(), 69_779);
        // Installments that add up to the principal: no interest.
        assert_eq!(solve_rate(1_200, 100, 12).unwrap(), 0);
    }

    #[test]
    fn solve_rate_refuses_what_no_rate_explains() {
        for (principal, installment, months) in [
            (1_200, 99, 12),
            (0, 100, 12),
            (1_200, 0, 12),
            (1_200, 100, 0),
            (1_200, 100, 601),
            // Twice the principal in two months: far above 50%.
            (100_000, 100_000, 2),
        ] {
            assert!(matches!(
                solve_rate(principal, installment, months),
                Err(DomainError::InvalidCommand(_))
            ));
        }
    }

    #[test]
    fn tan_from_taeg_undoes_the_monthly_compounding() {
        // Exact references, 12 × ((1 + TAEG)^(1/12) − 1).
        for (taeg, tan) in [
            (0, 0),
            (1, 1),
            (30_416, 30_000),
            (77_633, 75_000),
            (100_000, 95_690),
            (632_000, 499_940),
        ] {
            assert_eq!(tan_from_taeg(taeg).unwrap(), tan, "{taeg}");
        }
        assert!(matches!(
            tan_from_taeg(632_100),
            Err(DomainError::InvalidCommand(_))
        ));
        assert!(tan_from_taeg(u32::MAX).is_err());
    }

    /// A seeded linear congruential generator: the generated cases are the
    /// same at every run.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 11
        }

        fn between(&mut self, low: i64, high: i64) -> i64 {
            let span = u64::try_from(high - low + 1).unwrap();
            low + i64::try_from(self.next() % span).unwrap()
        }

        fn rate(&mut self) -> u32 {
            let ppm = match self.between(0, 4) {
                0 => 0,
                1 => 500_000,
                2 => self.between(5_000, 100_000),
                _ => self.between(1, 500_000),
            };
            u32::try_from(ppm).unwrap()
        }

        fn amount(&mut self, around: i64) -> i64 {
            match self.between(0, 9) {
                0 => MAX_AMOUNT,
                1 => 1,
                2..=4 => self.between(1, around / 10 + 1),
                _ => self.between(around / 2, around * 2).clamp(1, MAX_AMOUNT),
            }
        }
    }

    #[test]
    fn generated_schedules_keep_their_invariants() {
        let mut rng = Lcg(20_261_109);
        for case in 0..400 {
            let principal = match case % 5 {
                0 => MAX_AMOUNT,
                1 => rng.between(1, 1_000),
                2 => rng.between(1, 1_000_000),
                3 => rng.between(1, 1_000_000_000),
                _ => rng.between(1_000_000_000, MAX_AMOUNT),
            };
            let annual_rate_ppm = rng.rate();
            let months = u32::try_from(match rng.between(0, 2) {
                0 => rng.between(1, 12),
                1 => 600,
                _ => rng.between(1, 600),
            })
            .unwrap();
            let year = i32::try_from(rng.between(2000, 2040)).unwrap();
            let month = u32::try_from(rng.between(1, 12)).unwrap();
            let day = u32::try_from(rng.between(1, 31)).unwrap();
            let first_due =
                NaiveDate::from_ymd_opt(year, month, day).unwrap_or_else(|| date(year, month, 28));
            let formula = installment(principal, annual_rate_ppm, months).unwrap();
            let installment = if rng.between(0, 2) == 0 {
                LoanInstallment::Contract {
                    amount: (formula * rng.between(50, 150) / 100).clamp(1, MAX_AMOUNT),
                }
            } else {
                LoanInstallment::Computed
            };
            let fees = |rng: &mut Lcg, around: i64| match rng.between(0, 5) {
                0 => rng.amount(around),
                _ => 0,
            };
            let loan = LoanTerms {
                principal,
                annual_rate_ppm,
                months,
                first_due,
                installment,
                upfront_fees: fees(&mut rng, principal / 50 + 1),
                installment_fee: fees(&mut rng, 500),
            };

            let span = i64::from(months) * 31 + 100;
            let events: Vec<LoanEventView> = (0..rng.between(0, 8))
                .map(|_| {
                    let date = plus_days(first_due, rng.between(-100, span));
                    let id =
                        Uuid::from_u128((u128::from(rng.next()) << 64) | u128::from(rng.next()));
                    let event = if rng.between(0, 1) == 0 {
                        LoanEvent::Prepayment {
                            date,
                            amount: rng.amount(principal),
                            mode: if rng.between(0, 1) == 0 {
                                PrepaymentMode::Shorten
                            } else {
                                PrepaymentMode::Lower
                            },
                            fee: fees(&mut rng, 10_000),
                        }
                    } else {
                        LoanEvent::RateChange {
                            date,
                            annual_rate_ppm: rng.rate(),
                            installment: if rng.between(0, 2) == 0 {
                                LoanInstallment::Contract {
                                    amount: rng.amount(formula),
                                }
                            } else {
                                LoanInstallment::Computed
                            },
                        }
                    };
                    LoanEventView { id, event }
                })
                .collect();

            let s = schedule(&loan, &events)
                .unwrap_or_else(|e| panic!("case {case}: {e} for {loan:?} {events:?}"));
            assert_invariants(&loan, &events, &s);
            // The order the events come in does not matter, and the TAEG
            // is the terms' alone.
            let reversed: Vec<LoanEventView> = events.iter().rev().copied().collect();
            assert_eq!(schedule(&loan, &reversed).unwrap(), s, "case {case}");
            assert_eq!(
                schedule(&loan, &[]).unwrap().taeg_ppm,
                s.taeg_ppm,
                "case {case}"
            );
        }
    }
}
