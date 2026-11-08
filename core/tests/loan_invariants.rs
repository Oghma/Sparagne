//! Invariants of loans and mortgages, written against the contract (the
//! types and doc comments of `loan`, the commands, the schema) before the
//! arithmetic existed: reference values worked out with exact decimals,
//! properties over generated terms and events, the order the events apply
//! in, the totals, what the commands may and may not do, and the link to the
//! recurring template. Everything goes through the public API:
//! `Core::preview_loan` for the math, the commands and the four queries for
//! the engine.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::{Datelike, Days, NaiveDate};
use common::{
    sync::{exec, other_vault, pull, secs, sync, vault_of},
    *,
};
use sparagne_core::{
    Command, Core, DomainError, Frequency, RecurringView, Schedule, TransactionKind,
    TransactionView, VaultSnapshot,
    loan::{
        LinkedInstallment, LoanEvent, LoanEventView, LoanInstallment, LoanOverview, LoanPatch,
        LoanSchedule, LoanScheduleRow, LoanTerms, LoanView, LoanWarning, MAX_MONTHS, MAX_RATE_PPM,
        PrepaymentMode,
    },
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// xorshift64*, seeded once per test: the cases are the same on every run.
struct Gen(u64);

impl Gen {
    const fn new(seed: u64) -> Self {
        Self((seed << 1) | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `0..n`.
    fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "empty range");
        self.next_u64() % n
    }

    /// Uniform in `lo..=hi`.
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        let span = u64::try_from(hi - lo).unwrap() + 1;
        lo + i64::try_from(self.below(span)).unwrap()
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    /// Uniform in `0..len`.
    fn index(&mut self, len: usize) -> usize {
        usize::try_from(self.below(u64::try_from(len).unwrap())).unwrap()
    }

    fn uuid(&mut self) -> Uuid {
        Uuid::from_u64_pair(self.next_u64(), self.next_u64())
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.index(i + 1);
            items.swap(i, j);
        }
    }
}

fn u32_of(value: i64) -> u32 {
    u32::try_from(value).unwrap()
}

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

/// 100 M€ in cents: the most any amount of a loan (principal, fees, a
/// contract installment, a repayment, a penalty) may be. Above it the
/// commands and `preview_loan` refuse; within it, and within the rate and
/// months bounds, nothing of a loan may ever fail.
const MAX_LOAN_AMOUNT: i64 = 10_000_000_000;

fn plus_days(date: NaiveDate, days: i64) -> NaiveDate {
    if days >= 0 {
        date.checked_add_days(Days::new(u64::try_from(days).unwrap()))
            .unwrap()
    } else {
        date.checked_sub_days(Days::new(u64::try_from(-days).unwrap()))
            .unwrap()
    }
}

fn terms(principal: i64, annual_rate_ppm: u32, months: u32, first_due: NaiveDate) -> LoanTerms {
    LoanTerms {
        principal,
        annual_rate_ppm,
        months,
        first_due,
        installment: LoanInstallment::Computed,
        upfront_fees: 0,
        installment_fee: 0,
    }
}

/// 10 000,00 € at 7,5% over 60 months, due on the 15th from January 2026:
/// installment 200,38, last 200,35, interest 2 022,77.
fn ten_k() -> LoanTerms {
    terms(1_000_000, 75_000, 60, day(2026, 1, 15))
}

/// 200 000,00 € at 3% over 300 months: installment 948,42, last 949,60,
/// interest 84 527,18.
fn mortgage() -> LoanTerms {
    terms(20_000_000, 30_000, 300, day(2026, 1, 1))
}

fn contract(amount: i64) -> LoanInstallment {
    LoanInstallment::Contract { amount }
}

fn rate_change(date: NaiveDate, annual_rate_ppm: u32, installment: LoanInstallment) -> LoanEvent {
    LoanEvent::RateChange {
        date,
        annual_rate_ppm,
        installment,
    }
}

fn prepayment(date: NaiveDate, amount: i64, mode: PrepaymentMode, fee: i64) -> LoanEvent {
    LoanEvent::Prepayment {
        date,
        amount,
        mode,
        fee,
    }
}

fn ev(id: Uuid, event: LoanEvent) -> LoanEventView {
    LoanEventView { id, event }
}

fn preview(core: &Core, terms: &LoanTerms, events: &[LoanEventView]) -> LoanSchedule {
    core.preview_loan(terms, events)
        .unwrap_or_else(|e| panic!("preview of {terms:?} with {events:?}: {e}"))
}

fn payments(s: &LoanSchedule) -> Vec<i64> {
    s.rows.iter().map(|r| r.payment).collect()
}

fn last(s: &LoanSchedule) -> &LoanScheduleRow {
    s.rows.last().expect("a schedule with rows")
}

fn balloon(s: &LoanSchedule) -> Option<i64> {
    s.warnings.iter().find_map(|w| match w {
        LoanWarning::Balloon { amount } => Some(*amount),
        _ => None,
    })
}

fn below_interest(s: &LoanSchedule) -> Option<u32> {
    s.warnings.iter().find_map(|w| match w {
        LoanWarning::InstallmentBelowInterest { number } => Some(*number),
        _ => None,
    })
}

fn clipped(s: &LoanSchedule) -> Vec<Uuid> {
    let mut ids: Vec<Uuid> = s
        .warnings
        .iter()
        .filter_map(|w| match w {
            LoanWarning::PrepaymentClipped { event_id } => Some(*event_id),
            _ => None,
        })
        .collect();
    ids.sort();
    ids
}

fn after_end(s: &LoanSchedule) -> Vec<Uuid> {
    let mut ids: Vec<Uuid> = s
        .warnings
        .iter()
        .filter_map(|w| match w {
            LoanWarning::EventAfterEnd { event_id } => Some(*event_id),
            _ => None,
        })
        .collect();
    ids.sort();
    ids
}

// ---------------------------------------------------------------------------
// Reference arithmetic, written again from B2
// ---------------------------------------------------------------------------

/// `residual × ppm / 12 000 000`, rounded half up.
fn interest_ref(residual: i64, annual_rate_ppm: u32) -> i64 {
    let num = i128::from(residual) * i128::from(annual_rate_ppm);
    let den = 12_000_000_i128;
    i64::try_from((2 * num + den) / (2 * den)).unwrap()
}

/// The French installment on the discount factor `v = 1 / (1 + i)` at scale
/// 10^18, `ceil(principal / months)` at a zero rate; within a cent of the
/// exact value, which is all a reference needs (the exact values are pinned
/// by the reference tests).
fn installment_ref(principal: i64, annual_rate_ppm: u32, months: u32) -> i64 {
    let n = i128::from(months);
    if annual_rate_ppm == 0 {
        return i64::try_from((i128::from(principal) + n - 1) / n).unwrap();
    }
    const SCALE: i128 = 1_000_000_000_000_000_000;
    let base = 12_000_000_i128;
    let mut vn = SCALE;
    for _ in 0..months {
        vn = vn * base / (base + i128::from(annual_rate_ppm));
    }
    let numerator = i128::from(principal) * i128::from(annual_rate_ppm) * SCALE;
    let denominator = base * (SCALE - vn);
    i64::try_from((2 * numerator + denominator) / (2 * denominator)).unwrap()
}

/// `(1 + TAN/12)^12 − 1` in millionths, rounded half up: the TAEG of a loan
/// without fees.
fn closed_taeg(annual_rate_ppm: u32) -> u32 {
    const SCALE: i128 = 1_000_000_000_000_000_000;
    let base = 12_000_000_i128;
    let mut grown = SCALE;
    for _ in 0..12 {
        grown = grown * (base + i128::from(annual_rate_ppm)) / base;
    }
    let ppm = (grown - SCALE) * 1_000_000;
    u32::try_from((2 * ppm + SCALE) / (2 * SCALE)).unwrap()
}

/// The nominal due dates: monthly on the first one's day, the month's last
/// day when it is shorter, as the recurring schedule does it.
fn nominal_dues(terms: &LoanTerms) -> Vec<NaiveDate> {
    Schedule {
        frequency: Frequency::Monthly {
            day: u8::try_from(terms.first_due.day()).unwrap(),
        },
        interval: 1,
        start_date: terms.first_due,
        end_date: None,
    }
    .occurrences_from(terms.first_due, usize::try_from(terms.months).unwrap())
    .unwrap()
}

/// What the prepayments do to the debt, replayed from the ids a schedule
/// lists: each one applied in the listed order, cut to the residual.
struct Ledger {
    residual: i64,
    prepaid: i64,
    penalties: i64,
    listed: Vec<Uuid>,
    clipped: Vec<Uuid>,
}

impl Ledger {
    fn prepay(&mut self, id: Uuid, amount: i64, fee: i64) {
        let applied = amount.min(self.residual);
        if amount > self.residual {
            self.clipped.push(id);
        }
        self.residual -= applied;
        self.prepaid += applied;
        self.penalties += fee;
        self.listed.push(id);
    }
}

fn is_prepayment(event: &LoanEvent) -> bool {
    matches!(event, LoanEvent::Prepayment { .. })
}

/// Prepayments before rate changes, each kind by id.
fn assert_applied_in_order(at: &str, ids: &[Uuid], event_of: impl Fn(Uuid) -> LoanEvent) {
    let kinds: Vec<bool> = ids.iter().map(|id| is_prepayment(&event_of(*id))).collect();
    let first_rate = kinds.iter().position(|p| !p).unwrap_or(kinds.len());
    assert!(
        kinds[first_rate..].iter().all(|p| !p),
        "{at}: a prepayment after a rate change in {ids:?}"
    );
    for group in [&ids[..first_rate], &ids[first_rate..]] {
        assert!(
            group.windows(2).all(|w| w[0] < w[1]),
            "{at}: same-day events not by id in {ids:?}"
        );
    }
}

/// What the installment of the next row must be after `row`, worked out from
/// the events the row applied; `None` when the contract leaves it open (a
/// `Lower` and a `Contract` on the same day) or when the row has no next.
fn expected_next_installment(
    row: &LoanScheduleRow,
    rate: u32,
    remaining: u32,
    event_of: impl Fn(Uuid) -> LoanEvent,
) -> Option<(i64, bool)> {
    let mut recompute = false;
    let mut fixed: Option<i64> = None;
    for id in &row.event_ids {
        match event_of(*id) {
            LoanEvent::Prepayment { mode, .. } => {
                if mode == PrepaymentMode::Lower {
                    recompute = true;
                }
            }
            LoanEvent::RateChange { installment, .. } => match installment {
                LoanInstallment::Computed => {
                    recompute = true;
                    fixed = None;
                }
                LoanInstallment::Contract { amount } => {
                    fixed = Some(amount);
                }
            },
        }
    }
    match (fixed, recompute) {
        (Some(_), true) => None,
        (Some(amount), false) => Some((amount, true)),
        (None, true) => Some((installment_ref(row.residual, rate, remaining), false)),
        (None, false) => Some((row.payment, true)),
    }
}

/// Everything the contract says of a schedule, whatever the terms and the
/// events: dates, rates, the interest formula, a residual that never grows
/// nor goes below zero and ends at zero, the sums, the ids listed once and
/// in order, the warnings that follow from what happened.
fn check_schedule(ctx: &str, terms: &LoanTerms, events: &[LoanEventView], s: &LoanSchedule) {
    let dues = nominal_dues(terms);
    let months = usize::try_from(terms.months).unwrap();
    assert!(
        s.rows.len() <= months,
        "{ctx}: {} rows over {months} months",
        s.rows.len()
    );
    let event_of = |id: Uuid| {
        events
            .iter()
            .find(|e| e.id == id)
            .unwrap_or_else(|| panic!("{ctx}: unknown event {id}"))
            .event
    };
    let mut ledger = Ledger {
        residual: terms.principal,
        prepaid: 0,
        penalties: 0,
        listed: Vec::new(),
        clipped: Vec::new(),
    };

    assert_applied_in_order(
        &format!("{ctx} before the first"),
        &s.event_ids_before_first,
        event_of,
    );
    for id in &s.event_ids_before_first {
        match event_of(*id) {
            LoanEvent::Prepayment {
                date, amount, fee, ..
            } => {
                assert!(
                    date < terms.first_due,
                    "{ctx}: {id} dated {date} is not before the first installment"
                );
                ledger.prepay(*id, amount, fee);
            }
            LoanEvent::RateChange { .. } => {
                panic!("{ctx}: a rate change {id} before the first installment")
            }
        }
    }

    let below = below_interest(s);
    let mut rate = terms.annual_rate_ppm;
    let mut expected: Option<(i64, bool)> = match terms.installment {
        LoanInstallment::Contract { amount } => Some((amount, true)),
        LoanInstallment::Computed => {
            let lowered = s.event_ids_before_first.iter().any(|id| {
                matches!(
                    event_of(*id),
                    LoanEvent::Prepayment {
                        mode: PrepaymentMode::Lower,
                        ..
                    }
                )
            });
            let on = if lowered {
                ledger.residual
            } else {
                terms.principal
            };
            Some((installment_ref(on, rate, terms.months), false))
        }
    };
    let mut first_zero_capital = None;
    for (i, row) in s.rows.iter().enumerate() {
        let at = format!("{ctx} row {}", i + 1);
        let is_last = i + 1 == s.rows.len();
        assert_eq!(
            row.number,
            u32_of(i64::try_from(i).unwrap() + 1),
            "{at}: number"
        );
        assert_eq!(row.due_date, dues[i], "{at}: due date");
        assert_eq!(row.annual_rate_ppm, rate, "{at}: rate");
        assert_eq!(row.fee, terms.installment_fee, "{at}: fee");
        assert!(row.interest >= 0, "{at}: negative interest");
        assert!(row.capital >= 0, "{at}: negative capital {}", row.capital);
        assert_eq!(
            row.interest,
            interest_ref(ledger.residual, rate),
            "{at}: interest on {} at {rate}",
            ledger.residual
        );
        assert_eq!(
            row.payment,
            row.interest + row.capital,
            "{at}: payment is interest + capital"
        );
        assert!(
            row.capital <= ledger.residual,
            "{at}: capital {} over the debt {}",
            row.capital,
            ledger.residual
        );
        if !is_last && row.capital == 0 && first_zero_capital.is_none() {
            first_zero_capital = Some(row.number);
        }
        if let Some((installment, exact)) = expected
            && !is_last
            && below.is_none()
        {
            if exact {
                assert_eq!(row.payment, installment, "{at}: the installment");
            } else {
                assert!(
                    (row.payment - installment).abs() <= 1,
                    "{at}: installment {} is not the formula's {installment}",
                    row.payment
                );
            }
        }
        ledger.residual -= row.capital;

        assert_applied_in_order(&at, &row.event_ids, event_of);
        for id in &row.event_ids {
            let event = event_of(*id);
            assert!(
                event.date() >= row.due_date,
                "{at}: {id} dated {} before the due date {}",
                event.date(),
                row.due_date
            );
            if let Some(next) = dues.get(i + 1) {
                assert!(
                    event.date() < *next,
                    "{at}: {id} dated {} is due with the next installment {next}",
                    event.date()
                );
            }
            match event {
                LoanEvent::Prepayment { amount, fee, .. } => ledger.prepay(*id, amount, fee),
                LoanEvent::RateChange {
                    annual_rate_ppm, ..
                } => rate = annual_rate_ppm,
            }
        }
        assert_eq!(row.residual, ledger.residual, "{at}: residual");
        assert!(row.residual >= 0, "{at}: debt below zero");
        if !is_last {
            assert!(row.residual > 0, "{at}: a zero residual ends the schedule");
        }
        let remaining = terms.months - row.number;
        expected = if remaining == 0 {
            None
        } else {
            expected_next_installment(row, rate, remaining, event_of)
        };
    }

    match s.rows.last() {
        Some(row) => assert_eq!(row.residual, 0, "{ctx}: the last row leaves a debt"),
        None => assert_eq!(
            ledger.residual, 0,
            "{ctx}: no rows but a debt of {}",
            ledger.residual
        ),
    }
    let capital: i64 = s.rows.iter().map(|r| r.capital).sum();
    assert_eq!(
        capital + ledger.prepaid,
        terms.principal,
        "{ctx}: capital + prepayments"
    );
    assert_eq!(s.total_prepaid, ledger.prepaid, "{ctx}: total_prepaid");
    assert_eq!(
        s.total_penalties, ledger.penalties,
        "{ctx}: total_penalties"
    );
    assert_eq!(
        s.total_interest,
        s.rows.iter().map(|r| r.interest).sum::<i64>(),
        "{ctx}: total_interest"
    );
    assert_eq!(
        s.total_fees,
        s.rows.iter().map(|r| r.fee).sum::<i64>(),
        "{ctx}: total_fees"
    );
    let paid: i64 = s.rows.iter().map(|r| r.payment).sum();
    assert_eq!(
        s.total_cost,
        paid + terms.upfront_fees + s.total_fees + s.total_prepaid + s.total_penalties
            - terms.principal,
        "{ctx}: total_cost"
    );
    if terms.annual_rate_ppm == 0
        && events
            .iter()
            .all(|e| matches!(e.event, LoanEvent::Prepayment { .. }))
    {
        assert_eq!(s.total_interest, 0, "{ctx}: interest at a zero rate");
    }

    // Every event is applied once or flagged as after the end, by its date.
    let mut listed = ledger.listed.clone();
    let listed_count = listed.len();
    listed.sort();
    listed.dedup();
    assert_eq!(listed.len(), listed_count, "{ctx}: an event listed twice");
    let flagged = after_end(s);
    let end = s.rows.last().map(|r| r.due_date);
    for e in events {
        let is_listed = listed.contains(&e.id);
        let is_flagged = flagged.contains(&e.id);
        assert!(
            is_listed || is_flagged,
            "{ctx}: {} neither applied nor flagged",
            e.id
        );
        assert!(
            !(is_listed && is_flagged),
            "{ctx}: {} both applied and after the end",
            e.id
        );
        match end {
            Some(end) if e.event.date() < end => {
                assert!(
                    is_listed,
                    "{ctx}: {} dated before the end is not applied",
                    e.id
                );
            }
            Some(end) if e.event.date() > end => {
                assert!(
                    is_flagged,
                    "{ctx}: {} dated after the end is not flagged",
                    e.id
                );
            }
            None if e.event.date() >= terms.first_due => {
                assert!(
                    is_flagged,
                    "{ctx}: {} after a loan paid off before it began",
                    e.id
                );
            }
            _ => {}
        }
    }
    for id in &flagged {
        assert!(
            events.iter().any(|e| e.id == *id),
            "{ctx}: flagged unknown {id}"
        );
    }
    let mut expected_clipped = ledger.clipped.clone();
    expected_clipped.sort();
    assert_eq!(
        clipped(s),
        expected_clipped,
        "{ctx}: the clipped prepayments"
    );
    // The flag names the first row whose installment does not cover the
    // interest: every such row before the last one repays no capital. The
    // last row is left open, since it pays what is left whatever the
    // installment was.
    match first_zero_capital {
        Some(number) => assert_eq!(
            below,
            Some(number),
            "{ctx}: InstallmentBelowInterest against the first row with no capital"
        ),
        None => assert!(
            below.is_none() || below == s.rows.last().map(|r| r.number),
            "{ctx}: InstallmentBelowInterest {below:?} with capital in every row"
        ),
    }
    for w in &s.warnings {
        if let LoanWarning::PrepaymentClipped { event_id } = w {
            assert!(
                is_prepayment(&event_of(*event_id)),
                "{ctx}: clipped a rate change"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Reference values
// ---------------------------------------------------------------------------

#[test]
fn the_reference_arithmetic_of_this_file_agrees_with_the_plans_values() {
    assert_eq!(installment_ref(20_000_000, 30_000, 300), 94_842);
    assert_eq!(installment_ref(1_000_000, 75_000, 60), 20_038);
    assert_eq!(installment_ref(100, 0, 3), 34);
    assert_eq!(interest_ref(20_000_000, 30_000), 50_000);
    assert_eq!(interest_ref(1_000_000, 75_000), 6_250);
    assert_eq!(interest_ref(100, 60_000), 1, "half a cent rounds up");
    assert_eq!(interest_ref(100, 59_999), 0);
    assert_eq!(closed_taeg(75_000), 77_633);
    assert_eq!(closed_taeg(30_000), 30_416);
    assert_eq!(closed_taeg(0), 0);
}

#[test]
fn two_hundred_thousand_at_three_percent_over_three_hundred_months_is_the_plans_mortgage() {
    let core = Core::open_in_memory().unwrap();
    let t = mortgage();
    let s = preview(&core, &t, &[]);
    check_schedule("mortgage", &t, &[], &s);

    assert_eq!(s.rows.len(), 300);
    assert!(payments(&s)[..299].iter().all(|p| *p == 94_842));
    assert_eq!(last(&s).payment, 94_960);
    assert_eq!(s.total_interest, 8_452_718);
    assert_eq!(s.total_cost, 8_452_718, "no fees: the cost is the interest");
    assert_eq!(s.total_fees, 0);
    assert_eq!(s.total_prepaid, 0);
    assert_eq!(s.total_penalties, 0);
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    assert!(s.event_ids_before_first.is_empty());

    let first = &s.rows[0];
    assert_eq!(
        (first.number, first.interest, first.capital, first.residual),
        (1, 50_000, 44_842, 19_955_158)
    );
    assert_eq!((s.rows[1].interest, s.rows[1].capital), (49_888, 44_954));
    assert_eq!(s.rows[298].residual, 94_723);
    let end = last(&s);
    assert_eq!((end.interest, end.capital, end.residual), (237, 94_723, 0));
    assert_eq!(end.due_date, day(2050, 12, 1));
    assert!(s.rows.iter().all(|r| r.annual_rate_ppm == 30_000));

    // (1 + 0,03/12)^12 − 1 = 3,0416%: within a millionth of the solver.
    let taeg = s.taeg_ppm.expect("a TAEG");
    assert!((30_415..=30_417).contains(&taeg), "taeg {taeg}");
}

#[test]
fn ten_thousand_at_seven_and_a_half_percent_over_sixty_months_is_the_plans_loan() {
    let core = Core::open_in_memory().unwrap();
    let t = ten_k();
    let s = preview(&core, &t, &[]);
    check_schedule("ten_k", &t, &[], &s);

    assert_eq!(s.rows.len(), 60);
    assert!(payments(&s)[..59].iter().all(|p| *p == 20_038));
    assert_eq!(last(&s).payment, 20_035);
    assert_eq!(s.total_interest, 202_277);
    assert_eq!(s.total_cost, 202_277);
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    let rows: Vec<(i64, i64, i64)> = s
        .rows
        .iter()
        .map(|r| (r.interest, r.capital, r.residual))
        .collect();
    assert_eq!(rows[0], (6_250, 13_788, 986_212));
    assert_eq!(rows[1], (6_164, 13_874, 972_338));
    assert_eq!(rows[2], (6_077, 13_961, 958_377));
    assert_eq!(rows[58], (248, 19_790, 19_911));
    assert_eq!(rows[59], (124, 19_911, 0));
    assert_eq!(s.rows[0].due_date, day(2026, 1, 15));
    assert_eq!(last(&s).due_date, day(2030, 12, 15));

    // 7,763%: the closed form gives 77 632,6 ppm and the solver on the
    // rounded installments 77 632,8; both round to 77 633, and a bisection
    // may stop on either neighbour, hence one millionth of tolerance (the
    // UI shows three decimals of a percent, ten millionths).
    let taeg = s.taeg_ppm.expect("a TAEG");
    assert!((77_632..=77_634).contains(&taeg), "taeg {taeg}");
}

#[test]
fn the_interest_of_every_installment_is_the_residual_times_the_rate_over_twelve_rounded_half_up() {
    let core = Core::open_in_memory().unwrap();
    // 1,00 € at 6% for a month: 0,5 cents of interest round up to one.
    let up = preview(&core, &terms(100, 60_000, 1, day(2026, 3, 31)), &[]);
    assert_eq!(up.rows.len(), 1);
    assert_eq!((up.rows[0].interest, up.rows[0].payment), (1, 101));
    // A hair below: down to zero.
    let down = preview(&core, &terms(100, 59_999, 1, day(2026, 3, 31)), &[]);
    assert_eq!((down.rows[0].interest, down.rows[0].payment), (0, 100));
    // 1,00 € at 50% for a month: 4,17 cents → 4.
    let max = preview(&core, &terms(100, MAX_RATE_PPM, 1, day(2026, 3, 31)), &[]);
    assert_eq!((max.rows[0].interest, max.rows[0].payment), (4, 104));
    assert_eq!(max.total_cost, 4);

    // Over a whole schedule, on the residual the previous row left.
    let s = preview(&core, &ten_k(), &[]);
    let mut residual = 1_000_000;
    for row in &s.rows {
        assert_eq!(
            row.interest,
            interest_ref(residual, 75_000),
            "row {}",
            row.number
        );
        residual = row.residual;
    }
}

#[test]
fn a_zero_rate_splits_the_principal_in_equal_installments_rounded_up_with_no_interest() {
    let core = Core::open_in_memory().unwrap();
    let t = terms(100, 0, 3, day(2026, 1, 1));
    let s = preview(&core, &t, &[]);
    check_schedule("zero rate", &t, &[], &s);
    assert_eq!(payments(&s), vec![34, 34, 32]);
    assert!(s.rows.iter().all(|r| r.interest == 0));
    assert_eq!(s.total_interest, 0);
    assert_eq!(s.total_cost, 0);
    assert_eq!(
        s.taeg_ppm,
        Some(0),
        "nothing is paid on top: a TAEG of zero"
    );
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);

    let exact = preview(&core, &terms(1_200_000, 0, 12, day(2026, 1, 1)), &[]);
    assert!(payments(&exact).iter().all(|p| *p == 100_000));
}

#[test]
fn due_dates_fall_on_the_same_day_of_each_month_or_on_its_last_day() {
    let core = Core::open_in_memory().unwrap();
    let s = preview(&core, &terms(100_000, 20_000, 5, day(2027, 12, 31)), &[]);
    let dues: Vec<NaiveDate> = s.rows.iter().map(|r| r.due_date).collect();
    assert_eq!(
        dues,
        vec![
            day(2027, 12, 31),
            day(2028, 1, 31),
            day(2028, 2, 29),
            day(2028, 3, 31),
            day(2028, 4, 30),
        ],
        "the day is taken again from the first due date every month, it never drifts"
    );
    let mid = preview(&core, &terms(100_000, 20_000, 3, day(2026, 1, 30)), &[]);
    let dues: Vec<NaiveDate> = mid.rows.iter().map(|r| r.due_date).collect();
    assert_eq!(
        dues,
        vec![day(2026, 1, 30), day(2026, 2, 28), day(2026, 3, 30)]
    );
}

#[test]
fn fees_enter_the_cost_the_rows_and_the_taeg_but_never_the_installment() {
    let core = Core::open_in_memory().unwrap();
    let bare = ten_k();
    let mut with = bare;
    with.upfront_fees = 10_000;
    with.installment_fee = 200;
    let plain = preview(&core, &bare, &[]);
    let fees = preview(&core, &with, &[]);
    check_schedule("fees", &with, &[], &fees);

    assert_eq!(
        payments(&fees),
        payments(&plain),
        "the installment is the same"
    );
    assert!(fees.rows.iter().all(|r| r.fee == 200));
    assert_eq!(fees.total_fees, 60 * 200);
    assert_eq!(fees.total_interest, plain.total_interest);
    assert_eq!(fees.total_cost, 202_277 + 10_000 + 12_000);
    // 1,00 € upfront and 2,00 € a row push 7,763% to 8,673%.
    let taeg = fees.taeg_ppm.expect("a TAEG");
    assert!((86_727..=86_729).contains(&taeg), "taeg {taeg}");
    assert!(taeg > plain.taeg_ppm.unwrap());

    let mut mortgage_fees = mortgage();
    mortgage_fees.upfront_fees = 15_000;
    mortgage_fees.installment_fee = 350;
    let m = preview(&core, &mortgage_fees, &[]);
    assert_eq!(m.total_cost, 8_572_718);
}

// ---------------------------------------------------------------------------
// Properties over generated cases
// ---------------------------------------------------------------------------

fn gen_rate(g: &mut Gen) -> u32 {
    if g.chance(10) {
        0
    } else {
        u32_of(g.range(1, i64::from(MAX_RATE_PPM)))
    }
}

fn gen_date(g: &mut Gen) -> NaiveDate {
    let y = i32::try_from(g.range(2020, 2030)).unwrap();
    let m = u32_of(g.range(1, 12));
    let d = u32_of(g.range(1, 31));
    (1..=d)
        .rev()
        .find_map(|d| NaiveDate::from_ymd_opt(y, m, d))
        .unwrap()
}

/// Terms in the bounds the commands accept, every size of loan.
fn gen_terms(g: &mut Gen) -> LoanTerms {
    let principal = match g.below(10) {
        0 => g.range(1, 1_000),
        1..=3 => g.range(1_000, 1_000_000),
        4..=8 => g.range(1_000_000, 100_000_000),
        _ => g.range(100_000_000, MAX_LOAN_AMOUNT),
    };
    let upfront_fees = match g.below(10) {
        0..=5 => 0,
        6..=8 => g.range(1, (principal / 10).max(1)),
        _ => g.range(1, MAX_LOAN_AMOUNT),
    };
    let installment_fee = match g.below(10) {
        0..=5 => 0,
        6..=8 => g.range(1, 1_000),
        _ => g.range(1, MAX_LOAN_AMOUNT),
    };
    let months = u32_of(match g.below(4) {
        0 => g.range(1, 12),
        1 => g.range(12, 60),
        2 => g.range(60, 360),
        _ => g.range(360, i64::from(MAX_MONTHS)),
    });
    LoanTerms {
        principal,
        annual_rate_ppm: gen_rate(g),
        months,
        first_due: gen_date(g),
        installment: LoanInstallment::Computed,
        upfront_fees,
        installment_fee,
    }
}

/// Whether the TAEG of `terms` is bound to exist: no fees, or fees small
/// enough next to the principal that the yearly rate stays far from what
/// `u32` millionths can hold (with the fees over the principal there is no
/// rate at all, and the contract allows `None`).
fn taeg_is_bounded(terms: &LoanTerms) -> bool {
    let fees = terms.upfront_fees + terms.installment_fee * i64::from(terms.months);
    fees == 0 || (terms.months >= 12 && fees <= terms.principal / 10)
}

/// A date on a nominal due date or in the days after it, never past the
/// last one.
fn window_date(g: &mut Gen, dues: &[NaiveDate]) -> NaiveDate {
    let k = g.index(dues.len());
    plus_days(dues[k], g.range(0, 27)).min(*dues.last().unwrap())
}

/// Up to three events: rate changes inside the nominal window, prepayments
/// inside it or (once) before the first installment, now and then larger
/// than the whole debt.
fn gen_events(g: &mut Gen, terms: &LoanTerms, formula: i64) -> Vec<LoanEventView> {
    let dues = nominal_dues(terms);
    let n = g.index(4);
    let mut before_first = false;
    (0..n)
        .map(|_| {
            let id = g.uuid();
            let event = if g.chance(45) {
                let installment = if g.chance(70) {
                    LoanInstallment::Computed
                } else {
                    contract(g.range(1, (2 * formula.max(1)).min(MAX_LOAN_AMOUNT)))
                };
                rate_change(window_date(g, &dues), gen_rate(g), installment)
            } else {
                let date = if !before_first && g.chance(20) {
                    before_first = true;
                    plus_days(terms.first_due, -g.range(1, 60))
                } else {
                    window_date(g, &dues)
                };
                let amount = if g.chance(10) {
                    (terms.principal + g.range(1, 1_000)).min(MAX_LOAN_AMOUNT)
                } else {
                    g.range(1, terms.principal)
                };
                let mode = if g.chance(50) {
                    PrepaymentMode::Shorten
                } else {
                    PrepaymentMode::Lower
                };
                let fee = match g.below(10) {
                    0..=4 => 0,
                    5..=8 => g.range(1, 50_000),
                    _ => g.range(1, MAX_LOAN_AMOUNT),
                };
                prepayment(date, amount, mode, fee)
            };
            ev(id, event)
        })
        .collect()
}

#[test]
fn every_generated_loan_without_events_satisfies_the_contract() {
    let core = Core::open_in_memory().unwrap();
    let mut g = Gen::new(1);
    for case in 0..200 {
        let t = gen_terms(&mut g);
        let s = preview(&core, &t, &[]);
        let ctx = format!("case {case} {t:?}");
        check_schedule(&ctx, &t, &[], &s);
        assert!(!s.rows.is_empty(), "{ctx}: no rows");
        assert_eq!(preview(&core, &t, &[]), s, "{ctx}: deterministic");
        if below_interest(&s).is_none() && taeg_is_bounded(&t) {
            assert!(s.taeg_ppm.is_some(), "{ctx}: no TAEG");
            if t.upfront_fees == 0 && t.installment_fee == 0 {
                assert!(
                    s.taeg_ppm.unwrap() >= closed_taeg(t.annual_rate_ppm).saturating_sub(2),
                    "{ctx}: taeg {:?} under the TAN's yearly equivalent",
                    s.taeg_ppm
                );
            }
        }
    }
}

#[test]
fn every_generated_loan_with_events_satisfies_the_contract_and_keeps_the_taeg_of_its_terms() {
    let core = Core::open_in_memory().unwrap();
    let mut g = Gen::new(2);
    let mut with_events = 0;
    let mut clipped_seen = 0;
    let mut after_end_seen = 0;
    for case in 0..300 {
        let mut t = gen_terms(&mut g);
        let plain = preview(&core, &t, &[]);
        let formula = plain.rows[0].payment;
        if g.chance(20) && t.months > 1 {
            t.installment = contract(g.range(1, (2 * formula.max(1)).min(MAX_LOAN_AMOUNT)));
        }
        let events = gen_events(&mut g, &t, formula);
        if events.is_empty() {
            continue;
        }
        with_events += 1;
        let s = preview(&core, &t, &events);
        let ctx = format!("case {case} {t:?} {events:?}");
        check_schedule(&ctx, &t, &events, &s);
        clipped_seen += clipped(&s).len();
        after_end_seen += after_end(&s).len();
        assert_eq!(
            s.taeg_ppm,
            preview(&core, &t, &[]).taeg_ppm,
            "{ctx}: events changed the TAEG"
        );

        // The schedule reads the set of events, not the slice's order.
        let mut shuffled = events.clone();
        g.shuffle(&mut shuffled);
        assert_eq!(
            preview(&core, &t, &shuffled),
            s,
            "{ctx}: the order of the slice"
        );
    }
    assert!(with_events >= 200, "only {with_events} cases had events");
    assert!(
        clipped_seen >= 10,
        "only {clipped_seen} clipped prepayments seen"
    );
    assert!(
        after_end_seen >= 5,
        "only {after_end_seen} events after the end seen"
    );
}

#[test]
fn a_contract_installment_above_the_formula_ends_early_and_below_it_leaves_a_balloon() {
    let core = Core::open_in_memory().unwrap();
    let mut g = Gen::new(3);
    let mut checked = 0;
    for case in 0..150 {
        let mut t = gen_terms(&mut g);
        t.principal = t.principal.max(1_000_000);
        t.months = t.months.max(2);
        let plain = preview(&core, &t, &[]);
        let formula = plain.rows[0].payment;
        let first_interest = plain.rows[0].interest;
        if formula - first_interest < 4 || formula < 100 {
            continue;
        }
        checked += 1;
        let ctx = format!("case {case} {t:?} formula {formula}");

        let mut same = t;
        same.installment = contract(formula);
        let as_formula = preview(&core, &same, &[]);
        assert_eq!(
            as_formula, plain,
            "{ctx}: the formula's amount as a contract is the computed schedule"
        );

        let mut above = t;
        let up = (formula + g.range((formula / 50).max(1), formula)).min(MAX_LOAN_AMOUNT);
        if up <= formula {
            continue;
        }
        above.installment = contract(up);
        let early = preview(&core, &above, &[]);
        check_schedule(&format!("{ctx} above {up}"), &above, &[], &early);
        assert!(
            early.rows.len() < plain.rows.len(),
            "{ctx}: {up} over {formula} does not end early ({} rows)",
            early.rows.len()
        );
        assert!(last(&early).payment <= up, "{ctx}: the last row is smaller");
        assert_eq!(balloon(&early), None, "{ctx}: no balloon above the formula");
        assert_eq!(below_interest(&early), None, "{ctx}");
        assert!(
            early.total_interest < plain.total_interest,
            "{ctx}: paying more each month costs less interest"
        );

        let mut under = t;
        let step = (formula / 100).max(1);
        let down = formula - g.range(step, (formula - first_interest - 1).max(step));
        if down <= first_interest {
            continue;
        }
        under.installment = contract(down);
        let ballooned = preview(&core, &under, &[]);
        check_schedule(&format!("{ctx} below {down}"), &under, &[], &ballooned);
        assert_eq!(
            ballooned.rows.len(),
            usize::try_from(t.months).unwrap(),
            "{ctx}: below the formula the loan runs to the end"
        );
        let end = last(&ballooned);
        assert!(end.payment > down, "{ctx}: the last row carries the rest");
        assert_eq!(
            balloon(&ballooned),
            Some(end.payment - down),
            "{ctx}: the balloon is what the last row pays beyond the installment"
        );
        assert_eq!(below_interest(&ballooned), None, "{ctx}");
        assert!(
            ballooned.total_interest > plain.total_interest,
            "{ctx}: paying less each month costs more interest"
        );
    }
    assert!(checked >= 100, "only {checked} cases were checked");
}

#[test]
fn an_installment_that_does_not_cover_the_interest_is_flagged_and_the_schedule_still_ends() {
    let core = Core::open_in_memory().unwrap();
    let mut g = Gen::new(4);
    let mut flagged = 0;
    for case in 0..100 {
        let mut t = gen_terms(&mut g);
        t.principal = t.principal.max(100_000);
        t.annual_rate_ppm = t.annual_rate_ppm.max(10_000);
        t.months = t.months.max(2);
        let first_interest = interest_ref(t.principal, t.annual_rate_ppm);
        if first_interest == 0 {
            continue;
        }
        t.installment = contract(g.range(1, first_interest));
        let s = preview(&core, &t, &[]);
        let ctx = format!("case {case} {t:?}");
        check_schedule(&ctx, &t, &[], &s);
        flagged += 1;
        assert_eq!(
            below_interest(&s),
            Some(1),
            "{ctx}: the first installment is below the interest"
        );
        assert_eq!(s.rows.len(), usize::try_from(t.months).unwrap(), "{ctx}");
        let mut residual = t.principal;
        for row in &s.rows {
            assert!(
                row.residual <= residual,
                "{ctx}: the debt grew at row {}",
                row.number
            );
            residual = row.residual;
        }
        assert_eq!(last(&s).residual, 0, "{ctx}");
    }
    assert!(flagged >= 80, "only {flagged} cases flagged");

    // The plan's loan with 62,50 € a month, exactly the first interest.
    let mut t = ten_k();
    t.installment = contract(6_250);
    let s = preview(&core, &t, &[]);
    check_schedule("62,50", &t, &[], &s);
    assert_eq!(below_interest(&s), Some(1));
    assert_eq!(s.rows.len(), 60);
    assert_eq!(s.rows[0].capital, 0);
    assert_eq!(s.rows[0].interest, 6_250);
    assert_eq!(s.rows[58].residual, 1_000_000);
    assert_eq!(last(&s).payment, 1_006_250);
    assert_eq!(s.total_interest, 375_000);
}

#[test]
fn the_biggest_and_smallest_loans_the_commands_accept_never_fail_or_overflow() {
    let core = Core::open_in_memory().unwrap();
    let first = day(2026, 1, 31);
    let cases = [
        terms(1_000_000_000, MAX_RATE_PPM, MAX_MONTHS, first),
        terms(1_000_000_000, 1, MAX_MONTHS, first),
        terms(1_000_000_000, MAX_RATE_PPM, 1, first),
        terms(1, MAX_RATE_PPM, MAX_MONTHS, first),
        terms(1, 0, MAX_MONTHS, first),
        terms(1, 1, 1, first),
        terms(MAX_LOAN_AMOUNT, MAX_RATE_PPM, MAX_MONTHS, first),
        terms(MAX_LOAN_AMOUNT, MAX_RATE_PPM, 1, first),
        terms(MAX_LOAN_AMOUNT, 0, MAX_MONTHS, first),
    ];
    for t in cases {
        let s = preview(&core, &t, &[]);
        check_schedule(&format!("{t:?}"), &t, &[], &s);
        assert!(s.total_cost >= 0, "{t:?}");
    }
    // 100 M€ at 50% for a month: 4 166 666,67 € of interest on top.
    let month = preview(&core, &terms(MAX_LOAN_AMOUNT, MAX_RATE_PPM, 1, first), &[]);
    assert_eq!(month.rows[0].payment, MAX_LOAN_AMOUNT + 416_666_667);
    let s = preview(
        &core,
        &terms(1_000_000_000, MAX_RATE_PPM, MAX_MONTHS, first),
        &[],
    );
    assert_eq!(s.rows.len(), 600);
    assert_eq!(
        s.rows[0].interest, 41_666_667,
        "10 M€ at 50%: 416 666,67 € a month"
    );

    // A whole cent over fifty years at 50%: no installment, no interest, one
    // cent in the last row, and the flag that nothing is being paid.
    let penny = preview(&core, &terms(1, MAX_RATE_PPM, MAX_MONTHS, first), &[]);
    assert_eq!(penny.rows.len(), 600);
    assert_eq!(last(&penny).payment, 1);
    assert_eq!(penny.total_interest, 0);
    assert_eq!(below_interest(&penny), Some(1));
}

#[test]
fn a_preview_of_terms_the_commands_would_refuse_is_an_error_never_a_panic() {
    let core = Core::open_in_memory().unwrap();
    let first = day(2026, 1, 1);
    let bad = [
        terms(0, 30_000, 12, first),
        terms(-1, 30_000, 12, first),
        terms(100_000, 30_000, 0, first),
        terms(100_000, 30_000, MAX_MONTHS + 1, first),
        terms(100_000, MAX_RATE_PPM + 1, 12, first),
    ];
    for t in bad {
        assert!(core.preview_loan(&t, &[]).is_err(), "{t:?} previewed");
    }
    let mut fees = terms(100_000, 30_000, 12, first);
    fees.upfront_fees = -1;
    assert!(
        core.preview_loan(&fees, &[]).is_err(),
        "negative upfront fees"
    );
    let mut fee = terms(100_000, 30_000, 12, first);
    fee.installment_fee = -1;
    assert!(
        core.preview_loan(&fee, &[]).is_err(),
        "a negative installment fee"
    );
    let mut zero = terms(100_000, 30_000, 12, first);
    zero.installment = contract(0);
    assert!(
        core.preview_loan(&zero, &[]).is_err(),
        "a contract of nothing"
    );

    // Over 100 M€, anywhere: refused as an amount, never an overflow.
    let over = MAX_LOAN_AMOUNT + 1;
    let amount_errors = [
        ("the principal", terms(over, 30_000, 12, first)),
        (
            "the biggest i64",
            terms(i64::MAX, MAX_RATE_PPM, MAX_MONTHS, first),
        ),
    ];
    for (what, t) in amount_errors {
        assert_eq!(
            core.preview_loan(&t, &[]).map(|_| ()).unwrap_err().code(),
            "invalid_amount",
            "{what}"
        );
    }
    let mut upfront = ten_k();
    upfront.upfront_fees = over;
    let mut per_row = ten_k();
    per_row.installment_fee = over;
    let mut letter = ten_k();
    letter.installment = contract(over);
    for (what, t) in [
        ("upfront fees", upfront),
        ("an installment fee", per_row),
        ("a contract", letter),
    ] {
        assert_eq!(
            core.preview_loan(&t, &[]).map(|_| ()).unwrap_err().code(),
            "invalid_amount",
            "{what} over the bound"
        );
    }
    let events_over = [
        (
            "a repayment",
            prepayment(day(2026, 3, 20), over, PrepaymentMode::Shorten, 0),
        ),
        (
            "a penalty",
            prepayment(day(2026, 3, 20), 100, PrepaymentMode::Shorten, over),
        ),
        (
            "a letter",
            rate_change(day(2026, 3, 20), 50_000, contract(over)),
        ),
        (
            "the biggest i64",
            prepayment(first, i64::MAX, PrepaymentMode::Shorten, i64::MAX),
        ),
    ];
    for (what, event) in events_over {
        assert_eq!(
            core.preview_loan(&ten_k(), &[ev(id(1), event)])
                .map(|_| ())
                .unwrap_err()
                .code(),
            "invalid_amount",
            "{what} over the bound"
        );
    }
    // A rate over the bound in an event is refused too.
    assert!(
        core.preview_loan(
            &ten_k(),
            &[ev(
                id(1),
                rate_change(
                    day(2026, 3, 20),
                    MAX_RATE_PPM + 1,
                    LoanInstallment::Computed
                )
            )]
        )
        .is_err()
    );
}

#[test]
fn terms_and_events_at_the_upper_bounds_always_give_a_schedule() {
    let core = Core::open_in_memory().unwrap();
    let mut g = Gen::new(8);
    let at_bound = |g: &mut Gen| {
        if g.chance(50) {
            MAX_LOAN_AMOUNT
        } else {
            g.range(1, MAX_LOAN_AMOUNT)
        }
    };
    for case in 0..80 {
        let mut t = gen_terms(&mut g);
        t.principal = at_bound(&mut g);
        if g.chance(50) {
            t.upfront_fees = at_bound(&mut g);
        }
        if g.chance(50) {
            t.installment_fee = at_bound(&mut g);
        }
        if g.chance(50) {
            t.annual_rate_ppm = MAX_RATE_PPM;
        }
        if g.chance(50) {
            t.months = MAX_MONTHS;
        }
        if g.chance(30) {
            t.installment = contract(at_bound(&mut g));
        }
        let dues = nominal_dues(&t);
        let n = g.index(4);
        let events: Vec<LoanEventView> = (0..n)
            .map(|_| {
                let date = if g.chance(20) {
                    plus_days(t.first_due, -g.range(1, 60))
                } else {
                    window_date(&mut g, &dues)
                };
                let event = if g.chance(50) {
                    prepayment(
                        date,
                        at_bound(&mut g),
                        if g.chance(50) {
                            PrepaymentMode::Shorten
                        } else {
                            PrepaymentMode::Lower
                        },
                        at_bound(&mut g),
                    )
                } else {
                    let date = date.max(t.first_due);
                    let installment = if g.chance(50) {
                        LoanInstallment::Computed
                    } else {
                        contract(at_bound(&mut g))
                    };
                    rate_change(
                        date,
                        if g.chance(50) {
                            MAX_RATE_PPM
                        } else {
                            gen_rate(&mut g)
                        },
                        installment,
                    )
                };
                ev(g.uuid(), event)
            })
            .collect();
        let ctx = format!("case {case} {t:?} {events:?}");
        let s = core
            .preview_loan(&t, &events)
            .unwrap_or_else(|e| panic!("{ctx}: {e}"));
        check_schedule(&ctx, &t, &events, &s);
    }
}

// ---------------------------------------------------------------------------
// Event semantics
// ---------------------------------------------------------------------------

#[test]
fn a_rate_change_applies_to_the_installments_due_after_its_date() {
    let core = Core::open_in_memory().unwrap();
    let t = ten_k();
    // On the sixth due date: the sixth installment accrued at 7,5%, the
    // seventh is worked out on the residual 9 159,69 at 5% over 54 months.
    let on_due = [ev(
        id(1),
        rate_change(day(2026, 6, 15), 50_000, LoanInstallment::Computed),
    )];
    let s = preview(&core, &t, &on_due);
    check_schedule("rate change on a due date", &t, &on_due, &s);
    assert_eq!(s.rows.len(), 60);
    assert!(s.rows[..6].iter().all(|r| r.annual_rate_ppm == 75_000));
    assert!(s.rows[6..].iter().all(|r| r.annual_rate_ppm == 50_000));
    let sixth = &s.rows[5];
    assert_eq!(
        (sixth.payment, sixth.interest, sixth.residual),
        (20_038, 5_814, 915_969)
    );
    assert_eq!(sixth.event_ids, vec![id(1)]);
    let seventh = &s.rows[6];
    assert_eq!((seventh.payment, seventh.interest), (18_977, 3_817));
    assert!(payments(&s)[6..59].iter().all(|p| *p == 18_977));
    assert_eq!(last(&s).payment, 19_000);
    assert_eq!(s.total_interest, 145_009);
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);

    // The day before: the sixth installment is already at the new rate.
    let day_before = [ev(
        id(1),
        rate_change(day(2026, 6, 14), 50_000, LoanInstallment::Computed),
    )];
    let s = preview(&core, &t, &day_before);
    check_schedule("rate change the day before", &t, &day_before, &s);
    assert!(s.rows[..5].iter().all(|r| r.annual_rate_ppm == 75_000));
    assert!(s.rows[5..].iter().all(|r| r.annual_rate_ppm == 50_000));
    assert_eq!(s.rows[4].event_ids, vec![id(1)]);
    assert_eq!((s.rows[5].payment, s.rows[5].interest), (18_960, 3_876));
    assert_eq!(s.total_interest, 142_963);

    // The TAEG is the terms': the same with and without the change.
    assert_eq!(s.taeg_ppm, preview(&core, &t, &[]).taeg_ppm);
}

#[test]
fn a_rate_change_may_carry_the_installment_of_the_banks_letter() {
    let core = Core::open_in_memory().unwrap();
    let t = ten_k();
    let letter = [ev(
        id(1),
        rate_change(day(2026, 6, 15), 50_000, contract(25_000)),
    )];
    let s = preview(&core, &t, &letter);
    check_schedule("letter", &t, &letter, &s);
    assert_eq!(s.rows[6].payment, 25_000);
    assert_eq!(s.rows[6].interest, 3_817);
    assert_eq!(s.rows.len(), 46, "250,00 € a month pays it off sooner");
    assert_eq!(last(&s).payment, 21_008);
    assert_eq!(s.total_interest, 116_236);
    assert_eq!(balloon(&s), None);

    // A letter below the new interest: a warning from that row, not a
    // refusal, and the debt does not grow.
    let starving = [ev(
        id(1),
        rate_change(day(2026, 6, 15), 50_000, contract(3_000)),
    )];
    let s = preview(&core, &t, &starving);
    check_schedule("starving", &t, &starving, &s);
    assert_eq!(below_interest(&s), Some(7));
    assert_eq!(s.rows.len(), 60);
    assert!(s.rows[6..59].iter().all(|r| r.capital == 0));
}

#[test]
fn a_prepayment_applies_after_the_last_installment_due_on_or_before_its_date() {
    let core = Core::open_in_memory().unwrap();
    let t = ten_k();
    // 2 000,00 € on March 20th: after the third installment (March 15th),
    // before the fourth.
    for date in [day(2026, 3, 15), day(2026, 3, 20), day(2026, 4, 14)] {
        let shorten = [ev(
            id(1),
            prepayment(date, 200_000, PrepaymentMode::Shorten, 0),
        )];
        let s = preview(&core, &t, &shorten);
        check_schedule(&format!("shorten on {date}"), &t, &shorten, &s);
        assert_eq!(s.rows[2].event_ids, vec![id(1)], "on {date}");
        assert_eq!(s.rows[2].residual, 758_377, "on {date}");
        assert!(s.rows[3].event_ids.is_empty(), "on {date}");
    }
    // On the fourth due date it comes after the fourth installment.
    let on_fourth = [ev(
        id(1),
        prepayment(day(2026, 4, 15), 200_000, PrepaymentMode::Shorten, 0),
    )];
    let s = preview(&core, &t, &on_fourth);
    assert!(s.rows[2].event_ids.is_empty());
    assert_eq!(s.rows[3].event_ids, vec![id(1)]);
    assert_eq!(
        s.rows[3].residual,
        944_329 - 200_000,
        "the fourth row's debt, less 2 000"
    );
}

#[test]
fn shorten_keeps_the_installment_and_ends_earlier_lower_keeps_the_end_and_lowers_it() {
    let core = Core::open_in_memory().unwrap();
    let t = ten_k();
    let shorten = [ev(
        id(1),
        prepayment(day(2026, 3, 20), 200_000, PrepaymentMode::Shorten, 0),
    )];
    let s = preview(&core, &t, &shorten);
    check_schedule("shorten", &t, &shorten, &s);
    assert_eq!(s.rows.len(), 47);
    assert!(payments(&s)[..46].iter().all(|p| *p == 20_038));
    assert_eq!(last(&s).payment, 6_397);
    assert_eq!(last(&s).due_date, day(2029, 11, 15));
    assert_eq!(s.rows[3].interest, 4_740);
    assert_eq!(s.total_interest, 128_145);
    assert_eq!(s.total_prepaid, 200_000);
    assert_eq!(s.total_cost, 128_145, "the repayment itself is not a cost");
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);

    let lower = [ev(
        id(1),
        prepayment(day(2026, 3, 20), 200_000, PrepaymentMode::Lower, 0),
    )];
    let s = preview(&core, &t, &lower);
    check_schedule("lower", &t, &lower, &s);
    assert_eq!(s.rows.len(), 60);
    assert!(payments(&s)[..3].iter().all(|p| *p == 20_038));
    assert!(
        payments(&s)[3..59].iter().all(|p| *p == 15_856),
        "7 583,77 over 57 months at 7,5%"
    );
    assert_eq!(last(&s).payment, 15_875);
    assert_eq!(last(&s).due_date, day(2030, 12, 15));
    assert_eq!(s.total_interest, 163_925);
    assert_eq!(s.total_prepaid, 200_000);
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
}

#[test]
fn a_penalty_is_a_cost_only_and_never_touches_the_debt() {
    let core = Core::open_in_memory().unwrap();
    let t = ten_k();
    let free = [ev(
        id(1),
        prepayment(day(2026, 3, 20), 200_000, PrepaymentMode::Shorten, 0),
    )];
    let fined = [ev(
        id(1),
        prepayment(day(2026, 3, 20), 200_000, PrepaymentMode::Shorten, 5_000),
    )];
    let without = preview(&core, &t, &free);
    let with = preview(&core, &t, &fined);
    check_schedule("fined", &t, &fined, &with);
    assert_eq!(with.rows, without.rows);
    assert_eq!(with.total_penalties, 5_000);
    assert_eq!(without.total_penalties, 0);
    assert_eq!(with.total_cost, without.total_cost + 5_000);
    assert_eq!(with.taeg_ppm, without.taeg_ppm, "the TAEG is the terms'");
}

#[test]
fn a_prepayment_of_the_whole_residual_pays_the_loan_off_and_a_larger_one_is_clipped() {
    let core = Core::open_in_memory().unwrap();
    let t = ten_k();
    let residual_after_three = 958_377;
    let exact = [ev(
        id(1),
        prepayment(
            day(2026, 3, 20),
            residual_after_three,
            PrepaymentMode::Shorten,
            0,
        ),
    )];
    let s = preview(&core, &t, &exact);
    check_schedule("exact payoff", &t, &exact, &s);
    assert_eq!(s.rows.len(), 3);
    assert_eq!(
        last(&s).payment,
        20_038,
        "the third installment is a normal one"
    );
    assert_eq!(last(&s).residual, 0);
    assert_eq!(last(&s).event_ids, vec![id(1)]);
    assert_eq!(s.total_prepaid, residual_after_three);
    assert_eq!(s.total_interest, 18_491);
    assert_eq!(s.total_cost, 18_491);
    assert!(s.warnings.is_empty(), "no clipping: {:?}", s.warnings);

    for (what, amount) in [
        ("a cent over", residual_after_three + 1),
        ("the principal", 1_000_000),
    ] {
        let over = [ev(
            id(1),
            prepayment(day(2026, 3, 20), amount, PrepaymentMode::Lower, 100),
        )];
        let s = preview(&core, &t, &over);
        check_schedule(what, &t, &over, &s);
        assert_eq!(s.rows.len(), 3, "{what}");
        assert_eq!(
            s.total_prepaid, residual_after_three,
            "{what}: cut to the residual"
        );
        assert_eq!(s.total_penalties, 100, "{what}: the penalty stays");
        assert_eq!(s.total_cost, 18_491 + 100, "{what}");
        assert_eq!(clipped(&s), vec![id(1)], "{what}");
        assert_eq!(
            after_end(&s),
            Vec::<Uuid>::new(),
            "{what}: it was applied, not late"
        );
    }
}

#[test]
fn a_prepayment_before_the_first_installment_reduces_the_principal() {
    let core = Core::open_in_memory().unwrap();
    let t = ten_k();
    let shorten = [ev(
        id(1),
        prepayment(day(2026, 1, 10), 100_000, PrepaymentMode::Shorten, 0),
    )];
    let s = preview(&core, &t, &shorten);
    check_schedule("before first, shorten", &t, &shorten, &s);
    assert_eq!(s.event_ids_before_first, vec![id(1)]);
    assert!(s.rows.iter().all(|r| r.event_ids.is_empty()));
    assert_eq!(s.rows[0].interest, 5_625, "7,5% of 9 000 over twelve");
    assert_eq!(
        s.rows[0].payment, 20_038,
        "the installment of the terms is kept"
    );
    assert_eq!(s.rows[0].residual, 885_587);
    assert_eq!(s.rows.len(), 53);
    assert_eq!(last(&s).payment, 17_730);
    assert_eq!(s.total_interest, 159_706);
    assert_eq!(s.total_prepaid, 100_000);

    let lower = [ev(
        id(1),
        prepayment(day(2026, 1, 10), 100_000, PrepaymentMode::Lower, 0),
    )];
    let s = preview(&core, &t, &lower);
    check_schedule("before first, lower", &t, &lower, &s);
    assert_eq!(s.event_ids_before_first, vec![id(1)]);
    assert_eq!(s.rows.len(), 60);
    assert_eq!(s.rows[0].payment, 18_034, "9 000 over 60 months at 7,5%");
    assert_eq!(s.rows[0].interest, 5_625);
    assert_eq!(last(&s).payment, 18_043);
    assert_eq!(s.total_interest, 182_049);

    // The whole principal before the first installment: nothing to repay,
    // no rows, the event listed, the debt gone.
    let all = [ev(
        id(1),
        prepayment(day(2026, 1, 10), 1_000_000, PrepaymentMode::Shorten, 0),
    )];
    let s = preview(&core, &t, &all);
    check_schedule("before first, everything", &t, &all, &s);
    assert!(s.rows.is_empty());
    assert_eq!(s.event_ids_before_first, vec![id(1)]);
    assert_eq!(s.total_prepaid, 1_000_000);
    assert_eq!(s.total_interest, 0);
    assert_eq!(s.total_cost, 0);
    assert!(
        clipped(&s).is_empty(),
        "exactly the principal is not clipped"
    );
}

#[test]
fn on_the_same_day_a_prepayment_comes_before_a_rate_change_whatever_the_ids_say() {
    let core = Core::open_in_memory().unwrap();
    let t = ten_k();
    // The rate change carries the smaller id and still comes second: the
    // installment is worked out once, on the reduced debt at the new rate.
    let pair = [
        ev(
            id(1),
            rate_change(day(2026, 3, 20), 50_000, LoanInstallment::Computed),
        ),
        ev(
            id(2),
            prepayment(day(2026, 3, 20), 200_000, PrepaymentMode::Lower, 0),
        ),
    ];
    let s = preview(&core, &t, &pair);
    check_schedule("lower + computed", &t, &pair, &s);
    assert_eq!(s.rows[2].event_ids, vec![id(2), id(1)]);
    assert_eq!(s.rows[2].residual, 758_377);
    assert_eq!(s.rows[3].annual_rate_ppm, 50_000);
    assert_eq!((s.rows[3].payment, s.rows[3].interest), (14_975, 3_160));
    assert_eq!(s.rows.len(), 60);
    assert_eq!(last(&s).payment, 14_968);
    assert_eq!(s.total_interest, 113_682);

    // With the bank's letter on the rate change, the letter's installment is
    // the last word, the lowering having come before it.
    let letter = [
        ev(
            id(1),
            rate_change(day(2026, 3, 20), 50_000, contract(15_000)),
        ),
        ev(
            id(2),
            prepayment(day(2026, 3, 20), 200_000, PrepaymentMode::Lower, 0),
        ),
    ];
    let s = preview(&core, &t, &letter);
    check_schedule("lower + letter", &t, &letter, &s);
    assert_eq!(s.rows[2].event_ids, vec![id(2), id(1)]);
    assert!(payments(&s)[3..59].iter().all(|p| *p == 15_000));
    assert_eq!(last(&s).payment, 13_386);
    assert_eq!(s.total_interest, 113_500);
}

#[test]
fn two_events_of_the_same_kind_on_the_same_day_apply_by_id() {
    let core = Core::open_in_memory().unwrap();
    let t = ten_k();
    // Two repayments of 6 000 on a debt of 9 583,77: the smaller id takes
    // the whole amount, the other is cut to what is left.
    let two = [
        ev(
            id(2),
            prepayment(day(2026, 3, 20), 600_000, PrepaymentMode::Shorten, 0),
        ),
        ev(
            id(1),
            prepayment(day(2026, 3, 20), 600_000, PrepaymentMode::Shorten, 0),
        ),
    ];
    let s = preview(&core, &t, &two);
    check_schedule("two prepayments", &t, &two, &s);
    assert_eq!(s.rows.len(), 3);
    assert_eq!(s.rows[2].event_ids, vec![id(1), id(2)]);
    assert_eq!(s.total_prepaid, 958_377);
    assert_eq!(
        clipped(&s),
        vec![id(2)],
        "the second by id is the one clipped"
    );

    // Two rate changes: the larger id has the last word.
    let twice = [
        ev(
            id(2),
            rate_change(day(2026, 3, 20), 60_000, LoanInstallment::Computed),
        ),
        ev(
            id(1),
            rate_change(day(2026, 3, 20), 40_000, LoanInstallment::Computed),
        ),
    ];
    let s = preview(&core, &t, &twice);
    check_schedule("two rate changes", &t, &twice, &s);
    assert_eq!(s.rows[2].event_ids, vec![id(1), id(2)]);
    assert_eq!(s.rows[2].annual_rate_ppm, 75_000);
    assert!(s.rows[3..].iter().all(|r| r.annual_rate_ppm == 60_000));
    assert_eq!((s.rows[3].payment, s.rows[3].interest), (19_365, 4_792));
    assert_eq!(s.total_interest, 163_915);
}

#[test]
fn an_event_dated_after_a_shortened_end_is_flagged_and_changes_nothing() {
    let core = Core::open_in_memory().unwrap();
    let t = ten_k();
    // 8 000 after the third installment: the loan ends with the twelfth, in
    // December 2026, well inside the nominal five years.
    let big = ev(
        id(1),
        prepayment(day(2026, 3, 20), 800_000, PrepaymentMode::Shorten, 0),
    );
    let shortened = preview(&core, &t, &[big]);
    check_schedule("shortened", &t, &[big], &shortened);
    assert_eq!(shortened.rows.len(), 12);
    assert_eq!(last(&shortened).due_date, day(2026, 12, 15));
    assert_eq!(last(&shortened).payment, 2_632);

    let late_rate = ev(
        id(2),
        rate_change(day(2027, 3, 1), 10_000, LoanInstallment::Computed),
    );
    let late_prepayment = ev(
        id(3),
        prepayment(day(2028, 1, 1), 50_000, PrepaymentMode::Lower, 900),
    );
    let events = [big, late_rate, late_prepayment];
    let s = preview(&core, &t, &events);
    check_schedule("late events", &t, &events, &s);
    assert_eq!(s.rows, shortened.rows);
    assert_eq!(after_end(&s), vec![id(2), id(3)]);
    assert_eq!(
        s.total_prepaid, 800_000,
        "the late repayment is not applied"
    );
    assert_eq!(s.total_penalties, 0, "nor its penalty counted");
    assert_eq!(s.total_cost, shortened.total_cost);
    assert!(
        clipped(&s).is_empty(),
        "late is not clipped: {:?}",
        s.warnings
    );
}

#[test]
fn the_schedule_depends_on_the_set_of_events_not_on_the_order_they_are_given_in() {
    let core = Core::open_in_memory().unwrap();
    let t = ten_k();
    let events = [
        ev(
            id(3),
            rate_change(day(2027, 6, 15), 40_000, LoanInstallment::Computed),
        ),
        ev(
            id(1),
            prepayment(day(2026, 8, 1), 150_000, PrepaymentMode::Lower, 0),
        ),
        ev(
            id(2),
            rate_change(day(2026, 8, 1), 60_000, contract(17_000)),
        ),
        ev(
            id(4),
            prepayment(day(2026, 1, 2), 50_000, PrepaymentMode::Shorten, 0),
        ),
    ];
    let reference = preview(&core, &t, &events);
    check_schedule("reference", &t, &events, &reference);
    let mut g = Gen::new(5);
    for _ in 0..20 {
        let mut shuffled = events;
        g.shuffle(&mut shuffled);
        assert_eq!(preview(&core, &t, &shuffled), reference, "{shuffled:?}");
    }
    // The ids only break same-day ties: renumbering events on different days
    // keeps the same rows.
    let renumbered = [
        ev(id(9), events[0].event),
        ev(id(8), events[1].event),
        ev(id(7), events[2].event),
        ev(id(6), events[3].event),
    ];
    let s = preview(&core, &t, &renumbered);
    assert_eq!(payments(&s), payments(&reference));
    assert_eq!(s.total_interest, reference.total_interest);
    // August 1st follows the seventh installment (July 15th).
    assert_eq!(s.rows[6].event_ids, vec![id(8), id(7)]);
}

// ---------------------------------------------------------------------------
// Totals and TAEG
// ---------------------------------------------------------------------------

#[test]
fn the_total_cost_is_everything_paid_on_top_of_what_was_borrowed() {
    let core = Core::open_in_memory().unwrap();
    let mut t = ten_k();
    t.upfront_fees = 25_000;
    t.installment_fee = 150;
    let events = [
        ev(
            id(1),
            prepayment(day(2026, 3, 20), 200_000, PrepaymentMode::Lower, 2_000),
        ),
        ev(
            id(2),
            rate_change(day(2027, 1, 15), 60_000, LoanInstallment::Computed),
        ),
        ev(
            id(3),
            prepayment(day(2028, 6, 1), 5_000_000, PrepaymentMode::Shorten, 1_000),
        ),
    ];
    let s = preview(&core, &t, &events);
    check_schedule("totals", &t, &events, &s);
    let paid: i64 = s.rows.iter().map(|r| r.payment).sum();
    assert_eq!(s.total_fees, 150 * i64::try_from(s.rows.len()).unwrap());
    assert_eq!(s.total_penalties, 3_000);
    assert!(
        s.total_prepaid < 5_200_000,
        "the second repayment was clipped"
    );
    assert_eq!(clipped(&s), vec![id(3)]);
    assert_eq!(
        s.total_cost,
        paid + 25_000 + s.total_fees + s.total_prepaid + s.total_penalties - 1_000_000
    );
    assert_eq!(
        s.total_cost,
        s.total_interest + 25_000 + s.total_fees + s.total_penalties,
        "paid + prepaid − principal is the interest"
    );
    assert_eq!(s.taeg_ppm, preview(&core, &t, &[]).taeg_ppm);
}

#[test]
fn without_fees_the_taeg_is_the_yearly_equivalent_of_the_tan() {
    let core = Core::open_in_memory().unwrap();
    // Rounding the installments to the cent moves the solved rate: measured
    // with exact decimals, up to 7 millionths on a loan of 1 000 €, a tenth
    // of a millionth from 10 000 € up. Two millionths cover that and a
    // bisection that stops on either neighbour.
    let mut g = Gen::new(6);
    for case in 0..60 {
        let mut t = gen_terms(&mut g);
        t.principal = t.principal.max(1_000_000);
        t.upfront_fees = 0;
        t.installment_fee = 0;
        let s = preview(&core, &t, &[]);
        if below_interest(&s).is_some() {
            continue;
        }
        let taeg = i64::from(s.taeg_ppm.unwrap_or_else(|| panic!("case {case}: no TAEG")));
        let closed = i64::from(closed_taeg(t.annual_rate_ppm));
        assert!(
            (taeg - closed).abs() <= 2,
            "case {case} {t:?}: taeg {taeg} against (1 + i)^12 − 1 = {closed}"
        );
    }
    for (ppm, expected) in [
        (30_000, 30_416),
        (75_000, 77_633),
        (120_000, 126_825),
        (0, 0),
    ] {
        let s = preview(&core, &terms(5_000_000, ppm, 120, day(2026, 1, 1)), &[]);
        let taeg = i64::from(s.taeg_ppm.unwrap());
        assert!(
            (taeg - expected).abs() <= 1,
            "{ppm}: taeg {taeg}, expected {expected}"
        );
    }
}

#[test]
fn fees_raise_the_taeg_and_more_fees_raise_it_more() {
    let core = Core::open_in_memory().unwrap();
    let mut g = Gen::new(7);
    for case in 0..60 {
        let mut t = gen_terms(&mut g);
        t.principal = t.principal.max(100_000);
        t.upfront_fees = 0;
        t.installment_fee = 0;
        let bare = preview(&core, &t, &[]);
        if below_interest(&bare).is_some() {
            continue;
        }
        let mut upfront = t;
        upfront.upfront_fees = g.range(100, t.principal / 10);
        let mut per_row = t;
        per_row.installment_fee = g.range(100, 5_000);
        let mut both = upfront;
        both.installment_fee = per_row.installment_fee;
        let bare_taeg = bare.taeg_ppm.unwrap();
        let up = preview(&core, &upfront, &[]).taeg_ppm.unwrap();
        let row = preview(&core, &per_row, &[]).taeg_ppm.unwrap();
        let all = preview(&core, &both, &[]).taeg_ppm.unwrap();
        assert!(
            up > bare_taeg,
            "case {case}: upfront fees {up} vs {bare_taeg}"
        );
        assert!(
            row > bare_taeg,
            "case {case}: installment fees {row} vs {bare_taeg}"
        );
        assert!(
            all > up && all > row,
            "case {case}: both {all} vs {up} and {row}"
        );
        assert!(
            up > closed_taeg(t.annual_rate_ppm),
            "case {case}: with fees the TAEG is over the TAN's yearly equivalent"
        );
    }
}

// ---------------------------------------------------------------------------
// Commands and engine
// ---------------------------------------------------------------------------

fn loan_cmd(name: &str, terms: LoanTerms, recurring_id: Option<Uuid>) -> Command {
    Command::CreateLoan {
        name: name.to_string(),
        note: None,
        terms,
        recurring_id,
    }
}

fn event_cmd(loan_id: Uuid, event: LoanEvent) -> Command {
    Command::AddLoanEvent { loan_id, event }
}

fn remove_cmd(loan_id: Uuid, event_id: Uuid) -> Command {
    Command::RemoveLoanEvent { loan_id, event_id }
}

fn patch_cmd(loan_id: Uuid, patch: LoanPatch) -> Command {
    Command::UpdateLoan { loan_id, patch }
}

fn link_cmd(loan_id: Uuid, recurring_id: Option<Uuid>) -> Command {
    Command::SetLoanRecurring {
        loan_id,
        recurring_id,
    }
}

fn loans_of(core: &Core, vault: Uuid) -> Vec<LoanView> {
    core.loans(vault).unwrap()
}

fn loan_of(fx: &Fx, loan_id: Uuid) -> LoanView {
    loans_of(&fx.core, fx.vault)
        .into_iter()
        .find(|l| l.id == loan_id)
        .unwrap_or_else(|| panic!("no loan {loan_id}"))
}

fn schedule_of(fx: &Fx, loan_id: Uuid) -> LoanSchedule {
    fx.core.loan_schedule(fx.vault, loan_id).unwrap()
}

fn overview_of(fx: &Fx, loan_id: Uuid, today: NaiveDate) -> LoanOverview {
    fx.core
        .loan_overview(fx.vault, today)
        .unwrap()
        .into_iter()
        .find(|o| o.loan_id == loan_id)
        .unwrap_or_else(|| panic!("no overview of {loan_id} on {today}"))
}

/// An expense template of `amount` on the 15th of every month from `start`,
/// on the only wallet, in Unallocated, category Mutuo.
fn mortgage_template(fx: &mut Fx, amount: i64, start: NaiveDate, end: Option<NaiveDate>) -> Uuid {
    fx.id(Command::CreateRecurring {
        transaction_kind: TransactionKind::Expense,
        amount,
        wallet_id: None,
        flow_id: None,
        category: Some("Mutuo".to_string()),
        note: None,
        schedule: Schedule {
            frequency: Frequency::Monthly { day: 15 },
            interval: 1,
            start_date: start,
            end_date: end,
        },
        owner: None,
    })
}

fn execute_cmd(recurring_id: Uuid, period: NaiveDate, amount: Option<i64>) -> Command {
    Command::ExecuteRecurring {
        recurring_id,
        period_date: period,
        occurred_at: noon(period),
        person: None,
        amount,
    }
}

/// Everything a loan can touch, read back from the core.
#[derive(Debug, PartialEq)]
struct Projection {
    snapshot: VaultSnapshot,
    transactions: Vec<TransactionView>,
    loans: Vec<LoanView>,
    schedules: Vec<(Uuid, Result<LoanSchedule, DomainError>)>,
    overview: Vec<LoanOverview>,
    linked: Vec<LinkedInstallment>,
    recurring: Vec<RecurringView>,
    log: Vec<(i64, Uuid, Option<Uuid>)>,
}

fn projection(core: &Core, vault: Uuid) -> Projection {
    let loans = loans_of(core, vault);
    Projection {
        snapshot: core.snapshot(vault).unwrap(),
        transactions: list(core, vault, &all()),
        schedules: loans
            .iter()
            .map(|l| (l.id, core.loan_schedule(vault, l.id)))
            .collect(),
        loans,
        overview: core.loan_overview(vault, day(2026, 6, 1)).unwrap(),
        linked: core.linked_installments(vault).unwrap(),
        recurring: core.list_recurring(vault, true).unwrap(),
        log: core
            .commands_since(vault, 0)
            .unwrap()
            .iter()
            .map(|r| (r.seq, r.envelope.id, r.result_id))
            .collect(),
    }
}

/// Runs `cmd` expecting a refusal, and checks that the vault is as it was:
/// no log row, no loan, no event, no link, no transaction.
fn refused(fx: &mut Fx, what: &str, cmd: Command) -> DomainError {
    let before = projection(&fx.core, fx.vault);
    let err = fx.try_run(cmd).expect_err(what);
    assert_eq!(
        projection(&fx.core, fx.vault),
        before,
        "{what}: changed something"
    );
    assert_ne!(err.code(), "storage_error", "{what}: {err}");
    err
}

/// [`refused`], with the error code the app relies on.
fn refused_as(fx: &mut Fx, what: &str, code: &str, cmd: Command) -> DomainError {
    let err = refused(fx, what, cmd);
    assert_eq!(err.code(), code, "{what}: {err}");
    err
}

#[test]
fn a_created_loan_is_listed_with_its_terms_and_its_schedule_is_the_preview_of_them() {
    let mut fx = setup();
    let t = ten_k();
    let receipt = fx.run(Command::CreateLoan {
        name: "  Cucina ".to_string(),
        note: Some(" rate al negozio ".to_string()),
        terms: t,
        recurring_id: None,
    });
    let loan_id = receipt.result_id.unwrap();
    assert_eq!(loan_id, receipt.command_id, "the loan id is the command id");

    let loans = loans_of(&fx.core, fx.vault);
    assert_eq!(loans.len(), 1);
    let loan = &loans[0];
    assert_eq!(loan.id, loan_id);
    assert_eq!(loan.name, "Cucina", "trimmed");
    assert_eq!(loan.note.as_deref(), Some("rate al negozio"));
    assert_eq!(loan.terms, t);
    assert!(loan.events.is_empty());
    assert_eq!(loan.recurring_id, None);
    assert!(!loan.archived);
    assert_eq!(loan.created_by, "alice");

    let schedule = schedule_of(&fx, loan_id);
    assert_eq!(schedule, fx.core.preview_loan(&t, &[]).unwrap());
    assert_eq!(schedule.rows.len(), 60);

    // By name, archived included.
    fx.id(loan_cmd("Auto", mortgage(), None));
    let names: Vec<String> = loans_of(&fx.core, fx.vault)
        .into_iter()
        .map(|l| l.name)
        .collect();
    assert_eq!(names, vec!["Auto", "Cucina"]);
}

#[test]
fn a_loan_is_refused_on_terms_outside_the_bounds_and_the_vault_stays_as_it_was() {
    let mut fx = setup();
    let first = day(2026, 1, 15);
    let with = |f: fn(&mut LoanTerms)| {
        let mut t = ten_k();
        f(&mut t);
        t
    };
    let cases: Vec<(&str, &str, LoanTerms)> = vec![
        ("no principal", "invalid_amount", with(|t| t.principal = 0)),
        (
            "a negative principal",
            "invalid_amount",
            with(|t| t.principal = -100),
        ),
        (
            "a rate over fifty percent",
            "invalid_command",
            with(|t| t.annual_rate_ppm = MAX_RATE_PPM + 1),
        ),
        ("no months", "invalid_command", with(|t| t.months = 0)),
        (
            "more than fifty years",
            "invalid_command",
            with(|t| t.months = MAX_MONTHS + 1),
        ),
        (
            "negative upfront fees",
            "invalid_amount",
            with(|t| t.upfront_fees = -1),
        ),
        (
            "a negative installment fee",
            "invalid_amount",
            with(|t| t.installment_fee = -1),
        ),
        (
            "a contract of nothing",
            "invalid_amount",
            with(|t| t.installment = contract(0)),
        ),
        (
            "a negative contract",
            "invalid_amount",
            with(|t| t.installment = contract(-5)),
        ),
        // 62,50 is the first installment's interest: the debt would never
        // go down, and that depends on the command alone.
        (
            "a contract below the first interest",
            "invalid_amount",
            with(|t| t.installment = contract(6_250)),
        ),
    ];
    for (what, code, t) in cases {
        refused_as(&mut fx, what, code, loan_cmd("Cucina", t, None));
        assert!(loans_of(&fx.core, fx.vault).is_empty(), "{what}");
    }
    refused_as(
        &mut fx,
        "a blank name",
        "invalid_name",
        loan_cmd("   ", ten_k(), None),
    );

    // The bounds themselves are accepted.
    let edges = [
        terms(1, 0, 1, first),
        terms(1_000_000_000, MAX_RATE_PPM, MAX_MONTHS, first),
        with(|t| t.installment = contract(6_251)),
    ];
    for t in edges {
        fx.id(loan_cmd("Edge", t, None));
    }
    assert_eq!(loans_of(&fx.core, fx.vault).len(), 3);
}

#[test]
fn a_loan_at_the_upper_bounds_is_accepted_and_one_cent_over_them_is_refused() {
    let mut fx = setup();
    let template = mortgage_template(&mut fx, 100, day(2026, 1, 31), None);
    let first = day(2026, 1, 31);
    let last_nominal = day(2075, 12, 31);
    let mut top = terms(MAX_LOAN_AMOUNT, MAX_RATE_PPM, MAX_MONTHS, first);
    top.upfront_fees = MAX_LOAN_AMOUNT;
    top.installment_fee = MAX_LOAN_AMOUNT;
    let loan = fx.id(loan_cmd("Big", top, Some(template)));
    let mut bounded = top;
    bounded.installment = contract(MAX_LOAN_AMOUNT);
    let other = fx.id(loan_cmd("Bigger", bounded, None));
    for event in [
        rate_change(day(2027, 1, 31), MAX_RATE_PPM, contract(MAX_LOAN_AMOUNT)),
        rate_change(day(2030, 2, 28), 0, LoanInstallment::Computed),
        prepayment(
            day(2026, 1, 1),
            MAX_LOAN_AMOUNT,
            PrepaymentMode::Lower,
            MAX_LOAN_AMOUNT,
        ),
        prepayment(
            last_nominal,
            MAX_LOAN_AMOUNT,
            PrepaymentMode::Shorten,
            MAX_LOAN_AMOUNT,
        ),
        prepayment(day(2040, 6, 15), 1, PrepaymentMode::Shorten, 0),
    ] {
        fx.id(event_cmd(loan, event));
        fx.id(event_cmd(other, event));
    }
    for loan_id in [loan, other] {
        let view = loan_of(&fx, loan_id);
        let s = schedule_of(&fx, loan_id);
        check_schedule(&view.name, &view.terms, &view.events, &s);
        assert_eq!(s, fx.core.preview_loan(&view.terms, &view.events).unwrap());
    }
    for today in [
        day(2025, 1, 1),
        day(2026, 1, 31),
        day(2026, 2, 15),
        day(2040, 6, 15),
        last_nominal,
        day(2100, 1, 1),
    ] {
        let overview = fx.core.loan_overview(fx.vault, today).unwrap();
        assert_eq!(overview.len(), 2, "on {today}");
        for o in &overview {
            assert!(o.residual >= 0 && o.total_cost >= 0, "on {today}: {o:?}");
            assert!(o.paid_count <= o.total_count, "on {today}: {o:?}");
        }
    }
    assert!(!fx.core.linked_installments(fx.vault).unwrap().is_empty());

    // One cent over, on any amount, in any command: refused as an amount,
    // the vault as it was.
    let over = MAX_LOAN_AMOUNT + 1;
    let with = |f: fn(&mut LoanTerms)| {
        let mut t = ten_k();
        f(&mut t);
        t
    };
    for (what, t) in [
        ("the principal", with(|t| t.principal = MAX_LOAN_AMOUNT + 1)),
        (
            "upfront fees",
            with(|t| t.upfront_fees = MAX_LOAN_AMOUNT + 1),
        ),
        (
            "an installment fee",
            with(|t| t.installment_fee = MAX_LOAN_AMOUNT + 1),
        ),
        (
            "a contract",
            with(|t| t.installment = contract(MAX_LOAN_AMOUNT + 1)),
        ),
    ] {
        refused_as(&mut fx, what, "invalid_amount", loan_cmd("Over", t, None));
    }
    for (what, patch) in [
        (
            "the principal",
            LoanPatch {
                principal: Some(over),
                ..LoanPatch::default()
            },
        ),
        (
            "upfront fees",
            LoanPatch {
                upfront_fees: Some(over),
                ..LoanPatch::default()
            },
        ),
        (
            "an installment fee",
            LoanPatch {
                installment_fee: Some(over),
                ..LoanPatch::default()
            },
        ),
        (
            "a contract",
            LoanPatch {
                installment: Some(contract(over)),
                ..LoanPatch::default()
            },
        ),
    ] {
        refused_as(
            &mut fx,
            &format!("{what} patched over the bound"),
            "invalid_amount",
            patch_cmd(loan, patch),
        );
    }
    for (what, event) in [
        (
            "a repayment",
            prepayment(day(2030, 1, 31), over, PrepaymentMode::Shorten, 0),
        ),
        (
            "a penalty",
            prepayment(day(2030, 1, 31), 1, PrepaymentMode::Shorten, over),
        ),
        (
            "a letter",
            rate_change(day(2030, 1, 31), 10_000, contract(over)),
        ),
    ] {
        refused_as(&mut fx, what, "invalid_amount", event_cmd(loan, event));
    }
    assert_eq!(loans_of(&fx.core, fx.vault).len(), 2);
}

#[test]
fn a_loan_can_be_linked_only_to_an_active_expense_template_of_its_vault_that_no_other_loan_has() {
    let mut fx = setup();
    let template = mortgage_template(&mut fx, 20_038, day(2026, 1, 15), None);
    let income = fx.id(Command::CreateRecurring {
        transaction_kind: TransactionKind::Income,
        amount: 100_000,
        wallet_id: None,
        flow_id: None,
        category: Some("Salary".to_string()),
        note: None,
        schedule: monthly_from(1, day(2026, 1, 1)),
        owner: None,
    });
    let archived = mortgage_template(&mut fx, 100, day(2026, 1, 15), None);
    fx.run(Command::ArchiveRecurring {
        recurring_id: archived,
    });
    let (other, _) = other_vault(&mut fx.core);
    let theirs = run(
        &mut fx.core,
        other,
        Command::CreateRecurring {
            transaction_kind: TransactionKind::Expense,
            amount: 100,
            wallet_id: None,
            flow_id: None,
            category: None,
            note: None,
            schedule: monthly_from(1, day(2026, 1, 1)),
            owner: None,
        },
    )
    .result_id
    .unwrap();

    refused_as(
        &mut fx,
        "an unknown template",
        "not_found",
        loan_cmd("Cucina", ten_k(), Some(Uuid::now_v7())),
    );
    refused_as(
        &mut fx,
        "a template of another vault",
        "not_found",
        loan_cmd("Cucina", ten_k(), Some(theirs)),
    );
    refused_as(
        &mut fx,
        "an income template",
        "invalid_command",
        loan_cmd("Cucina", ten_k(), Some(income)),
    );
    refused_as(
        &mut fx,
        "an archived template",
        "invalid_command",
        loan_cmd("Cucina", ten_k(), Some(archived)),
    );
    assert!(loans_of(&fx.core, fx.vault).is_empty());

    let first = fx.id(loan_cmd("Cucina", ten_k(), Some(template)));
    assert_eq!(loan_of(&fx, first).recurring_id, Some(template));

    // A second loan on the same template: a domain refusal, never the unique
    // index surfacing as a storage error.
    let err = refused_as(
        &mut fx,
        "a template already linked",
        "already_exists",
        loan_cmd("Auto", mortgage(), Some(template)),
    );
    assert_ne!(err.code(), "storage_error");
    let second = fx.id(loan_cmd("Auto", mortgage(), None));
    refused_as(
        &mut fx,
        "linking a second loan to the same template",
        "already_exists",
        link_cmd(second, Some(template)),
    );
    refused_as(
        &mut fx,
        "linking to an income template",
        "invalid_command",
        link_cmd(second, Some(income)),
    );
    refused_as(
        &mut fx,
        "linking to an archived template",
        "invalid_command",
        link_cmd(second, Some(archived)),
    );
    refused_as(
        &mut fx,
        "linking to a template of another vault",
        "not_found",
        link_cmd(second, Some(theirs)),
    );
    refused_as(
        &mut fx,
        "linking an unknown loan",
        "not_found",
        link_cmd(Uuid::now_v7(), Some(template)),
    );

    // Unlinking frees the template for the other loan.
    fx.run(link_cmd(first, None));
    assert_eq!(loan_of(&fx, first).recurring_id, None);
    fx.run(link_cmd(second, Some(template)));
    assert_eq!(loan_of(&fx, second).recurring_id, Some(template));
    refused_as(
        &mut fx,
        "linking back while the other has it",
        "already_exists",
        link_cmd(first, Some(template)),
    );
    // Archiving the template is allowed and keeps the link.
    fx.run(Command::ArchiveRecurring {
        recurring_id: template,
    });
    assert_eq!(loan_of(&fx, second).recurring_id, Some(template));
}

#[test]
fn updating_a_loan_changes_only_the_fields_given_and_the_schedule_follows_the_new_terms() {
    let mut fx = setup();
    let loan = fx.id(Command::CreateLoan {
        name: "Cucina".to_string(),
        note: Some("negozio".to_string()),
        terms: ten_k(),
        recurring_id: None,
    });
    let event = fx.id(event_cmd(
        loan,
        prepayment(day(2026, 3, 20), 200_000, PrepaymentMode::Lower, 0),
    ));

    refused_as(
        &mut fx,
        "an empty patch",
        "invalid_command",
        patch_cmd(loan, LoanPatch::default()),
    );
    refused_as(
        &mut fx,
        "an unknown loan",
        "not_found",
        patch_cmd(
            Uuid::now_v7(),
            LoanPatch {
                name: Some("X".to_string()),
                ..LoanPatch::default()
            },
        ),
    );
    refused_as(
        &mut fx,
        "a blank name",
        "invalid_name",
        patch_cmd(
            loan,
            LoanPatch {
                name: Some("  ".to_string()),
                ..LoanPatch::default()
            },
        ),
    );
    for (what, code, patch) in [
        (
            "no principal",
            "invalid_amount",
            LoanPatch {
                principal: Some(0),
                ..LoanPatch::default()
            },
        ),
        (
            "a rate over the bound",
            "invalid_command",
            LoanPatch {
                annual_rate_ppm: Some(MAX_RATE_PPM + 1),
                ..LoanPatch::default()
            },
        ),
        (
            "too many months",
            "invalid_command",
            LoanPatch {
                months: Some(MAX_MONTHS + 1),
                ..LoanPatch::default()
            },
        ),
        (
            "negative fees",
            "invalid_amount",
            LoanPatch {
                upfront_fees: Some(-1),
                ..LoanPatch::default()
            },
        ),
        (
            "a contract below the first interest",
            "invalid_amount",
            LoanPatch {
                installment: Some(contract(6_250)),
                ..LoanPatch::default()
            },
        ),
        // The rate and the contract are checked together, as they will be.
        (
            "a rate that starves the contract",
            "invalid_amount",
            LoanPatch {
                annual_rate_ppm: Some(MAX_RATE_PPM),
                installment: Some(contract(30_000)),
                ..LoanPatch::default()
            },
        ),
    ] {
        refused_as(&mut fx, what, code, patch_cmd(loan, patch));
    }

    fx.run(patch_cmd(
        loan,
        LoanPatch {
            note: Some("".to_string()),
            annual_rate_ppm: Some(50_000),
            installment_fee: Some(200),
            ..LoanPatch::default()
        },
    ));
    let view = loan_of(&fx, loan);
    assert_eq!(view.name, "Cucina", "untouched");
    assert_eq!(view.note, None, "a blank note clears it");
    let mut expected = ten_k();
    expected.annual_rate_ppm = 50_000;
    expected.installment_fee = 200;
    assert_eq!(view.terms, expected);
    assert_eq!(view.events.len(), 1, "the event stays");
    assert_eq!(
        schedule_of(&fx, loan),
        fx.core.preview_loan(&expected, &view.events).unwrap(),
        "the schedule is worked out again from the new terms"
    );
    assert!(schedule_of(&fx, loan).rows.iter().all(|r| r.fee == 200));
    assert_eq!(schedule_of(&fx, loan).rows[2].event_ids, vec![event]);

    // Moving the first due date leaves events where they are, by date.
    fx.run(patch_cmd(
        loan,
        LoanPatch {
            first_due: Some(day(2026, 2, 15)),
            months: Some(48),
            ..LoanPatch::default()
        },
    ));
    let view = loan_of(&fx, loan);
    assert_eq!(view.terms.first_due, day(2026, 2, 15));
    assert_eq!(view.terms.months, 48);
    let s = schedule_of(&fx, loan);
    assert_eq!(s.rows.len(), 48);
    assert_eq!(
        s.rows[1].event_ids,
        vec![event],
        "March 20th now follows the second installment"
    );
}

#[test]
fn an_archived_loan_refuses_edits_events_and_links_and_a_restored_one_takes_them_again() {
    let mut fx = setup();
    let template = mortgage_template(&mut fx, 20_038, day(2026, 1, 15), None);
    let loan = fx.id(loan_cmd("Cucina", ten_k(), Some(template)));
    let other = fx.id(loan_cmd("Auto", mortgage(), None));
    let today = day(2026, 6, 1);
    assert_eq!(fx.core.loan_overview(fx.vault, today).unwrap().len(), 2);

    refused_as(
        &mut fx,
        "restoring an active loan",
        "invalid_command",
        Command::RestoreLoan { loan_id: loan },
    );
    refused_as(
        &mut fx,
        "archiving an unknown loan",
        "not_found",
        Command::ArchiveLoan {
            loan_id: Uuid::now_v7(),
        },
    );
    let before = schedule_of(&fx, loan);
    fx.run(Command::ArchiveLoan { loan_id: loan });

    let view = loan_of(&fx, loan);
    assert!(view.archived);
    assert_eq!(view.recurring_id, Some(template), "the link is kept");
    assert_eq!(
        loans_of(&fx.core, fx.vault).len(),
        2,
        "listed, archived included"
    );
    let overview = fx.core.loan_overview(fx.vault, today).unwrap();
    assert_eq!(
        overview.iter().map(|o| o.loan_id).collect::<Vec<_>>(),
        vec![other],
        "the overview lists the active loans"
    );
    assert!(
        fx.core.linked_installments(fx.vault).unwrap().is_empty(),
        "an archived loan's template records nothing of it"
    );

    refused_as(
        &mut fx,
        "archiving twice",
        "invalid_command",
        Command::ArchiveLoan { loan_id: loan },
    );
    refused_as(
        &mut fx,
        "an edit on an archived loan",
        "invalid_command",
        patch_cmd(
            loan,
            LoanPatch {
                name: Some("Cucina nuova".to_string()),
                ..LoanPatch::default()
            },
        ),
    );
    refused_as(
        &mut fx,
        "an event on an archived loan",
        "invalid_command",
        event_cmd(
            loan,
            prepayment(day(2026, 3, 20), 100, PrepaymentMode::Shorten, 0),
        ),
    );
    refused_as(
        &mut fx,
        "a link on an archived loan",
        "invalid_command",
        link_cmd(loan, None),
    );

    fx.run(Command::RestoreLoan { loan_id: loan });
    let view = loan_of(&fx, loan);
    assert!(!view.archived);
    assert_eq!(schedule_of(&fx, loan), before);
    assert_eq!(fx.core.loan_overview(fx.vault, today).unwrap().len(), 2);
    assert_eq!(fx.core.linked_installments(fx.vault).unwrap().len(), 60);
    let event = fx.id(event_cmd(
        loan,
        prepayment(day(2026, 3, 20), 100, PrepaymentMode::Shorten, 0),
    ));
    assert_eq!(loan_of(&fx, loan).events[0].id, event);
}

#[test]
fn an_event_is_accepted_inside_the_nominal_window_whatever_the_schedule_makes_of_it() {
    let mut fx = setup();
    let loan = fx.id(loan_cmd("Cucina", ten_k(), None));
    let last_nominal = day(2030, 12, 15);

    // Validation looks at the command and the loan's row only.
    for (what, code, event) in [
        (
            "a rate change before the first installment",
            "invalid_command",
            rate_change(day(2026, 1, 14), 50_000, LoanInstallment::Computed),
        ),
        (
            "a rate change after the last nominal installment",
            "invalid_command",
            rate_change(
                plus_days(last_nominal, 1),
                50_000,
                LoanInstallment::Computed,
            ),
        ),
        (
            "a prepayment after the last nominal installment",
            "invalid_command",
            prepayment(plus_days(last_nominal, 1), 100, PrepaymentMode::Shorten, 0),
        ),
        (
            "a rate over the bound",
            "invalid_command",
            rate_change(
                day(2026, 6, 15),
                MAX_RATE_PPM + 1,
                LoanInstallment::Computed,
            ),
        ),
        (
            "a letter of nothing",
            "invalid_amount",
            rate_change(day(2026, 6, 15), 50_000, contract(0)),
        ),
        (
            "a prepayment of nothing",
            "invalid_amount",
            prepayment(day(2026, 3, 20), 0, PrepaymentMode::Shorten, 0),
        ),
        (
            "a negative prepayment",
            "invalid_amount",
            prepayment(day(2026, 3, 20), -100, PrepaymentMode::Shorten, 0),
        ),
        (
            "a negative penalty",
            "invalid_amount",
            prepayment(day(2026, 3, 20), 100, PrepaymentMode::Shorten, -1),
        ),
    ] {
        refused_as(&mut fx, what, code, event_cmd(loan, event));
        assert!(loan_of(&fx, loan).events.is_empty(), "{what}");
    }
    refused_as(
        &mut fx,
        "an unknown loan",
        "not_found",
        event_cmd(
            Uuid::now_v7(),
            prepayment(day(2026, 3, 20), 100, PrepaymentMode::Shorten, 0),
        ),
    );

    // Accepted: the window's edges, a repayment before the first installment,
    // one larger than the whole debt, a letter below the interest, a change
    // dated after an end the repayment will have shortened. Each is a
    // warning of the schedule, never a refusal.
    let accepted = [
        prepayment(day(2025, 12, 1), 50_000, PrepaymentMode::Shorten, 0),
        rate_change(day(2026, 1, 15), 60_000, LoanInstallment::Computed),
        prepayment(day(2026, 3, 20), 5_000_000, PrepaymentMode::Shorten, 0),
        rate_change(day(2026, 2, 15), 70_000, contract(1)),
        rate_change(last_nominal, 10_000, LoanInstallment::Computed),
        prepayment(last_nominal, 100, PrepaymentMode::Lower, 50),
    ];
    let mut ids = Vec::new();
    for event in accepted {
        let receipt = fx.run(event_cmd(loan, event));
        assert_eq!(
            receipt.result_id,
            Some(receipt.command_id),
            "the event id is the command id"
        );
        ids.push(receipt.command_id);
    }
    let view = loan_of(&fx, loan);
    assert_eq!(view.events.len(), 6);
    let dates: Vec<NaiveDate> = view.events.iter().map(|e| e.event.date()).collect();
    assert!(dates.windows(2).all(|w| w[0] <= w[1]), "by date: {dates:?}");
    assert_eq!(
        view.events[4].id.min(view.events[5].id),
        view.events[4].id,
        "then by id"
    );
    let s = schedule_of(&fx, loan);
    assert_eq!(s, fx.core.preview_loan(&ten_k(), &view.events).unwrap());
    assert_eq!(s.event_ids_before_first, vec![ids[0]]);
    assert_eq!(clipped(&s), vec![ids[2]]);
    assert_eq!(after_end(&s), {
        let mut late = vec![ids[4], ids[5]];
        late.sort();
        late
    });
    assert_eq!(s.rows.len(), 3, "paid off with the third installment");
}

#[test]
fn removing_an_event_restores_the_schedule_exactly() {
    let mut fx = setup();
    let loan = fx.id(loan_cmd("Cucina", ten_k(), None));
    let other = fx.id(loan_cmd("Auto", mortgage(), None));
    let before = (loan_of(&fx, loan), schedule_of(&fx, loan));

    let first = fx.id(event_cmd(
        loan,
        prepayment(day(2026, 3, 20), 200_000, PrepaymentMode::Lower, 1_000),
    ));
    let second = fx.id(event_cmd(
        loan,
        rate_change(day(2027, 1, 15), 60_000, LoanInstallment::Computed),
    ));
    let with_first = schedule_of(&fx, loan);
    assert_ne!(with_first, before.1);
    let theirs = fx.id(event_cmd(
        other,
        rate_change(day(2027, 1, 1), 40_000, LoanInstallment::Computed),
    ));

    refused_as(
        &mut fx,
        "an unknown event",
        "not_found",
        remove_cmd(loan, Uuid::now_v7()),
    );
    refused_as(
        &mut fx,
        "an event of another loan",
        "not_found",
        remove_cmd(loan, theirs),
    );
    refused_as(
        &mut fx,
        "an unknown loan",
        "not_found",
        remove_cmd(Uuid::now_v7(), first),
    );
    assert_eq!(loan_of(&fx, loan).events.len(), 2);
    assert_eq!(loan_of(&fx, other).events.len(), 1);

    fx.run(remove_cmd(loan, second));
    let view = loan_of(&fx, loan);
    assert_eq!(
        view.events.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![first]
    );
    assert_eq!(
        schedule_of(&fx, loan),
        fx.core.preview_loan(&ten_k(), &view.events).unwrap()
    );
    fx.run(remove_cmd(loan, first));
    assert_eq!((loan_of(&fx, loan), schedule_of(&fx, loan)), before);
    refused_as(
        &mut fx,
        "removing twice",
        "not_found",
        remove_cmd(loan, first),
    );
    assert_eq!(
        loan_of(&fx, other).events.len(),
        1,
        "the other loan kept its event"
    );
}

#[test]
fn an_event_dated_in_the_past_recomputes_the_schedule_from_there_on() {
    let mut fx = setup();
    let loan = fx.id(loan_cmd("Cucina", ten_k(), None));
    let later = fx.id(event_cmd(
        loan,
        rate_change(day(2028, 1, 15), 60_000, LoanInstallment::Computed),
    ));
    let before = schedule_of(&fx, loan);

    let earlier = fx.id(event_cmd(
        loan,
        prepayment(day(2026, 3, 20), 200_000, PrepaymentMode::Shorten, 0),
    ));
    let after = schedule_of(&fx, loan);
    assert_eq!(after.rows[..2], before.rows[..2], "the rows before it stay");
    let third = (&after.rows[2], &before.rows[2]);
    assert_eq!(
        (third.0.payment, third.0.interest, third.0.capital),
        (third.1.payment, third.1.interest, third.1.capital),
        "the third installment is paid as before"
    );
    assert_eq!(
        after.rows[2].event_ids,
        vec![earlier],
        "listed on the third row"
    );
    assert_eq!(after.rows[2].residual, before.rows[2].residual - 200_000);
    assert_ne!(after.rows[3], before.rows[3]);
    assert!(
        after.rows.len() < before.rows.len(),
        "Shorten ends it earlier"
    );
    assert!(
        after.rows.iter().any(|r| r.event_ids == vec![later]),
        "the later event is still applied"
    );
    assert_eq!(
        loan_of(&fx, loan)
            .events
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        vec![earlier, later],
        "events are listed by date, not by when they were added"
    );
}

#[test]
fn two_vaults_adding_the_same_events_in_a_different_order_get_the_same_schedule() {
    let mut fx = setup();
    let (other, _) = other_vault(&mut fx.core);
    let loan_a = fx.id(loan_cmd("Cucina", ten_k(), None));
    let loan_b = run(&mut fx.core, other, loan_cmd("Cucina", ten_k(), None))
        .result_id
        .unwrap();
    let events = [
        (
            id(0x10),
            rate_change(day(2027, 6, 15), 40_000, LoanInstallment::Computed),
        ),
        (
            id(0x11),
            prepayment(day(2026, 8, 1), 150_000, PrepaymentMode::Lower, 0),
        ),
        (
            id(0x12),
            rate_change(day(2026, 8, 1), 60_000, contract(17_000)),
        ),
        (
            id(0x13),
            prepayment(day(2026, 1, 2), 50_000, PrepaymentMode::Shorten, 0),
        ),
    ];
    for (event_id, event) in events {
        fx.core
            .execute(sparagne_core::CommandEnvelope::with_id(
                event_id,
                fx.vault,
                "alice",
                event_cmd(loan_a, event),
            ))
            .unwrap();
    }
    for (event_id, event) in events.iter().rev() {
        fx.core
            .execute(sparagne_core::CommandEnvelope::with_id(
                *event_id,
                other,
                "alice",
                event_cmd(loan_b, *event),
            ))
            .unwrap();
    }
    assert_eq!(
        fx.core.loan_schedule(fx.vault, loan_a).unwrap(),
        fx.core.loan_schedule(other, loan_b).unwrap()
    );
    assert_eq!(
        loan_of(&fx, loan_a).events,
        fx.core.loans(other).unwrap()[0].events,
        "listed by date and id, not by insertion"
    );
}

#[test]
fn a_loan_of_another_vault_is_not_found_on_every_command() {
    let mut fx = setup();
    let loan = fx.id(loan_cmd("Cucina", ten_k(), None));
    let event = fx.id(event_cmd(
        loan,
        prepayment(day(2026, 3, 20), 100, PrepaymentMode::Shorten, 0),
    ));
    let (other, _) = other_vault(&mut fx.core);
    let mine = projection(&fx.core, fx.vault);
    let not_mine = projection(&fx.core, other);
    let cases = [
        (
            "update",
            patch_cmd(
                loan,
                LoanPatch {
                    name: Some("X".to_string()),
                    ..LoanPatch::default()
                },
            ),
        ),
        ("link", link_cmd(loan, None)),
        ("archive", Command::ArchiveLoan { loan_id: loan }),
        ("restore", Command::RestoreLoan { loan_id: loan }),
        (
            "add event",
            event_cmd(
                loan,
                prepayment(day(2026, 4, 20), 100, PrepaymentMode::Shorten, 0),
            ),
        ),
        ("remove event", remove_cmd(loan, event)),
    ];
    for (what, cmd) in cases {
        let err = try_run(&mut fx.core, other, cmd).expect_err(what);
        assert_eq!(err.code(), "not_found", "{what}: {err}");
        assert_eq!(projection(&fx.core, fx.vault), mine, "{what}");
        assert_eq!(projection(&fx.core, other), not_mine, "{what}");
    }
    assert!(fx.core.loans(other).unwrap().is_empty());
    assert!(fx.core.loan_schedule(other, loan).is_err());
}

#[test]
fn replaying_the_log_into_a_fresh_core_reproduces_the_loans_and_their_schedules() {
    let mut fx = setup();
    income_in(&mut fx, 10_000_000, secs(day(2025, 12, 1)));
    let template = mortgage_template(&mut fx, 20_038, day(2026, 1, 15), None);
    let loan = fx.id(loan_cmd("Cucina", ten_k(), Some(template)));
    let other = fx.id(Command::CreateLoan {
        name: "Mutuo".to_string(),
        note: Some("casa".to_string()),
        terms: mortgage(),
        recurring_id: None,
    });
    let gone = fx.id(event_cmd(
        loan,
        rate_change(day(2026, 6, 15), 50_000, LoanInstallment::Computed),
    ));
    fx.id(event_cmd(
        loan,
        prepayment(day(2026, 3, 20), 200_000, PrepaymentMode::Lower, 500),
    ));
    fx.run(remove_cmd(loan, gone));
    fx.id(event_cmd(
        other,
        prepayment(day(2027, 1, 1), 1_000_000, PrepaymentMode::Shorten, 0),
    ));
    fx.run(patch_cmd(
        other,
        LoanPatch {
            annual_rate_ppm: Some(35_000),
            installment_fee: Some(300),
            ..LoanPatch::default()
        },
    ));
    fx.run(execute_cmd(template, day(2026, 1, 15), Some(20_038)));
    fx.run(execute_cmd(template, day(2026, 2, 15), Some(20_038)));
    let archived = fx.id(loan_cmd(
        "Auto",
        terms(500_000, 20_000, 24, day(2026, 2, 1)),
        None,
    ));
    fx.run(Command::ArchiveLoan { loan_id: archived });
    fx.run(link_cmd(loan, None));
    fx.run(link_cmd(other, Some(template)));

    let original = projection(&fx.core, fx.vault);
    assert_eq!(original.loans.len(), 3);
    let fresh = replayed(&fx.core, fx.vault);
    assert_eq!(projection(&fresh, fx.vault), original);
    for today in [day(2026, 1, 1), day(2027, 7, 1), day(2060, 1, 1)] {
        assert_eq!(
            fresh.loan_overview(fx.vault, today).unwrap(),
            fx.core.loan_overview(fx.vault, today).unwrap(),
            "on {today}"
        );
    }
}

#[test]
fn two_members_adding_events_apart_end_with_the_same_schedule_after_the_rebase() {
    let mut server = Core::open_in_memory().unwrap();
    let (mut alice, vault) = vault_of("alice", "Casa");
    exec(&mut alice, vault, "alice", wallet_cmd("Cash", 0));
    let loan = exec(&mut alice, vault, "alice", loan_cmd("Mutuo", ten_k(), None));
    sync(&mut alice, &mut server, vault);
    let mut bob = Core::open_in_memory().unwrap();
    let joined = bob.integrate_pull(vault, &pull(&server, vault, 0)).unwrap();
    assert!(joined.rebased);
    assert_eq!(bob.loans(vault).unwrap(), alice.loans(vault).unwrap());

    // Apart: Alice a rate change, Bob a repayment and a rate change on the
    // same day as Alice's.
    exec(
        &mut alice,
        vault,
        "alice",
        event_cmd(
            loan,
            rate_change(day(2026, 6, 15), 50_000, LoanInstallment::Computed),
        ),
    );
    exec(
        &mut bob,
        vault,
        "bob",
        event_cmd(
            loan,
            prepayment(day(2026, 3, 20), 200_000, PrepaymentMode::Lower, 0),
        ),
    );
    exec(
        &mut bob,
        vault,
        "bob",
        event_cmd(
            loan,
            rate_change(day(2026, 6, 15), 40_000, LoanInstallment::Computed),
        ),
    );
    assert_ne!(
        alice.loan_schedule(vault, loan).unwrap(),
        bob.loan_schedule(vault, loan).unwrap()
    );

    // Bob pushes first; Alice's command lands after his and she rebases.
    let (bob_push, _) = sync(&mut bob, &mut server, vault);
    assert_eq!(bob_push.confirmed, 2);
    let (alice_push, alice_pull) = sync(&mut alice, &mut server, vault);
    assert_eq!(alice_push.confirmed, 1);
    assert!(alice_pull.rebased, "Alice must rebase under Bob's events");
    assert!(alice_pull.rejected.is_empty(), "{:?}", alice_pull.rejected);
    let (_, second) = sync(&mut bob, &mut server, vault);
    assert!(second.rebased);
    assert!(second.rejected.is_empty());

    let expected = server.loan_schedule(vault, loan).unwrap();
    let loans = server.loans(vault).unwrap();
    assert_eq!(loans[0].events.len(), 3);
    assert_eq!(
        expected,
        server.preview_loan(&ten_k(), &loans[0].events).unwrap(),
        "the schedule is the preview of the three events"
    );
    for (who, core) in [("alice", &alice), ("bob", &bob)] {
        assert_eq!(core.loan_schedule(vault, loan).unwrap(), expected, "{who}");
        assert_eq!(core.loans(vault).unwrap(), loans, "{who}");
        assert_eq!(
            core.loan_overview(vault, day(2027, 1, 1)).unwrap(),
            server.loan_overview(vault, day(2027, 1, 1)).unwrap(),
            "{who}"
        );
        let state = core.sync_state(vault).unwrap();
        assert_eq!((state.outbox, state.rejected), (0, 0), "{who}");
    }
    // Same day, two rate changes: the larger id (Alice's or Bob's, whichever
    // was made later) has the last word on every core.
    let same_day: Vec<&LoanEventView> = loans[0]
        .events
        .iter()
        .filter(|e| e.event.date() == day(2026, 6, 15))
        .collect();
    assert_eq!(same_day.len(), 2);
    assert!(same_day[0].id < same_day[1].id);
    let LoanEvent::RateChange {
        annual_rate_ppm, ..
    } = same_day[1].event
    else {
        panic!("a rate change")
    };
    assert_eq!(expected.rows[6].annual_rate_ppm, annual_rate_ppm);
}

// ---------------------------------------------------------------------------
// The recurring template
// ---------------------------------------------------------------------------

#[test]
fn an_execution_with_an_amount_records_that_amount_and_leaves_the_template_alone() {
    let mut fx = setup();
    income_in(&mut fx, 10_000_000, secs(day(2025, 12, 1)));
    let template = mortgage_template(&mut fx, 20_038, day(2026, 1, 15), None);

    let own = fx.id(execute_cmd(template, day(2026, 1, 15), Some(18_977)));
    assert_eq!(fx.core.transaction(fx.vault, own).unwrap().amount, 18_977);
    let templates = fx.core.list_recurring(fx.vault, false).unwrap();
    assert_eq!(templates[0].amount, 20_038, "the template keeps its amount");

    let default = fx.id(execute_cmd(template, day(2026, 2, 15), None));
    assert_eq!(
        fx.core.transaction(fx.vault, default).unwrap().amount,
        20_038
    );

    for (what, amount) in [("nothing", 0), ("a negative amount", -5)] {
        let err = refused(
            &mut fx,
            what,
            execute_cmd(template, day(2026, 3, 15), Some(amount)),
        );
        assert_eq!(err.code(), "invalid_amount", "{what}: {err}");
    }
    let runs = fx.core.recurring_runs(fx.vault, template).unwrap();
    assert_eq!(runs.len(), 2, "the refused period left no run");
    assert_eq!(runs[0].transaction_id, Some(own));

    // The amount travels in the log: a replay records it again.
    let fresh = replayed(&fx.core, fx.vault);
    assert_eq!(fresh.transaction(fx.vault, own).unwrap().amount, 18_977);
    assert_eq!(fresh.transaction(fx.vault, default).unwrap().amount, 20_038);
}

#[test]
fn linked_installments_say_what_each_due_date_should_be_recorded_with() {
    let mut fx = setup();
    let template = mortgage_template(&mut fx, 20_038, day(2026, 1, 15), None);
    let mut with_fee = ten_k();
    with_fee.installment_fee = 250;
    assert!(fx.core.linked_installments(fx.vault).unwrap().is_empty());

    let unlinked = fx.id(loan_cmd("Auto", mortgage(), None));
    assert!(
        fx.core.linked_installments(fx.vault).unwrap().is_empty(),
        "an unlinked loan records nothing"
    );
    let loan = fx.id(loan_cmd("Cucina", with_fee, Some(template)));
    let s = schedule_of(&fx, loan);
    let expected: Vec<LinkedInstallment> = s
        .rows
        .iter()
        .map(|r| LinkedInstallment {
            recurring_id: template,
            due_date: r.due_date,
            amount: r.payment + r.fee,
        })
        .collect();
    assert_eq!(fx.core.linked_installments(fx.vault).unwrap(), expected);
    assert_eq!(expected.len(), 60);
    assert_eq!(expected[0].amount, 20_038 + 250);
    assert_eq!(expected[59].amount, 20_035 + 250);

    // A rate change reaches the amounts; the template's own amount does not.
    fx.id(event_cmd(
        loan,
        rate_change(day(2026, 6, 15), 50_000, LoanInstallment::Computed),
    ));
    let linked = fx.core.linked_installments(fx.vault).unwrap();
    assert_eq!(linked[6].amount, 18_977 + 250);
    assert_eq!(linked[5].amount, 20_038 + 250);
    fx.run(Command::UpdateRecurring {
        recurring_id: template,
        patch: sparagne_core::RecurringPatch {
            amount: Some(1),
            ..Default::default()
        },
    });
    assert_eq!(fx.core.linked_installments(fx.vault).unwrap(), linked);

    // Unlinking, or archiving the loan, drops its rows.
    fx.run(link_cmd(loan, None));
    assert!(fx.core.linked_installments(fx.vault).unwrap().is_empty());
    fx.run(link_cmd(unlinked, Some(template)));
    assert_eq!(fx.core.linked_installments(fx.vault).unwrap().len(), 300);
    fx.run(Command::ArchiveLoan { loan_id: unlinked });
    assert!(fx.core.linked_installments(fx.vault).unwrap().is_empty());
}

#[test]
fn the_overview_reads_the_schedule_on_the_given_day() {
    let mut fx = setup();
    let loan = fx.id(loan_cmd("Cucina", ten_k(), None));
    let s = schedule_of(&fx, loan);

    let before = overview_of(&fx, loan, day(2026, 1, 14));
    assert_eq!(before.residual, 1_000_000);
    assert_eq!((before.paid_count, before.total_count), (0, 60));
    assert_eq!(before.next_due, Some(day(2026, 1, 15)));
    assert_eq!(before.next_payment, Some(20_038));
    assert_eq!((before.capital_repaid, before.interest_paid), (0, 0));
    assert_eq!(before.interest_left, 202_277);
    assert_eq!(before.end_date, Some(day(2030, 12, 15)));
    assert_eq!(before.taeg_ppm, s.taeg_ppm);
    assert_eq!(before.total_cost, 202_277);
    assert_eq!(before.registered_count, None, "no template linked");
    assert!(before.warnings.is_empty());

    // March 20th: three installments due, the third on the 15th.
    let march = overview_of(&fx, loan, day(2026, 3, 20));
    assert_eq!(march.residual, 958_377);
    assert_eq!(march.paid_count, 3);
    assert_eq!(march.next_due, Some(day(2026, 4, 15)));
    assert_eq!(march.next_payment, Some(20_038));
    assert_eq!(march.capital_repaid, 41_623);
    assert_eq!(march.interest_paid, 18_491);
    assert_eq!(march.interest_left, 202_277 - 18_491);
    assert_eq!(march.capital_repaid + march.residual, 1_000_000);

    // On a due date the installment counts as paid.
    let on_due = overview_of(&fx, loan, day(2026, 3, 15));
    assert_eq!(on_due.paid_count, 3);
    assert_eq!(on_due.residual, 958_377);

    let after = overview_of(&fx, loan, day(2031, 1, 1));
    assert_eq!(after.residual, 0);
    assert_eq!(after.paid_count, 60);
    assert_eq!((after.next_due, after.next_payment), (None, None));
    assert_eq!(after.capital_repaid, 1_000_000);
    assert_eq!(after.interest_paid, 202_277);
    assert_eq!(after.interest_left, 0);

    // With a fee the next payment carries it; the warnings are the schedule's.
    fx.run(patch_cmd(
        loan,
        LoanPatch {
            installment_fee: Some(250),
            installment: Some(contract(15_000)),
            ..LoanPatch::default()
        },
    ));
    let fee = overview_of(&fx, loan, day(2026, 1, 1));
    assert_eq!(fee.next_payment, Some(15_250));
    assert_eq!(fee.warnings, schedule_of(&fx, loan).warnings);
    assert!(
        fee.warnings
            .iter()
            .any(|w| matches!(w, LoanWarning::Balloon { .. }))
    );
    assert_eq!(fee.total_cost, schedule_of(&fx, loan).total_cost);
}

#[test]
fn registered_installments_are_the_live_executions_of_the_linked_template_from_the_first_due_on() {
    let mut fx = setup();
    income_in(&mut fx, 10_000_000, secs(day(2025, 11, 1)));
    // The template started two months before the loan: those runs are not
    // the loan's.
    let template = mortgage_template(&mut fx, 20_038, day(2025, 11, 15), None);
    let loan = fx.id(loan_cmd("Cucina", ten_k(), Some(template)));
    let today = day(2026, 4, 20);
    assert_eq!(overview_of(&fx, loan, today).registered_count, Some(0));

    fx.run(execute_cmd(template, day(2025, 12, 15), None));
    assert_eq!(
        overview_of(&fx, loan, today).registered_count,
        Some(0),
        "a period before the first due date is not an installment"
    );
    fx.run(execute_cmd(template, day(2026, 1, 15), Some(20_038)));
    assert_eq!(overview_of(&fx, loan, today).registered_count, Some(1));
    let voided = fx.id(execute_cmd(template, day(2026, 2, 15), Some(20_038)));
    assert_eq!(overview_of(&fx, loan, today).registered_count, Some(2));
    fx.run(Command::VoidTransaction {
        transaction_id: voided,
    });
    assert_eq!(
        overview_of(&fx, loan, today).registered_count,
        Some(1),
        "a voided transaction does not count"
    );
    fx.run(Command::SkipRecurring {
        recurring_id: template,
        period_date: day(2026, 3, 15),
    });
    let april = overview_of(&fx, loan, today);
    assert_eq!(
        april.registered_count,
        Some(1),
        "a skipped period is no installment"
    );
    assert_eq!(april.paid_count, 4, "while four are due by the date");

    // Registered counts the installments due by the day: one recorded ahead
    // of time waits for its due date, so the count never exceeds paid_count.
    fx.run(execute_cmd(template, day(2026, 4, 15), Some(20_038)));
    fx.run(execute_cmd(template, day(2026, 5, 15), Some(20_038)));
    assert_eq!(overview_of(&fx, loan, today).registered_count, Some(2));
    assert_eq!(
        overview_of(&fx, loan, day(2026, 5, 15)).registered_count,
        Some(3)
    );

    fx.run(link_cmd(loan, None));
    assert_eq!(overview_of(&fx, loan, today).registered_count, None);
    fx.run(link_cmd(loan, Some(template)));
    assert_eq!(overview_of(&fx, loan, today).registered_count, Some(2));
}
