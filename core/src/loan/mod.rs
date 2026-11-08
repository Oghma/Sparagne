//! Loans and mortgages: a debt repaid by a French amortization (a constant
//! installment, the monthly rate being the yearly TAN over twelve, interest
//! rounded to the cent on each installment, the last installment paying what
//! is left). A loan never touches a wallet: its installments are expenses the
//! ledger records like any other, usually through the recurring template the
//! loan is linked to. What the loan knows is the debt: how much is left and
//! how each installment splits between capital and interest.
//!
//! The commands carry what the user said, the terms and the events (a rate
//! change, an early repayment); the schedule is worked out again at every
//! read by [`math`], with integer arithmetic only, so editing a term or
//! adding an event dated in the past redraws the whole schedule. Anything
//! that depends on the schedule (an installment that never covers the
//! interest, a final balloon, a repayment larger than the debt) is a
//! [`LoanWarning`], never a refusal: a refusal would depend on the order a
//! rebase gives the events.

pub(crate) mod math;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The yearly rate in millionths: 3.125% is `31_250`, 100% is `1_000_000`.
pub const FULL_RATE_PPM: u32 = 1_000_000;
/// The highest yearly rate a loan accepts, 50%.
pub const MAX_RATE_PPM: u32 = 500_000;
/// The longest loan, in monthly installments: 50 years.
pub const MAX_MONTHS: u32 = 600;

/// How the installment is set, for the loan or for the stretch after a rate
/// change. Stored as data inside the loan's JSON, tagged by `installment`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "installment", rename_all = "snake_case")]
pub enum LoanInstallment {
    /// Worked out by the amortization formula on the residual debt.
    Computed,
    /// The amount the contract or the bank's letter states, minor units,
    /// `> 0`. Above the formula the loan ends early with a smaller last
    /// installment; below it a balloon is left at the end (a warning).
    Contract { amount: i64 },
}

/// What an early repayment keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum PrepaymentMode {
    /// Keeps the installment: the loan ends earlier.
    Shorten,
    /// Keeps the end: the installment is worked out again on the residual.
    Lower,
}

/// Something that changes a loan's schedule from a date on. Tagged by
/// `event` in the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum LoanEvent {
    /// A new yearly rate for the installments due after `date` (the one due
    /// on `date` accrued at the old rate), the remaining number of
    /// installments unchanged.
    RateChange {
        date: NaiveDate,
        annual_rate_ppm: u32,
        installment: LoanInstallment,
    },
    /// An early repayment of `amount` minor units on `date`, applied after
    /// the last installment due on or before it (to the principal when it
    /// comes before the first one), clipped to the residual: as much as the
    /// residual pays the loan off. `fee` is the penalty, a cost only.
    Prepayment {
        date: NaiveDate,
        amount: i64,
        mode: PrepaymentMode,
        fee: i64,
    },
}

impl LoanEvent {
    /// The day the event takes effect.
    #[must_use]
    pub const fn date(&self) -> NaiveDate {
        match self {
            Self::RateChange { date, .. } | Self::Prepayment { date, .. } => *date,
        }
    }
}

/// What a loan was agreed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct LoanTerms {
    /// The amount borrowed, minor units, `> 0`.
    pub principal: i64,
    /// The TAN, yearly, in millionths: `0..=MAX_RATE_PPM`.
    pub annual_rate_ppm: u32,
    /// Monthly installments: `1..=MAX_MONTHS`.
    pub months: u32,
    /// The first installment's due date; the next ones fall on the same day
    /// of each following month, the month's last day when it is shorter.
    pub first_due: NaiveDate,
    pub installment: LoanInstallment,
    /// Paid once when the loan is signed (application, survey, substitute
    /// tax…), minor units, `>= 0`: they count in the TAEG and in the cost.
    #[serde(default)]
    #[uniffi(default = 0)]
    pub upfront_fees: i64,
    /// Paid with every installment (collection fee), minor units, `>= 0`.
    #[serde(default)]
    #[uniffi(default = 0)]
    pub installment_fee: i64,
}

/// The fields `UpdateLoan` can change. `None` leaves the field as it is; a
/// blank `name` is refused, a blank `note` clears it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
#[serde(default)]
pub struct LoanPatch {
    #[uniffi(default = None)]
    pub name: Option<String>,
    #[uniffi(default = None)]
    pub note: Option<String>,
    #[uniffi(default = None)]
    pub principal: Option<i64>,
    #[uniffi(default = None)]
    pub annual_rate_ppm: Option<u32>,
    #[uniffi(default = None)]
    pub months: Option<u32>,
    #[uniffi(default = None)]
    pub first_due: Option<NaiveDate>,
    #[uniffi(default = None)]
    pub installment: Option<LoanInstallment>,
    #[uniffi(default = None)]
    pub upfront_fees: Option<i64>,
    #[uniffi(default = None)]
    pub installment_fee: Option<i64>,
}

impl LoanPatch {
    /// A patch that carries no field at all changes nothing.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.note.is_none()
            && self.principal.is_none()
            && self.annual_rate_ppm.is_none()
            && self.months.is_none()
            && self.first_due.is_none()
            && self.installment.is_none()
            && self.upfront_fees.is_none()
            && self.installment_fee.is_none()
    }
}

/// One event of a loan with its id, the id of the `AddLoanEvent` that made
/// it: a v7 id, so ids order events made on the same day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct LoanEventView {
    pub id: Uuid,
    pub event: LoanEvent,
}

/// A loan as the UI sees it.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LoanView {
    pub id: Uuid,
    pub name: String,
    pub note: Option<String>,
    pub terms: LoanTerms,
    /// By date, then by id.
    pub events: Vec<LoanEventView>,
    /// The recurring template that records the installments, if linked.
    pub recurring_id: Option<Uuid>,
    pub archived: bool,
    pub created_by: String,
}

/// One installment of a schedule.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LoanScheduleRow {
    /// From 1.
    pub number: u32,
    pub due_date: NaiveDate,
    /// Interest + capital, minor units, the fee excluded.
    pub payment: i64,
    pub interest: i64,
    pub capital: i64,
    /// The installment fee paid with it.
    pub fee: i64,
    /// The debt left once this installment and the events after it are
    /// applied.
    pub residual: i64,
    /// The yearly rate this installment's interest was worked out at.
    pub annual_rate_ppm: u32,
    /// The events applied right after this installment, in order.
    pub event_ids: Vec<Uuid>,
}

/// Something in a schedule that is likely a typing mistake or worth a look.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LoanWarning {
    /// From installment `number` on, the installment does not cover the
    /// interest: the debt would never go down.
    InstallmentBelowInterest { number: u32 },
    /// A contract installment below the formula leaves `amount` to pay with
    /// the last installment.
    Balloon { amount: i64 },
    /// An early repayment larger than the residual was cut to it.
    PrepaymentClipped { event_id: Uuid },
    /// An event dated after the loan's end (a shortened one, often).
    EventAfterEnd { event_id: Uuid },
}

/// A loan's whole schedule, its events applied.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LoanSchedule {
    pub rows: Vec<LoanScheduleRow>,
    /// Early repayments applied before the first installment.
    pub event_ids_before_first: Vec<Uuid>,
    pub total_interest: i64,
    /// Every installment fee.
    pub total_fees: i64,
    /// Early repayments, as applied (clipped).
    pub total_prepaid: i64,
    /// Early repayment penalties.
    pub total_penalties: i64,
    /// Installments + upfront fees + installment fees + repayments +
    /// penalties − principal: what the loan costs on top of what was
    /// borrowed, events included.
    pub total_cost: i64,
    /// The TAEG of the terms alone (EU Directive 2008/48: a standard year of
    /// twelve equal months, the rate held for the whole loan), in millionths;
    /// `None` when it cannot be worked out (a schedule that never ends).
    pub taeg_ppm: Option<u32>,
    pub warnings: Vec<LoanWarning>,
}

/// A loan on a given day: what the Patrimonio tab lists.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LoanOverview {
    pub loan_id: Uuid,
    /// The debt after every installment due on or before the day.
    pub residual: i64,
    /// Installments due on or before the day.
    pub paid_count: u32,
    /// Installments of the schedule.
    pub total_count: u32,
    /// The next installment after the day, with its fee.
    pub next_due: Option<NaiveDate>,
    pub next_payment: Option<i64>,
    pub capital_repaid: i64,
    pub interest_paid: i64,
    pub interest_left: i64,
    /// The last installment's due date.
    pub end_date: Option<NaiveDate>,
    pub taeg_ppm: Option<u32>,
    pub total_cost: i64,
    /// Due installments on or after `first_due` the linked template recorded
    /// with a live transaction; `None` without a linked template.
    pub registered_count: Option<u32>,
    pub warnings: Vec<LoanWarning>,
}

/// What the linked template should record for one due date: the schedule's
/// payment plus the installment fee.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LinkedInstallment {
    pub recurring_id: Uuid,
    pub due_date: NaiveDate,
    pub amount: i64,
}
