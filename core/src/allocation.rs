//! Allocation plan: how the money that reaches Unallocated is shared out among
//! the envelopes, one schedule period at a time.
//!
//! A plan is an ordered list of lines, envelope → rule, and the order is the
//! priority: when the total runs short, the last lines get less or nothing.
//! Nothing is ever moved automatically. The app asks
//! `Core::pending_allocation` for the period waiting for a decision, shows
//! [`resolve`]'s answer (through `Core::preview_allocation`) on the total of
//! `Core::allocation_base`, and the user either sends `ExecuteAllocation` with
//! the amounts resolved or skips the period.
//!
//! The command carries amounts, never rules: replaying the log gives the same
//! transfers whatever the plan says by then.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{FlowView, RunOutcome, Schedule, TransactionView};

/// Basis points in 100%: a [`AllocationRule::Percent`] of `FULL_PERCENT_BP`
/// takes the whole total.
pub const FULL_PERCENT_BP: u32 = 10_000;

/// What one line of a plan asks for its envelope. Stored as data inside the
/// plan's JSON, tagged by `rule`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum AllocationRule {
    /// The same amount every period, in minor units, `> 0`.
    Fixed { amount: i64 },
    /// A share of the whole total, in basis points: `1..=10000`, so 12.5% is
    /// 1250. Rounded down to the minor unit.
    Percent { basis_points: u32 },
    /// Whatever brings the envelope up to its cap: the room left under a
    /// net cap, or under an income cap. An envelope without a cap gets
    /// nothing.
    FillToCap,
}

/// One line of a plan: an envelope and what it asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct AllocationLine {
    pub flow_id: Uuid,
    pub rule: AllocationRule,
}

/// One transfer out of Unallocated, as `ExecuteAllocation` carries it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct AllocationMove {
    pub flow_id: Uuid,
    /// Minor units, `> 0`.
    pub amount: i64,
}

/// The fields `UpdateAllocationPlan` can change. `None` leaves the field as
/// it is; `lines` replaces the whole list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
#[serde(default)]
pub struct AllocationPlanPatch {
    #[uniffi(default = None)]
    pub schedule: Option<Schedule>,
    #[uniffi(default = None)]
    pub lines: Option<Vec<AllocationLine>>,
    /// A disabled plan has no period due.
    #[uniffi(default = None)]
    pub enabled: Option<bool>,
}

impl AllocationPlanPatch {
    /// A patch that carries no field at all changes nothing.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.schedule.is_none() && self.lines.is_none() && self.enabled.is_none()
    }
}

// ---------------------------------------------------------------------------
// Read-side views
// ---------------------------------------------------------------------------

/// The vault's plan as the UI sees it.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AllocationPlanView {
    pub id: Uuid,
    pub schedule: Schedule,
    /// In priority order.
    pub lines: Vec<AllocationLine>,
    pub enabled: bool,
    pub created_by: String,
}

/// The period of the plan waiting for a decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PendingAllocation {
    pub plan_id: Uuid,
    /// The most recent due period: the one to execute or skip.
    pub period_date: NaiveDate,
    /// Older due periods with no decision. Deciding `period_date` closes them
    /// too, and the incomes they saw are already in the total.
    pub missed: u32,
}

/// What the next execution shares out: the incomes that reached Unallocated
/// since the plan's last decision.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AllocationBase {
    /// The sum of `incomes`.
    pub total: i64,
    /// Oldest first.
    pub incomes: Vec<TransactionView>,
}

/// How a line of a preview came out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LineStatus {
    /// It gets all it asks for; an envelope already full asks for nothing.
    Full,
    /// The cap stopped it: it gets the room left.
    CapLimited,
    /// The total ran out first: it gets what was left, possibly nothing.
    Short,
    /// A fill-to-cap line on an envelope without a cap: nothing.
    NoCap,
    /// The envelope is archived (or gone): nothing.
    Archived,
}

/// One line of a preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PreviewLine {
    pub flow_id: Uuid,
    pub rule: AllocationRule,
    /// What the rule asks for before the cap and the total.
    pub wanted: i64,
    /// Room under the cap, never negative; `None` without a cap.
    pub room: Option<i64>,
    /// What the envelope gets.
    pub amount: i64,
    /// The envelope's balance once `amount` is in.
    pub balance_after: i64,
    pub status: LineStatus,
}

/// A plan worked out on a total, line by line, in priority order.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AllocationPreview {
    pub total: i64,
    pub lines: Vec<PreviewLine>,
    /// The sum of the amounts.
    pub distributed: i64,
    /// `total - distributed`: what stays in Unallocated.
    pub remainder: i64,
    /// Unallocated's balance once `distributed` is out. Negative when the
    /// total was raised above what Unallocated holds: a warning, not a
    /// refusal.
    pub unallocated_after: i64,
    /// The percent lines added up, in basis points; above
    /// [`FULL_PERCENT_BP`] the plan asks for more than the total.
    pub percent_total_bp: u32,
}

/// One transfer of an executed period.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RunMove {
    pub flow_id: Uuid,
    pub amount: i64,
    pub transaction_id: Uuid,
    /// Deleted from the ledger since.
    pub voided: bool,
}

/// One decided period of the plan.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AllocationRunView {
    pub period_date: NaiveDate,
    pub outcome: RunOutcome,
    /// The total the moves were worked out on; 0 for a skipped period.
    pub total: i64,
    /// Empty for a skipped period.
    pub moves: Vec<RunMove>,
    pub created_by: String,
}

// ---------------------------------------------------------------------------
// Resolver
// ---------------------------------------------------------------------------

/// Works `lines` out on `total`, in order, against the envelopes in `flows`
/// (the vault's [`FlowView`]s, in any order) and Unallocated's
/// `unallocated_balance`. Pure: the same inputs always give the same preview.
///
/// A stub for now.
#[must_use]
pub fn resolve(
    total: i64,
    lines: &[AllocationLine],
    flows: &[FlowView],
    unallocated_balance: i64,
) -> AllocationPreview {
    let _ = (lines, flows);
    AllocationPreview {
        total,
        lines: Vec::new(),
        distributed: 0,
        remainder: total,
        unallocated_after: unallocated_balance,
        percent_total_bp: 0,
    }
}
