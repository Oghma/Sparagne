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

use std::collections::HashMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{FlowView, RunOutcome, Schedule, TransactionView, flow::headroom};

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
/// A cascade: each line gets the least of what its rule asks, the room under
/// its envelope's cap and what the lines before it left of `total`. The room
/// is [`headroom`], the very bound the engine checks on the transfer, so an
/// execution of this preview is never refused for a cap. A line on an
/// archived or unknown envelope, or on Unallocated, gets nothing, and so does
/// a fill-to-cap line on an envelope without a cap: neither uses up any of the
/// total. Every line is weighed against its envelope as `flows` has it, since
/// a saved plan names each envelope once.
///
/// Any input is taken without a panic, an unsaved draft's too: an ask below
/// zero (a negative total, a draft's negative amount) counts as nothing, and
/// the sums saturate.
#[must_use]
pub fn resolve(
    total: i64,
    lines: &[AllocationLine],
    flows: &[FlowView],
    unallocated_balance: i64,
) -> AllocationPreview {
    let by_id: HashMap<Uuid, &FlowView> = flows.iter().map(|flow| (flow.id, flow)).collect();
    let mut remaining = total.max(0);
    let mut distributed: i64 = 0;
    let mut percent_total_bp: u32 = 0;
    let mut resolved = Vec::with_capacity(lines.len());
    for line in lines {
        if let AllocationRule::Percent { basis_points } = line.rule {
            percent_total_bp = percent_total_bp.saturating_add(basis_points);
        }
        let preview = resolve_line(*line, by_id.get(&line.flow_id).copied(), total, remaining);
        remaining = remaining.saturating_sub(preview.amount);
        distributed = distributed.saturating_add(preview.amount);
        resolved.push(preview);
    }
    AllocationPreview {
        total,
        lines: resolved,
        distributed,
        remainder: total.saturating_sub(distributed),
        unallocated_after: unallocated_balance.saturating_sub(distributed),
        percent_total_bp,
    }
}

/// One line of the cascade, with `remaining` (never negative) still to share.
fn resolve_line(
    line: AllocationLine,
    flow: Option<&FlowView>,
    total: i64,
    remaining: i64,
) -> PreviewLine {
    let nothing = |balance: i64, status: LineStatus| PreviewLine {
        flow_id: line.flow_id,
        rule: line.rule,
        wanted: 0,
        room: None,
        amount: 0,
        balance_after: balance,
        status,
    };
    let Some(envelope) = flow.filter(|f| !f.archived && !f.is_unallocated) else {
        return nothing(flow.map_or(0, |f| f.balance), LineStatus::Archived);
    };
    let room = headroom(envelope.mode, envelope.balance, envelope.income_total).map(|r| r.max(0));
    let wanted = match line.rule {
        AllocationRule::Fixed { amount } => amount,
        AllocationRule::Percent { basis_points } => share(total, basis_points),
        AllocationRule::FillToCap => match room {
            Some(room) => room,
            None => return nothing(envelope.balance, LineStatus::NoCap),
        },
    }
    .max(0);
    let amount = wanted.min(room.unwrap_or(i64::MAX)).min(remaining).max(0);
    let status = if amount == wanted {
        LineStatus::Full
    } else if room == Some(amount) {
        LineStatus::CapLimited
    } else {
        LineStatus::Short
    };
    PreviewLine {
        flow_id: line.flow_id,
        rule: line.rule,
        wanted,
        room,
        amount,
        balance_after: envelope.balance.saturating_add(amount),
        status,
    }
}

/// `basis_points` of `total`, rounded down to the minor unit. The product
/// fits an `i128` for any input; only the quotient may need clamping.
fn share(total: i64, basis_points: u32) -> i64 {
    let exact = i128::from(total) * i128::from(basis_points);
    let floor = exact.div_euclid(i128::from(FULL_PERCENT_BP));
    i64::try_from(floor).unwrap_or(if floor < 0 { i64::MIN } else { i64::MAX })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FlowMode;

    fn id(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn envelope(n: u8, balance: i64, mode: FlowMode) -> FlowView {
        FlowView {
            id: id(n),
            name: format!("envelope {n}"),
            balance,
            mode,
            income_total: match mode {
                FlowMode::IncomeCapped { .. } => Some(balance.max(0)),
                _ => None,
            },
            allow_negative: false,
            archived: false,
            is_unallocated: false,
        }
    }

    fn line(n: u8, rule: AllocationRule) -> AllocationLine {
        AllocationLine {
            flow_id: id(n),
            rule,
        }
    }

    fn fixed(n: u8, amount: i64) -> AllocationLine {
        line(n, AllocationRule::Fixed { amount })
    }

    fn percent(n: u8, basis_points: u32) -> AllocationLine {
        line(n, AllocationRule::Percent { basis_points })
    }

    fn fill(n: u8) -> AllocationLine {
        line(n, AllocationRule::FillToCap)
    }

    /// `(amount, status)` of every line.
    fn outcome(preview: &AllocationPreview) -> Vec<(i64, LineStatus)> {
        preview.lines.iter().map(|l| (l.amount, l.status)).collect()
    }

    /// Rent 800, savings 10%, groceries up to their cap, fun 5%.
    fn household() -> (Vec<AllocationLine>, Vec<FlowView>) {
        let lines = vec![fixed(1, 80_000), percent(2, 1000), fill(3), percent(4, 500)];
        let flows = vec![
            envelope(1, 0, FlowMode::Unlimited),
            envelope(2, 120_000, FlowMode::Unlimited),
            envelope(3, 15_000, FlowMode::NetCapped { cap: 40_000 }),
            envelope(4, 2_000, FlowMode::Unlimited),
        ];
        (lines, flows)
    }

    #[test]
    fn the_lines_take_their_share_in_order() {
        let (lines, flows) = household();
        let preview = resolve(200_000, &lines, &flows, 250_000);

        assert_eq!(
            outcome(&preview),
            vec![
                (80_000, LineStatus::Full),
                (20_000, LineStatus::Full),
                (25_000, LineStatus::Full),
                (10_000, LineStatus::Full),
            ]
        );
        let groceries = preview.lines[2];
        assert_eq!(groceries.wanted, 25_000);
        assert_eq!(groceries.room, Some(25_000));
        assert_eq!(groceries.balance_after, 40_000);
        assert_eq!(preview.lines[0].room, None);
        assert_eq!(preview.lines[1].balance_after, 140_000);
        assert_eq!(preview.total, 200_000);
        assert_eq!(preview.distributed, 135_000);
        assert_eq!(preview.remainder, 65_000);
        assert_eq!(preview.unallocated_after, 115_000);
        assert_eq!(preview.percent_total_bp, 1500);
    }

    #[test]
    fn a_short_total_leaves_the_last_lines_with_less_or_nothing() {
        let (lines, flows) = household();
        let preview = resolve(100_000, &lines, &flows, 100_000);

        assert_eq!(
            outcome(&preview),
            vec![
                (80_000, LineStatus::Full),
                (10_000, LineStatus::Full),
                (10_000, LineStatus::Short),
                (0, LineStatus::Short),
            ]
        );
        assert_eq!(preview.lines[2].wanted, 25_000);
        assert_eq!(preview.lines[3].wanted, 5_000);
        assert_eq!(preview.distributed, 100_000);
        assert_eq!(preview.remainder, 0);
        assert_eq!(preview.unallocated_after, 0);
    }

    #[test]
    fn the_cap_stops_a_line_and_the_excess_stays_out() {
        let flows = vec![
            envelope(1, 10_000, FlowMode::NetCapped { cap: 30_000 }),
            // Spending does not free room under an income cap.
            FlowView {
                balance: 1_000,
                income_total: Some(8_000),
                ..envelope(2, 0, FlowMode::IncomeCapped { cap: 10_000 })
            },
            envelope(3, 0, FlowMode::Unlimited),
        ];
        let lines = vec![fixed(1, 50_000), fixed(2, 5_000), fixed(3, 1_000)];
        let preview = resolve(100_000, &lines, &flows, 100_000);

        assert_eq!(
            outcome(&preview),
            vec![
                (20_000, LineStatus::CapLimited),
                (2_000, LineStatus::CapLimited),
                (1_000, LineStatus::Full),
            ]
        );
        assert_eq!(preview.lines[0].balance_after, 30_000);
        assert_eq!(preview.lines[1].room, Some(2_000));
        assert_eq!(preview.remainder, 77_000);
    }

    #[test]
    fn the_cap_wins_over_a_short_total_when_both_stop_a_line() {
        let flows = vec![envelope(1, 0, FlowMode::NetCapped { cap: 500 })];
        let preview = resolve(500, &[fixed(1, 800)], &flows, 500);
        assert_eq!(outcome(&preview), vec![(500, LineStatus::CapLimited)]);
    }

    #[test]
    fn an_envelope_already_full_asks_for_nothing() {
        let flows = vec![
            envelope(1, 40_000, FlowMode::NetCapped { cap: 40_000 }),
            envelope(2, 40_000, FlowMode::NetCapped { cap: 40_000 }),
        ];
        let preview = resolve(10_000, &[fill(1), fixed(2, 100)], &flows, 10_000);

        assert_eq!(
            outcome(&preview),
            vec![(0, LineStatus::Full), (0, LineStatus::CapLimited)]
        );
        assert_eq!(preview.lines[0].wanted, 0);
        assert_eq!(preview.lines[1].room, Some(0));
        assert_eq!(preview.remainder, 10_000);
    }

    #[test]
    fn a_negative_balance_leaves_more_room_than_the_cap() {
        let flows = vec![FlowView {
            allow_negative: true,
            ..envelope(1, -500, FlowMode::NetCapped { cap: 1_000 })
        }];
        let preview = resolve(5_000, &[fill(1)], &flows, 5_000);
        assert_eq!(outcome(&preview), vec![(1_500, LineStatus::Full)]);
        assert_eq!(preview.lines[0].balance_after, 1_000);
    }

    #[test]
    fn lines_without_a_usable_envelope_get_nothing_and_use_nothing() {
        let unallocated = FlowView {
            is_unallocated: true,
            allow_negative: true,
            ..envelope(5, 9_000, FlowMode::Unlimited)
        };
        let archived = FlowView {
            archived: true,
            ..envelope(1, 300, FlowMode::NetCapped { cap: 1_000 })
        };
        let flows = vec![
            archived,
            envelope(2, 0, FlowMode::Unlimited),
            envelope(3, 0, FlowMode::Unlimited),
            unallocated,
        ];
        let lines = vec![
            fixed(1, 400),
            fill(2),
            percent(9, 5000),
            fixed(5, 100),
            fixed(3, 1_000),
        ];
        let preview = resolve(1_000, &lines, &flows, 9_000);

        assert_eq!(
            outcome(&preview),
            vec![
                (0, LineStatus::Archived),
                (0, LineStatus::NoCap),
                (0, LineStatus::Archived),
                (0, LineStatus::Archived),
                (1_000, LineStatus::Full),
            ]
        );
        for skipped in &preview.lines[..4] {
            assert_eq!(skipped.wanted, 0);
            assert_eq!(skipped.room, None);
        }
        assert_eq!(preview.lines[0].balance_after, 300);
        assert_eq!(preview.lines[2].balance_after, 0);
        assert_eq!(preview.lines[3].balance_after, 9_000);
        // A percent line still counts as asked, whatever its envelope.
        assert_eq!(preview.percent_total_bp, 5000);
        assert_eq!(preview.remainder, 0);
    }

    #[test]
    fn percents_round_down_and_the_cents_left_stay_out() {
        let flows: Vec<FlowView> = (1..=3)
            .map(|n| envelope(n, 0, FlowMode::Unlimited))
            .collect();
        let lines = vec![percent(1, 3333), percent(2, 3333), percent(3, 3334)];
        let preview = resolve(100, &lines, &flows, 100);
        assert_eq!(
            outcome(&preview),
            vec![
                (33, LineStatus::Full),
                (33, LineStatus::Full),
                (33, LineStatus::Full),
            ]
        );
        assert_eq!(preview.remainder, 1);
        assert_eq!(preview.percent_total_bp, FULL_PERCENT_BP);

        // 12.5% of 9.99 is 1.24875.
        let preview = resolve(999, &[percent(1, 1250)], &flows, 999);
        assert_eq!(preview.lines[0].amount, 124);
    }

    #[test]
    fn percents_are_of_the_whole_total_not_of_what_is_left() {
        let flows: Vec<FlowView> = (1..=2)
            .map(|n| envelope(n, 0, FlowMode::Unlimited))
            .collect();
        let preview = resolve(1_000, &[fixed(1, 600), percent(2, 5000)], &flows, 1_000);
        assert_eq!(
            outcome(&preview),
            vec![(600, LineStatus::Full), (400, LineStatus::Short)]
        );
        assert_eq!(preview.lines[1].wanted, 500);
    }

    #[test]
    fn a_total_above_unallocated_leaves_it_below_zero() {
        let flows = vec![envelope(1, 0, FlowMode::Unlimited)];
        let preview = resolve(5_000, &[fixed(1, 5_000)], &flows, 1_000);
        assert_eq!(preview.unallocated_after, -4_000);
    }

    #[test]
    fn asks_below_zero_count_as_nothing() {
        let flows: Vec<FlowView> = (1..=2)
            .map(|n| envelope(n, 0, FlowMode::Unlimited))
            .collect();
        let draft = resolve(1_000, &[fixed(1, -50), fixed(2, 0)], &flows, 1_000);
        assert_eq!(
            outcome(&draft),
            vec![(0, LineStatus::Full), (0, LineStatus::Full)]
        );
        assert_eq!(draft.remainder, 1_000);

        let negative = resolve(-1_000, &[percent(1, 5000), fixed(2, 10)], &flows, 0);
        assert_eq!(
            outcome(&negative),
            vec![(0, LineStatus::Full), (0, LineStatus::Short)]
        );
        assert_eq!(negative.distributed, 0);
        assert_eq!(negative.remainder, -1_000);
    }

    #[test]
    fn the_order_of_the_envelopes_does_not_matter() {
        let (lines, mut flows) = household();
        let forward = resolve(150_000, &lines, &flows, 150_000);
        flows.reverse();
        assert_eq!(resolve(150_000, &lines, &flows, 150_000), forward);
    }

    #[test]
    fn no_lines_share_nothing() {
        let preview = resolve(700, &[], &[], 900);
        assert!(preview.lines.is_empty());
        assert_eq!(preview.distributed, 0);
        assert_eq!(preview.remainder, 700);
        assert_eq!(preview.unallocated_after, 900);
        assert_eq!(preview.percent_total_bp, 0);
    }

    #[test]
    fn extreme_inputs_never_panic() {
        let flows = vec![
            envelope(1, i64::MIN, FlowMode::NetCapped { cap: i64::MAX }),
            envelope(2, i64::MAX, FlowMode::Unlimited),
            FlowView {
                income_total: Some(0),
                ..envelope(3, i64::MIN, FlowMode::IncomeCapped { cap: i64::MAX })
            },
            envelope(4, 0, FlowMode::NetCapped { cap: 1 }),
        ];
        let lines = vec![
            fill(1),
            percent(2, u32::MAX),
            fixed(3, i64::MAX),
            fixed(4, i64::MAX),
            percent(9, u32::MAX),
        ];
        for total in [i64::MAX, i64::MIN, 0, 1] {
            for balance in [i64::MAX, i64::MIN] {
                let preview = resolve(total, &lines, &flows, balance);
                assert!(preview.lines.iter().all(|l| l.amount >= 0));
                assert!(preview.lines.iter().all(|l| l.amount <= l.wanted));
                assert!(preview.distributed <= total.max(0));
                assert_eq!(preview.percent_total_bp, u32::MAX);
            }
        }
        let full = resolve(i64::MAX, &lines, &flows, 0);
        assert_eq!(full.distributed, i64::MAX);
        assert_eq!(full.lines[0].room, Some(i64::MAX));
        assert_eq!(full.lines[1].wanted, i64::MAX);
        assert_eq!(full.remainder, 0);
    }
}
