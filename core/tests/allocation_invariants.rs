//! Invariants of the allocation plan, checked on generated plans and
//! envelopes: the resolver against a cascade written again from the spec,
//! and the engine's conservation, ordering, atomicity, replay, base and
//! reopen rules through the commands.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone};
use common::*;
use sparagne_core::{
    AllocationBase, AllocationLine, AllocationPlanPatch, AllocationPlanView, AllocationPreview,
    AllocationRule, AllocationRunView, Command, CommandEnvelope, Core, Currency, DomainError,
    FlowMode, FlowView, LineStatus, PendingAllocation, PreviewLine, RunOutcome, TransactionFilter,
    TransactionKind, TransactionPatch, TransactionView, VaultSnapshot,
    allocation::{FULL_PERCENT_BP, resolve},
    headroom, replay,
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Generator
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

    /// `n` parts, each `>= 1`, adding up to `sum` (`sum >= n`).
    fn partition(&mut self, sum: i64, n: usize) -> Vec<i64> {
        let mut parts = vec![1; n];
        let mut left = sum - i64::try_from(n).unwrap();
        for part in parts.iter_mut().take(n - 1) {
            let take = self.range(0, left);
            *part += take;
            left -= take;
        }
        *parts.last_mut().unwrap() += left;
        parts
    }
}

// ---------------------------------------------------------------------------
// Synthetic envelopes for the resolver
// ---------------------------------------------------------------------------

fn view(
    id: Uuid,
    balance: i64,
    mode: FlowMode,
    income_total: Option<i64>,
    allow_negative: bool,
) -> FlowView {
    FlowView {
        id,
        name: format!("flow {id}"),
        balance,
        mode,
        income_total,
        allow_negative,
        archived: false,
        is_unallocated: false,
    }
}

/// An envelope in a state the engine could have written: a net-capped one
/// within its cap (below zero only with `allow_negative`), an income-capped
/// one with its income within the cap and its balance within the income.
fn envelope_view(g: &mut Gen, index: usize) -> FlowView {
    let archived = g.chance(10);
    let allow_negative = g.chance(25);
    let floor = |cap: i64| if allow_negative { -cap } else { 0 };
    let (mode, income_total, balance) = match g.below(3) {
        0 => (FlowMode::Unlimited, None, g.range(0, 50_000)),
        1 => {
            let cap = g.range(1, 50_000);
            (FlowMode::NetCapped { cap }, None, g.range(floor(cap), cap))
        }
        _ => {
            let cap = g.range(1, 50_000);
            let income = g.range(0, cap);
            let balance = g.range(floor(cap), income);
            (FlowMode::IncomeCapped { cap }, Some(income), balance)
        }
    };
    let mut flow = view(
        g.uuid(),
        if archived { 0 } else { balance },
        mode,
        income_total,
        allow_negative,
    );
    flow.name = format!("Envelope {index}");
    flow.archived = archived;
    flow
}

struct World {
    /// The vault's flows in a random order, Unallocated among them.
    flows: Vec<FlowView>,
    unallocated: i64,
}

fn world(g: &mut Gen) -> World {
    let n = g.index(6) + 1;
    let mut flows: Vec<FlowView> = (0..n).map(|i| envelope_view(g, i)).collect();
    let unallocated = g.range(-10_000, 200_000);
    let mut system = view(g.uuid(), unallocated, FlowMode::Unlimited, None, true);
    system.name = "unallocated".to_string();
    system.is_unallocated = true;
    flows.push(system);
    g.shuffle(&mut flows);
    World { flows, unallocated }
}

fn rule(g: &mut Gen) -> AllocationRule {
    match g.below(3) {
        0 => AllocationRule::Fixed {
            amount: g.range(1, 20_000),
        },
        1 => AllocationRule::Percent {
            basis_points: u32::try_from(g.range(1, 10_000)).unwrap(),
        },
        _ => AllocationRule::FillToCap,
    }
}

/// A plan over distinct envelopes of the world, Unallocated and unknown
/// ids included now and then.
fn lines(g: &mut Gen, flows: &[FlowView]) -> Vec<AllocationLine> {
    let mut ids: Vec<Uuid> = flows.iter().map(|f| f.id).collect();
    g.shuffle(&mut ids);
    let n = g.index(ids.len()) + 1;
    ids.truncate(n);
    ids.into_iter()
        .map(|id| AllocationLine {
            flow_id: if g.chance(5) { g.uuid() } else { id },
            rule: rule(g),
        })
        .collect()
}

fn total(g: &mut Gen) -> i64 {
    match g.below(10) {
        0 => 0,
        1 => g.range(0, 500),
        2..=7 => g.range(0, 60_000),
        _ => g.range(0, 2_000_000),
    }
}

fn live(flows: &[FlowView], id: Uuid) -> Option<&FlowView> {
    flows
        .iter()
        .find(|f| f.id == id)
        .filter(|f| !f.archived && !f.is_unallocated)
}

fn room_of(flow: &FlowView) -> Option<i64> {
    headroom(flow.mode, flow.balance, flow.income_total)
}

// ---------------------------------------------------------------------------
// The cascade, written again from the spec
// ---------------------------------------------------------------------------

/// `remaining` starts at the total; every line takes
/// `min(wanted, room, remaining)`, with `wanted` the fixed amount, the
/// percent of the whole total rounded down, or the headroom. A line that
/// gets nothing by its status asks for nothing, has no room and keeps its
/// envelope's balance (0 for an envelope the vault does not have); an ask
/// below zero counts as nothing.
fn reference(
    total: i64,
    lines: &[AllocationLine],
    flows: &[FlowView],
    unallocated: i64,
) -> AllocationPreview {
    let mut remaining = total.max(0);
    let mut percent_total_bp = 0;
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        if let AllocationRule::Percent { basis_points } = line.rule {
            percent_total_bp += basis_points;
        }
        let nothing = |balance: i64, status: LineStatus| PreviewLine {
            flow_id: line.flow_id,
            rule: line.rule,
            wanted: 0,
            room: None,
            amount: 0,
            balance_after: balance,
            status,
        };
        let known = flows.iter().find(|f| f.id == line.flow_id);
        let Some(flow) = live(flows, line.flow_id) else {
            let balance = known.map_or(0, |f| f.balance);
            out.push(nothing(balance, LineStatus::Archived));
            continue;
        };
        let head = room_of(flow);
        let wanted = match line.rule {
            AllocationRule::Fixed { amount } => amount,
            AllocationRule::Percent { basis_points } => i64::try_from(
                i128::from(total) * i128::from(basis_points) / i128::from(FULL_PERCENT_BP),
            )
            .unwrap(),
            AllocationRule::FillToCap => match head {
                Some(head) => head,
                None => {
                    out.push(nothing(flow.balance, LineStatus::NoCap));
                    continue;
                }
            },
        };
        let wanted = wanted.max(0);
        let room = head.map(|h| h.max(0));
        let mut amount = wanted.min(remaining);
        if let Some(room) = room {
            amount = amount.min(room);
        }
        let status = if amount == wanted {
            LineStatus::Full
        } else if room == Some(amount) {
            LineStatus::CapLimited
        } else {
            LineStatus::Short
        };
        remaining -= amount;
        out.push(PreviewLine {
            flow_id: line.flow_id,
            rule: line.rule,
            wanted,
            room,
            amount,
            balance_after: flow.balance + amount,
            status,
        });
    }
    let distributed = total - remaining;
    AllocationPreview {
        total,
        lines: out,
        distributed,
        remainder: remaining,
        unallocated_after: unallocated - distributed,
        percent_total_bp,
    }
}

fn assert_same_preview(got: &AllocationPreview, want: &AllocationPreview, context: &str) {
    assert_eq!(got.total, want.total, "{context}: total");
    assert_eq!(got.distributed, want.distributed, "{context}: distributed");
    assert_eq!(got.remainder, want.remainder, "{context}: remainder");
    assert_eq!(
        got.unallocated_after, want.unallocated_after,
        "{context}: unallocated_after"
    );
    assert_eq!(
        got.percent_total_bp, want.percent_total_bp,
        "{context}: percent_total_bp"
    );
    assert_eq!(got.lines, want.lines, "{context}: lines");
}

fn amounts(preview: &AllocationPreview) -> Vec<i64> {
    preview.lines.iter().map(|l| l.amount).collect()
}

fn prefix_sums(preview: &AllocationPreview) -> Vec<i64> {
    preview
        .lines
        .iter()
        .scan(0, |sum, l| {
            *sum += l.amount;
            Some(*sum)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Resolver
// ---------------------------------------------------------------------------

#[test]
fn every_line_stays_within_its_wanted_its_room_and_what_is_left_of_the_total() {
    let mut g = Gen::new(1);
    for case in 0..200 {
        let world = world(&mut g);
        let lines = lines(&mut g, &world.flows);
        let total = total(&mut g);
        let preview = resolve(total, &lines, &world.flows, world.unallocated);

        assert_eq!(preview.total, total, "case {case}");
        assert_eq!(preview.lines.len(), lines.len(), "case {case}: a line each");
        let mut remaining = total;
        for (i, (line, planned)) in preview.lines.iter().zip(&lines).enumerate() {
            let at = format!("case {case} line {i}");
            assert_eq!(line.flow_id, planned.flow_id, "{at}: flow");
            assert_eq!(line.rule, planned.rule, "{at}: rule");
            assert!(line.amount >= 0, "{at}: negative amount {}", line.amount);
            assert!(
                line.amount <= remaining,
                "{at}: {} over the {remaining} left",
                line.amount
            );
            if let Some(room) = line.room {
                assert!(room >= 0, "{at}: negative room {room}");
                assert!(
                    line.amount <= room,
                    "{at}: {} over room {room}",
                    line.amount
                );
            }
            match line.status {
                LineStatus::Archived => {
                    assert!(live(&world.flows, line.flow_id).is_none(), "{at}: archived");
                    assert_eq!(line.amount, 0, "{at}: archived gets nothing");
                    assert_eq!(line.wanted, 0, "{at}: archived asks for nothing");
                    assert_eq!(line.room, None, "{at}: archived has no room");
                    let known = world.flows.iter().find(|f| f.id == line.flow_id);
                    assert_eq!(
                        line.balance_after,
                        known.map_or(0, |f| f.balance),
                        "{at}: archived keeps its balance"
                    );
                }
                LineStatus::NoCap => {
                    let flow = live(&world.flows, line.flow_id).unwrap();
                    assert_eq!(planned.rule, AllocationRule::FillToCap, "{at}: no cap");
                    assert_eq!(flow.mode, FlowMode::Unlimited, "{at}: no cap");
                    assert_eq!(line.amount, 0, "{at}: no cap gets nothing");
                    assert_eq!(line.wanted, 0, "{at}: no cap asks for nothing");
                    assert_eq!(line.room, None, "{at}: no cap has no room");
                    assert_eq!(line.balance_after, flow.balance, "{at}: no cap keeps");
                }
                LineStatus::Full | LineStatus::CapLimited | LineStatus::Short => {
                    let flow = live(&world.flows, line.flow_id).unwrap();
                    assert!(line.wanted >= 0, "{at}: negative wanted");
                    assert!(line.amount <= line.wanted, "{at}: over wanted");
                    assert_eq!(line.room, room_of(flow).map(|r| r.max(0)), "{at}: room");
                    assert_eq!(line.balance_after, flow.balance + line.amount, "{at}");
                    match line.status {
                        LineStatus::Full => assert_eq!(line.amount, line.wanted, "{at}"),
                        LineStatus::CapLimited => {
                            assert!(line.amount < line.wanted, "{at}: cap-limited");
                            assert_eq!(Some(line.amount), line.room, "{at}: cap-limited");
                        }
                        _ => {
                            assert!(line.amount < line.wanted, "{at}: short");
                            assert_eq!(line.amount, remaining, "{at}: short takes the rest");
                        }
                    }
                }
            }
            remaining -= line.amount;
        }
        assert!(
            remaining >= 0,
            "case {case}: distributed more than the total"
        );
        assert_eq!(preview.distributed, total - remaining, "case {case}");
        assert_eq!(preview.remainder, remaining, "case {case}");
        assert_eq!(
            preview.unallocated_after,
            world.unallocated - preview.distributed,
            "case {case}"
        );
    }
}

#[test]
fn the_resolver_agrees_with_the_cascade_of_the_spec_on_every_field() {
    let mut g = Gen::new(2);
    for case in 0..300 {
        let world = world(&mut g);
        let lines = lines(&mut g, &world.flows);
        let total = total(&mut g);
        assert_same_preview(
            &resolve(total, &lines, &world.flows, world.unallocated),
            &reference(total, &lines, &world.flows, world.unallocated),
            &format!("case {case}"),
        );
    }
}

#[test]
fn the_first_lines_of_a_preview_do_not_depend_on_the_lines_after_them() {
    let mut g = Gen::new(3);
    for case in 0..150 {
        let world = world(&mut g);
        let lines = lines(&mut g, &world.flows);
        let total = total(&mut g);
        let full = resolve(total, &lines, &world.flows, world.unallocated);
        assert_eq!(full.lines.len(), lines.len(), "case {case}: a line each");
        for k in 0..=lines.len() {
            let prefix = resolve(total, &lines[..k], &world.flows, world.unallocated);
            assert_eq!(
                prefix.lines.as_slice(),
                &full.lines[..k],
                "case {case}: the first {k} lines"
            );
            assert_eq!(
                prefix.distributed,
                full.lines[..k].iter().map(|l| l.amount).sum::<i64>(),
                "case {case}: distributed over {k} lines"
            );
        }
    }
}

#[test]
fn once_the_total_runs_out_every_later_line_gets_nothing() {
    let mut g = Gen::new(4);
    let mut exhausted = 0;
    for case in 0..200 {
        let world = world(&mut g);
        let lines = lines(&mut g, &world.flows);
        let total = g.range(0, 10_000);
        let preview = resolve(total, &lines, &world.flows, world.unallocated);
        let Some(first_dry) = prefix_sums(&preview).iter().position(|sum| *sum == total) else {
            continue;
        };
        exhausted += 1;
        for (i, line) in preview.lines.iter().enumerate().skip(first_dry + 1) {
            assert_eq!(line.amount, 0, "case {case} line {i}: the total was gone");
            let dry_but_wanting = matches!(
                line.status,
                LineStatus::Full | LineStatus::CapLimited | LineStatus::Short
            ) && line.wanted > 0
                && line.room != Some(0);
            if dry_but_wanting {
                assert_eq!(line.status, LineStatus::Short, "case {case} line {i}");
            }
        }
    }
    assert!(exhausted >= 40, "only {exhausted} cases ran dry");
}

#[test]
fn percents_adding_up_to_the_whole_on_uncapped_envelopes_are_all_full_and_leave_less_than_a_cent_each()
 {
    let mut g = Gen::new(5);
    for case in 0..100 {
        let n = g.index(6) + 1;
        let flows: Vec<FlowView> = (0..n)
            .map(|_| {
                let balance = g.range(0, 50_000);
                view(g.uuid(), balance, FlowMode::Unlimited, None, false)
            })
            .collect();
        let bps = g.partition(i64::from(FULL_PERCENT_BP), n);
        let lines: Vec<AllocationLine> = flows
            .iter()
            .zip(&bps)
            .map(|(f, bp)| percent(f.id, u32::try_from(*bp).unwrap()))
            .collect();
        let total = g.range(0, 1_000_000);
        let preview = resolve(total, &lines, &flows, 0);

        assert_eq!(preview.percent_total_bp, FULL_PERCENT_BP, "case {case}");
        for (line, bp) in preview.lines.iter().zip(&bps) {
            assert_eq!(line.status, LineStatus::Full, "case {case}");
            assert_eq!(
                i128::from(line.amount),
                i128::from(total) * i128::from(*bp) / i128::from(FULL_PERCENT_BP),
                "case {case}: rounded down"
            );
        }
        assert!(preview.remainder >= 0, "case {case}");
        assert!(
            preview.remainder < i64::try_from(n).unwrap(),
            "case {case}: remainder {} over {n} lines",
            preview.remainder
        );
        assert_eq!(
            preview.distributed + preview.remainder,
            total,
            "case {case}"
        );
    }
}

#[test]
fn a_bigger_total_never_distributes_less_to_any_prefix_of_the_plan() {
    let mut g = Gen::new(6);
    for case in 0..150 {
        let world = world(&mut g);
        let lines = lines(&mut g, &world.flows);
        let small = total(&mut g);
        let big = small + g.range(0, 50_000);
        let on_small = resolve(small, &lines, &world.flows, world.unallocated);
        let on_big = resolve(big, &lines, &world.flows, world.unallocated);
        assert_eq!(
            on_small.lines.len(),
            lines.len(),
            "case {case}: a line each"
        );
        assert_eq!(on_big.lines.len(), lines.len(), "case {case}: a line each");
        for (k, (s, b)) in prefix_sums(&on_small)
            .iter()
            .zip(prefix_sums(&on_big))
            .enumerate()
        {
            assert!(b >= *s, "case {case}: the first {k} lines got {b} < {s}");
        }
        assert!(on_big.distributed >= on_small.distributed, "case {case}");
    }
}

#[test]
fn percent_only_plans_within_the_whole_never_run_short_and_grow_with_the_total() {
    let mut g = Gen::new(7);
    let mut checked = 0;
    for case in 0..150 {
        let world = world(&mut g);
        let mut envelopes: Vec<&FlowView> = world
            .flows
            .iter()
            .filter(|f| !f.archived && !f.is_unallocated)
            .collect();
        if envelopes.is_empty() {
            continue;
        }
        checked += 1;
        g.shuffle(&mut envelopes);
        let n = g.index(envelopes.len()) + 1;
        let sum = g.range(i64::try_from(n).unwrap(), i64::from(FULL_PERCENT_BP));
        let bps = g.partition(sum, n);
        let lines: Vec<AllocationLine> = envelopes[..n]
            .iter()
            .zip(&bps)
            .map(|(f, bp)| percent(f.id, u32::try_from(*bp).unwrap()))
            .collect();
        let small = total(&mut g);
        let big = small + g.range(0, 50_000);
        let on_small = resolve(small, &lines, &world.flows, world.unallocated);
        let on_big = resolve(big, &lines, &world.flows, world.unallocated);
        for preview in [&on_small, &on_big] {
            assert_eq!(preview.lines.len(), n, "case {case}: a line each");
            assert!(preview.percent_total_bp <= FULL_PERCENT_BP, "case {case}");
            for (i, line) in preview.lines.iter().enumerate() {
                assert_ne!(line.status, LineStatus::Short, "case {case} line {i}");
                let capped = line.room.map_or(line.wanted, |room| line.wanted.min(room));
                assert_eq!(line.amount, capped, "case {case} line {i}: wanted or room");
            }
        }
        for (i, (s, b)) in amounts(&on_small).iter().zip(amounts(&on_big)).enumerate() {
            assert!(b >= *s, "case {case} line {i}: {b} < {s} on a bigger total");
        }
    }
    assert!(checked >= 100, "only {checked} cases had an envelope");
}

/// Rounding down line by line makes the per-line monotonicity fail for a
/// mixed plan: two percent lines can both step up on the same extra cent
/// and leave a fixed line after them one cent poorer. The cascade of the
/// spec says so; this pins it rather than hides it.
#[test]
fn rounding_can_give_a_fixed_line_after_percent_lines_less_on_a_bigger_total() {
    let mut g = Gen::new(8);
    let flows: Vec<FlowView> = (0..3)
        .map(|_| view(g.uuid(), 0, FlowMode::Unlimited, None, false))
        .collect();
    let lines = [
        percent(flows[0].id, 5_000),
        percent(flows[1].id, 4_999),
        fixed(flows[2].id, 2),
    ];
    let on = |total: i64| amounts(&resolve(total, &lines, &flows, 0));
    assert_eq!(on(5_001), vec![2_500, 2_499, 2]);
    assert_eq!(on(5_002), vec![2_501, 2_500, 1]);
}

#[test]
fn archived_and_uncapped_fill_lines_consume_nothing_and_the_order_of_the_envelopes_does_not_matter()
{
    let mut g = Gen::new(9);
    let mut dropped = 0;
    for case in 0..200 {
        let world = world(&mut g);
        let lines = lines(&mut g, &world.flows);
        let total = total(&mut g);
        let preview = resolve(total, &lines, &world.flows, world.unallocated);
        assert_eq!(preview.lines.len(), lines.len(), "case {case}: a line each");

        let kept: Vec<AllocationLine> = lines
            .iter()
            .zip(&preview.lines)
            .filter(|(_, p)| !matches!(p.status, LineStatus::Archived | LineStatus::NoCap))
            .map(|(l, _)| *l)
            .collect();
        dropped += lines.len() - kept.len();
        let without = resolve(total, &kept, &world.flows, world.unallocated);
        let money_lines: Vec<&PreviewLine> = preview
            .lines
            .iter()
            .filter(|p| !matches!(p.status, LineStatus::Archived | LineStatus::NoCap))
            .collect();
        assert_eq!(
            money_lines,
            without.lines.iter().collect::<Vec<_>>(),
            "case {case}: the lines that get money"
        );
        assert_eq!(without.distributed, preview.distributed, "case {case}");
        assert_eq!(without.remainder, preview.remainder, "case {case}");
        assert_eq!(
            without.unallocated_after, preview.unallocated_after,
            "case {case}"
        );

        let mut shuffled = world.flows.clone();
        g.shuffle(&mut shuffled);
        assert_eq!(
            resolve(total, &lines, &shuffled, world.unallocated),
            preview,
            "case {case}: the order of the flows"
        );
    }
    assert!(
        dropped >= 40,
        "only {dropped} archived or no-cap lines seen"
    );
}

#[test]
fn the_cap_rules_of_the_spec_come_out_as_stated() {
    let mut g = Gen::new(10);
    let full = view(
        g.uuid(),
        1_000,
        FlowMode::NetCapped { cap: 1_000 },
        None,
        false,
    );
    let below = view(
        g.uuid(),
        -250,
        FlowMode::NetCapped { cap: 1_000 },
        None,
        true,
    );
    let spent = view(
        g.uuid(),
        120,
        FlowMode::IncomeCapped { cap: 1_000 },
        Some(400),
        false,
    );
    let open = view(g.uuid(), 5, FlowMode::Unlimited, None, false);
    let flows = vec![full.clone(), below.clone(), spent.clone(), open.clone()];
    let lines = [
        fill(full.id),
        fixed(full.id, 100),
        fill(below.id),
        fill(spent.id),
        fill(open.id),
        percent(open.id, 10_000),
    ];
    let preview = resolve(10_000, &lines, &flows, 10_000);
    assert_eq!(preview.lines.len(), lines.len(), "a line each");
    let got: Vec<(i64, Option<i64>, i64, LineStatus)> = preview
        .lines
        .iter()
        .map(|l| (l.wanted, l.room, l.amount, l.status))
        .collect();
    let archived_or_nocap = |l: &PreviewLine| matches!(l.status, LineStatus::NoCap);
    assert_eq!(
        got[..4],
        [
            // Already full: asks for nothing and is Full with nothing.
            (0, Some(0), 0, LineStatus::Full),
            // A fixed amount on a full envelope: the cap stops it at zero.
            (100, Some(0), 0, LineStatus::CapLimited),
            // Below zero: the room is more than the cap.
            (1_250, Some(1_250), 1_250, LineStatus::Full),
            // Income-capped: the room is what the cap leaves to the income,
            // not to the balance.
            (600, Some(600), 600, LineStatus::Full),
        ]
    );
    assert!(archived_or_nocap(&preview.lines[4]));
    assert_eq!(preview.lines[4].amount, 0);
    // 100% of the total, less what the lines before took: short.
    assert_eq!(
        (got[5].0, got[5].1, got[5].2, got[5].3),
        (10_000, None, 10_000 - 1_250 - 600, LineStatus::Short)
    );
    assert_eq!(preview.lines[5].balance_after, 5 + 10_000 - 1_250 - 600);
    assert_eq!(preview.distributed, 10_000);
    assert_eq!(preview.remainder, 0);
    assert_eq!(preview.unallocated_after, 0);
    assert_eq!(preview.percent_total_bp, 10_000);
}

#[test]
fn percent_total_bp_adds_up_the_percent_lines_whatever_the_envelopes_are() {
    let mut g = Gen::new(11);
    let open = view(g.uuid(), 0, FlowMode::Unlimited, None, false);
    let mut gone = view(g.uuid(), 0, FlowMode::Unlimited, None, false);
    gone.archived = true;
    let flows = vec![open.clone(), gone.clone()];
    let lines = [
        percent(open.id, 2_500),
        fixed(open.id, 10),
        percent(gone.id, 3_000),
        percent(g.uuid(), 6_000),
        fill(open.id),
    ];
    let preview = resolve(1_000, &lines, &flows, 0);
    assert_eq!(preview.percent_total_bp, 2_500 + 3_000 + 6_000);
    assert!(preview.percent_total_bp > FULL_PERCENT_BP);
    assert_eq!(
        resolve(1_000, &lines[..2], &flows, 0).percent_total_bp,
        2_500
    );
    assert_eq!(resolve(1_000, &[], &flows, 0).percent_total_bp, 0);
}

#[test]
fn extreme_totals_balances_and_caps_never_make_the_resolver_panic() {
    let mut g = Gen::new(12);
    let max_open = view(g.uuid(), i64::MAX, FlowMode::Unlimited, None, false);
    let zero_open = view(g.uuid(), 0, FlowMode::Unlimited, None, false);
    let one_open = view(g.uuid(), 1, FlowMode::Unlimited, None, false);
    let max_capped = view(
        g.uuid(),
        0,
        FlowMode::NetCapped { cap: i64::MAX },
        None,
        false,
    );
    let deep = view(
        g.uuid(),
        i64::MIN + 1,
        FlowMode::NetCapped { cap: i64::MAX },
        None,
        true,
    );
    let over = view(
        g.uuid(),
        2_000,
        FlowMode::NetCapped { cap: 1_000 },
        None,
        false,
    );
    let income_max = view(
        g.uuid(),
        0,
        FlowMode::IncomeCapped { cap: i64::MAX },
        Some(0),
        false,
    );
    let flows = vec![
        max_open.clone(),
        zero_open.clone(),
        one_open.clone(),
        max_capped.clone(),
        deep.clone(),
        over.clone(),
        income_max.clone(),
    ];
    let every_line = [
        fixed(max_open.id, i64::MAX),
        percent(max_open.id, 10_000),
        percent(zero_open.id, 10_000),
        percent(zero_open.id, 1),
        fixed(one_open.id, i64::MAX),
        fill(max_capped.id),
        fixed(max_capped.id, i64::MAX),
        fill(deep.id),
        fill(over.id),
        fixed(over.id, 10),
        fill(income_max.id),
        fixed(zero_open.id, 0),
        fixed(zero_open.id, -7),
    ];
    let totals = [i64::MAX, i64::MAX - 1, 0, 1, -1, i64::MIN, i64::MIN + 1];
    let balances = [i64::MAX, i64::MIN, 0, -1, 1];
    for total in totals {
        for balance in balances {
            for k in 0..=every_line.len() {
                let preview = resolve(total, &every_line[..k], &flows, balance);
                assert_eq!(preview.lines.len(), k);
                for (i, line) in preview.lines.iter().enumerate() {
                    let at = format!("total {total} balance {balance} line {i}");
                    assert!(line.amount >= 0, "{at}: {}", line.amount);
                    assert!(line.wanted >= 0, "{at}: wanted {}", line.wanted);
                    if let Some(room) = line.room {
                        assert!(room >= 0, "{at}: room {room}");
                        assert!(line.amount <= room, "{at}");
                    }
                    if total < 0 {
                        assert_eq!(line.amount, 0, "{at}: a negative total gives nothing");
                    }
                    let flow = flows.iter().find(|f| f.id == line.flow_id).unwrap();
                    assert_eq!(
                        line.balance_after,
                        flow.balance.saturating_add(line.amount),
                        "{at}: the balance after saturates"
                    );
                }
                let sum: i128 = preview.lines.iter().map(|l| i128::from(l.amount)).sum();
                assert_eq!(i128::from(preview.distributed), sum, "total {total}");
                assert_eq!(
                    preview.remainder,
                    total.max(0).saturating_sub(preview.distributed),
                    "total {total}: the remainder saturates and is never below zero"
                );
                assert_eq!(
                    preview.unallocated_after,
                    balance.saturating_sub(preview.distributed),
                    "total {total} balance {balance}: Unallocated after saturates"
                );
                if total >= 0 {
                    assert!(sum <= i128::from(total), "total {total}: over the total");
                }
            }
        }
    }

    // A draft's asks below zero count as nothing and the line is Full with
    // nothing, like an envelope already at its cap; so does a fill on an
    // envelope found over its cap.
    let drafts = resolve(100, &every_line[8..], &flows, 0);
    let over_cap = drafts.lines[0];
    assert_eq!(
        (
            over_cap.wanted,
            over_cap.room,
            over_cap.amount,
            over_cap.status
        ),
        (0, Some(0), 0, LineStatus::Full)
    );
    assert_eq!(over_cap.balance_after, 2_000);
    let negative = drafts.lines[4];
    assert_eq!(negative.rule, AllocationRule::Fixed { amount: -7 });
    assert_eq!(
        (
            negative.wanted,
            negative.room,
            negative.amount,
            negative.status
        ),
        (0, None, 0, LineStatus::Full)
    );
    // The fixed ask on the envelope over its cap is stopped at zero too, and
    // the fill on the income-capped envelope then takes the whole total.
    let stopped = drafts.lines[1];
    assert_eq!(
        (stopped.wanted, stopped.room, stopped.amount, stopped.status),
        (10, Some(0), 0, LineStatus::CapLimited)
    );
    assert_eq!(drafts.lines[2].amount, 100);
    assert_eq!(drafts.distributed, 100);
    // A negative total shares out nothing: the lines still say what they
    // ask, a percent of it asks for nothing.
    let nothing = resolve(-5, &every_line, &flows, 40);
    assert_eq!(nothing.distributed, 0);
    assert_eq!(nothing.remainder, 0, "nothing was there to keep");
    assert_eq!(nothing.unallocated_after, 40);
    assert!(nothing.lines.iter().all(|l| l.amount == 0 && l.wanted >= 0));
    assert_eq!(nothing.lines[1].wanted, 0, "a percent of less than nothing");
    assert_eq!(nothing.lines[0].wanted, i64::MAX, "a fixed ask stays");

    // The whole of i64::MAX goes to a single 100% line with nothing lost.
    let whole = resolve(i64::MAX, &[percent(zero_open.id, 10_000)], &flows, 0);
    assert_eq!(whole.lines[0].wanted, i64::MAX);
    assert_eq!(whole.lines[0].amount, i64::MAX);
    assert_eq!(whole.lines[0].balance_after, i64::MAX);
    assert_eq!(whole.lines[0].status, LineStatus::Full);
    assert_eq!(whole.remainder, 0);
    assert_eq!(whole.unallocated_after, -i64::MAX);
    // One basis point of i64::MAX, rounded down.
    let sliver = resolve(i64::MAX, &[percent(zero_open.id, 1)], &flows, 0);
    assert_eq!(sliver.lines[0].wanted, i64::MAX / 10_000);
    // A cap of i64::MAX leaves a room of i64::MAX.
    let roomy = resolve(i64::MAX, &[fill(max_capped.id)], &flows, i64::MAX);
    assert_eq!(roomy.lines[0].room, Some(i64::MAX));
    assert_eq!(roomy.lines[0].amount, i64::MAX);
    assert_eq!(roomy.unallocated_after, 0);
}

// ---------------------------------------------------------------------------
// Engine fixtures
// ---------------------------------------------------------------------------

fn start() -> NaiveDate {
    day(2026, 1, 1)
}

fn secs(date: NaiveDate) -> i64 {
    noon(date).timestamp()
}

fn income_on(fx: &mut Fx, amount: i64, date: NaiveDate) -> Uuid {
    income_in(fx, amount, secs(date))
}

/// Money in Unallocated from before the plan's start: it funds every move
/// and enters no base.
fn fund(fx: &mut Fx) {
    income_on(fx, 1_000_000, day(2025, 12, 20));
}

fn plan_from_start(fx: &mut Fx, lines: Vec<AllocationLine>) -> Uuid {
    run(
        &mut fx.core,
        fx.vault,
        plan_cmd(monthly_from(1, start()), lines),
    )
    .result_id
    .unwrap()
}

fn base_of(fx: &Fx) -> AllocationBase {
    fx.core.allocation_base(fx.vault).unwrap()
}

fn income_ids(base: &AllocationBase) -> Vec<Uuid> {
    base.incomes.iter().map(|t| t.id).collect()
}

fn runs_of(fx: &Fx) -> Vec<AllocationRunView> {
    fx.core.allocation_runs(fx.vault, 100).unwrap()
}

fn pending_on(fx: &Fx, today: NaiveDate) -> Option<PendingAllocation> {
    fx.core.pending_allocation(fx.vault, today).unwrap()
}

fn balance_of(fx: &Fx, flow: Uuid) -> i64 {
    flow_balance(&fx.core, fx.vault, flow)
}

fn transfers_of(fx: &Fx) -> Vec<TransactionView> {
    list(&fx.core, fx.vault, &all())
        .into_iter()
        .filter(|t| t.kind == TransactionKind::TransferFlow)
        .collect()
}

fn void(fx: &mut Fx, transaction_id: Uuid) {
    run(
        &mut fx.core,
        fx.vault,
        Command::VoidTransaction { transaction_id },
    );
}

fn expense_from(fx: &mut Fx, flow: Uuid, amount: i64) {
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(
            amount,
            None,
            Some(flow),
            None,
            secs(day(2025, 12, 22)),
        )),
    );
}

fn archived_envelope(fx: &mut Fx, name: &str) -> Uuid {
    let id = envelope(fx, name, FlowMode::Unlimited);
    run(&mut fx.core, fx.vault, Command::ArchiveFlow { flow_id: id });
    id
}

fn execute_at(plan: Uuid, period: NaiveDate, total: i64, moves: &[(Uuid, i64)]) -> Command {
    execute_cmd(plan, period, total, moves)
}

/// Everything the allocation can touch, read back from the core.
#[derive(Debug, PartialEq)]
struct Projection {
    snapshot: VaultSnapshot,
    transactions: Vec<TransactionView>,
    plan: Option<AllocationPlanView>,
    runs: Vec<AllocationRunView>,
    base: AllocationBase,
    pending: Option<PendingAllocation>,
    log: Vec<(i64, Uuid, Option<Uuid>)>,
}

fn projection(core: &Core, vault: Uuid) -> Projection {
    Projection {
        snapshot: core.snapshot(vault).unwrap(),
        transactions: list(core, vault, &all()),
        plan: core.allocation_plan(vault).unwrap(),
        runs: core.allocation_runs(vault, 100).unwrap(),
        base: core.allocation_base(vault).unwrap(),
        pending: core.pending_allocation(vault, day(2026, 12, 31)).unwrap(),
        log: core
            .commands_since(vault, 0)
            .unwrap()
            .iter()
            .map(|r| (r.seq, r.envelope.id, r.result_id))
            .collect(),
    }
}

/// Runs `cmd` expecting a refusal, and checks that nothing changed: no
/// log row, no transaction, no run, no balance.
fn refused(fx: &mut Fx, what: &str, cmd: Command) -> DomainError {
    let before = projection(&fx.core, fx.vault);
    let err = try_run(&mut fx.core, fx.vault, cmd).expect_err(what);
    assert_eq!(
        projection(&fx.core, fx.vault),
        before,
        "{what}: changed something"
    );
    err
}

/// [`refused`], with the error code the app relies on.
fn refused_as(fx: &mut Fx, what: &str, code: &str, cmd: Command) -> DomainError {
    let err = refused(fx, what, cmd);
    assert_eq!(err.code(), code, "{what}: {err}");
    err
}

/// An envelope with a state shaped through the engine: within its cap,
/// spent down, sometimes below zero.
fn shaped_envelope(g: &mut Gen, fx: &mut Fx, index: usize) -> Uuid {
    let name = format!("Envelope {index}");
    let allow_negative = g.chance(30);
    match g.below(3) {
        0 => {
            let opening = g.range(0, 20_000);
            let id = run(
                &mut fx.core,
                fx.vault,
                flow_cmd(&name, FlowMode::Unlimited, allow_negative, opening),
            )
            .result_id
            .unwrap();
            if g.chance(50) {
                let max = if allow_negative {
                    opening + 5_000
                } else {
                    opening
                };
                if max > 0 {
                    expense_from(fx, id, g.range(1, max));
                }
            }
            id
        }
        1 => {
            let cap = g.range(100, 50_000);
            let opening = g.range(0, cap);
            let id = run(
                &mut fx.core,
                fx.vault,
                flow_cmd(&name, FlowMode::NetCapped { cap }, allow_negative, opening),
            )
            .result_id
            .unwrap();
            if g.chance(50) {
                let max = if allow_negative {
                    opening + cap
                } else {
                    opening
                };
                if max > 0 {
                    expense_from(fx, id, g.range(1, max));
                }
            }
            id
        }
        _ => {
            let cap = g.range(100, 50_000);
            let id = run(
                &mut fx.core,
                fx.vault,
                flow_cmd(&name, FlowMode::IncomeCapped { cap }, allow_negative, 0),
            )
            .result_id
            .unwrap();
            let income = g.range(0, cap);
            if income > 0 {
                run(
                    &mut fx.core,
                    fx.vault,
                    Command::Income(entry(income, None, Some(id), None, secs(day(2025, 12, 21)))),
                );
            }
            if g.chance(50) {
                let max = if allow_negative { income + cap } else { income };
                if max > 0 {
                    expense_from(fx, id, g.range(1, max));
                }
            }
            id
        }
    }
}

/// A plan over a random subset of `pool`, each envelope once.
fn plan_lines(g: &mut Gen, pool: &[Uuid]) -> Vec<AllocationLine> {
    let mut ids = pool.to_vec();
    g.shuffle(&mut ids);
    let n = g.index(ids.len()) + 1;
    ids.truncate(n);
    ids.into_iter()
        .map(|flow_id| AllocationLine {
            flow_id,
            rule: rule(g),
        })
        .collect()
}

/// A move per plan line that still has room, within that room.
fn moves_within_room(g: &mut Gen, fx: &Fx, lines: &[AllocationLine]) -> Vec<(Uuid, i64)> {
    let snapshot = fx.core.snapshot(fx.vault).unwrap();
    lines
        .iter()
        .filter_map(|line| {
            let flow = snapshot.flows.iter().find(|f| f.id == line.flow_id)?;
            if flow.archived {
                return None;
            }
            let room = room_of(flow).unwrap_or(20_000).min(20_000);
            (room > 0).then(|| (flow.id, g.range(1, room)))
        })
        .collect()
}

fn sum(moves: &[(Uuid, i64)]) -> i64 {
    moves.iter().map(|m| m.1).sum()
}

// ---------------------------------------------------------------------------
// Engine: conservation
// ---------------------------------------------------------------------------

#[test]
fn an_execution_moves_exactly_the_amounts_out_of_unallocated_and_touches_no_wallet() {
    let mut g = Gen::new(21);
    let mut executed = 0;
    for case in 0..10 {
        let mut fx = setup();
        fund(&mut fx);
        let mut pool = vec![envelope(&mut fx, "Open", FlowMode::Unlimited)];
        for i in 0..g.index(4) {
            pool.push(shaped_envelope(&mut g, &mut fx, i));
        }
        let lines = plan_lines(&mut g, &pool);
        let plan = plan_from_start(&mut fx, lines.clone());
        for _ in 0..=g.index(3) {
            let amount = g.range(100, 50_000);
            let date = day(2026, 1, u32::try_from(g.range(2, 28)).unwrap());
            income_on(&mut fx, amount, date);
        }
        let moves = moves_within_room(&mut g, &fx, &lines);
        if moves.is_empty() {
            continue;
        }
        executed += 1;
        let total = sum(&moves) + g.range(0, 1_000);

        let before = projection(&fx.core, fx.vault);
        let (wallets_before, flows_before) = balances(&fx.core, fx.vault);
        run(
            &mut fx.core,
            fx.vault,
            execute_at(plan, start(), total, &moves),
        );
        let (wallets_after, flows_after) = balances(&fx.core, fx.vault);

        assert_eq!(wallets_after, wallets_before, "case {case}: wallets");
        for ((id, was), (same, now)) in flows_before.iter().zip(&flows_after) {
            assert_eq!(id, same);
            let expected = if *id == fx.unallocated {
                was - sum(&moves)
            } else {
                was + moves
                    .iter()
                    .filter(|m| m.0 == *id)
                    .map(|m| m.1)
                    .sum::<i64>()
            };
            assert_eq!(*now, expected, "case {case}: flow {id}");
        }
        let wallet_sum: i64 = wallets_after.iter().map(|w| w.1).sum();
        let flow_sum: i64 = flows_after.iter().map(|f| f.1).sum();
        assert_eq!(wallet_sum, flow_sum, "case {case}: the ledger balances");

        let new: Vec<TransactionView> = list(&fx.core, fx.vault, &all())
            .into_iter()
            .filter(|t| !before.transactions.iter().any(|b| b.id == t.id))
            .collect();
        assert_eq!(new.len(), moves.len(), "case {case}: one transfer per move");
        for t in &new {
            assert_eq!(t.kind, TransactionKind::TransferFlow, "case {case}");
            assert_eq!(t.from_id, Some(fx.unallocated), "case {case}");
            assert!(!t.voided, "case {case}");
            assert_eq!(t.occurred_at, noon(start()), "case {case}");
            assert!(t.category_is_system, "case {case}: Uncategorized");
            assert_eq!(t.created_by, "alice", "case {case}");
            let (_, amount) = moves.iter().find(|m| Some(m.0) == t.to_id).unwrap();
            assert_eq!(t.amount, *amount, "case {case}");
            let legs: Vec<i64> = t.legs.iter().map(|l| l.amount).collect();
            assert_eq!(legs, vec![-amount, *amount], "case {case}");
        }

        let runs = runs_of(&fx);
        assert_eq!(runs.len(), 1, "case {case}");
        let run_view = &runs[0];
        assert_eq!(run_view.period_date, start());
        assert_eq!(run_view.outcome, RunOutcome::Executed);
        assert_eq!(run_view.total, total);
        assert_eq!(run_view.created_by, "alice");
        let mut recorded: Vec<(Uuid, i64)> = run_view
            .moves
            .iter()
            .map(|m| (m.flow_id, m.amount))
            .collect();
        let mut sent = moves.clone();
        recorded.sort();
        sent.sort();
        assert_eq!(recorded, sent, "case {case}: the run's moves");
        for m in &run_view.moves {
            assert!(!m.voided);
            let t = new.iter().find(|t| t.id == m.transaction_id).unwrap();
            assert_eq!(t.to_id, Some(m.flow_id), "case {case}");
        }
        assert_eq!(pending_on(&fx, day(2026, 1, 31)), None, "case {case}");
        assert_eq!(
            pending_on(&fx, day(2026, 2, 1)),
            Some(PendingAllocation {
                plan_id: plan,
                period_date: day(2026, 2, 1),
                missed: 0
            }),
            "case {case}"
        );
    }
    assert!(executed >= 6, "only {executed} cases had a move");
}

// ---------------------------------------------------------------------------
// Engine: once, in order
// ---------------------------------------------------------------------------

#[test]
fn deciding_the_same_period_twice_is_refused_and_changes_nothing() {
    let mut fx = setup();
    fund(&mut fx);
    let a = envelope(&mut fx, "A", FlowMode::NetCapped { cap: 5_000 });
    let plan = plan_from_start(&mut fx, vec![fixed(a, 800)]);
    income_on(&mut fx, 20_000, day(2026, 1, 5));
    run(
        &mut fx.core,
        fx.vault,
        execute_at(plan, start(), 20_000, &[(a, 800)]),
    );

    let err = refused(
        &mut fx,
        "executed twice",
        execute_at(plan, start(), 20_000, &[(a, 100)]),
    );
    assert!(matches!(err, DomainError::AlreadyExists(_)), "{err}");
    let err = refused(&mut fx, "skipped after executed", skip_cmd(plan, start()));
    assert!(matches!(err, DomainError::AlreadyExists(_)), "{err}");

    run(&mut fx.core, fx.vault, skip_cmd(plan, day(2026, 2, 1)));
    let err = refused(&mut fx, "skipped twice", skip_cmd(plan, day(2026, 2, 1)));
    assert!(matches!(err, DomainError::AlreadyExists(_)), "{err}");
    let err = refused(
        &mut fx,
        "executed after skipped",
        execute_at(plan, day(2026, 2, 1), 100, &[(a, 1)]),
    );
    assert!(matches!(err, DomainError::AlreadyExists(_)), "{err}");
    assert_eq!(runs_of(&fx).len(), 2);
}

#[test]
fn deciding_the_latest_due_period_closes_the_missed_ones_and_an_earlier_one_is_refused() {
    let mut fx = setup();
    fund(&mut fx);
    let a = envelope(&mut fx, "A", FlowMode::NetCapped { cap: 5_000 });
    let plan = plan_from_start(&mut fx, vec![fixed(a, 800)]);
    let i1 = income_on(&mut fx, 1_000, day(2026, 1, 5));
    let i2 = income_on(&mut fx, 2_000, day(2026, 2, 5));
    let i3 = income_on(&mut fx, 3_000, day(2026, 3, 5));

    assert_eq!(pending_on(&fx, day(2025, 12, 31)), None);
    assert_eq!(
        pending_on(&fx, start()),
        Some(PendingAllocation {
            plan_id: plan,
            period_date: start(),
            missed: 0
        })
    );
    assert_eq!(
        pending_on(&fx, day(2026, 3, 15)),
        Some(PendingAllocation {
            plan_id: plan,
            period_date: day(2026, 3, 1),
            missed: 2
        })
    );
    let base = base_of(&fx);
    assert_eq!(income_ids(&base), vec![i1, i2, i3]);
    assert_eq!(base.total, 6_000);

    run(
        &mut fx.core,
        fx.vault,
        execute_at(plan, day(2026, 3, 1), 6_000, &[(a, 800)]),
    );
    assert_eq!(pending_on(&fx, day(2026, 3, 15)), None);
    assert_eq!(
        pending_on(&fx, day(2026, 4, 1)),
        Some(PendingAllocation {
            plan_id: plan,
            period_date: day(2026, 4, 1),
            missed: 0
        })
    );
    let runs = runs_of(&fx);
    assert_eq!(runs.len(), 1, "the missed periods get no row of their own");
    assert_eq!(runs[0].period_date, day(2026, 3, 1));
    assert_eq!(
        base_of(&fx),
        AllocationBase {
            total: 0,
            incomes: Vec::new()
        }
    );

    for (what, cmd) in [
        (
            "execute a missed period",
            execute_at(plan, day(2026, 2, 1), 100, &[(a, 1)]),
        ),
        ("skip a missed period", skip_cmd(plan, start())),
        (
            "execute the first period",
            execute_at(plan, start(), 100, &[(a, 1)]),
        ),
    ] {
        refused_as(&mut fx, what, "invalid_command", cmd);
    }

    // The total is the user's number: nothing in the base is no refusal.
    run(
        &mut fx.core,
        fx.vault,
        execute_at(plan, day(2026, 4, 1), 100, &[(a, 100)]),
    );
    let periods: Vec<NaiveDate> = runs_of(&fx).iter().map(|r| r.period_date).collect();
    assert_eq!(periods, vec![day(2026, 4, 1), day(2026, 3, 1)]);
}

#[test]
fn nothing_is_pending_before_the_start_after_the_end_or_on_a_disabled_plan() {
    let mut fx = setup();
    fund(&mut fx);
    let a = envelope(&mut fx, "A", FlowMode::Unlimited);
    let mut schedule = monthly_from(1, start());
    schedule.end_date = Some(day(2026, 2, 15));
    let plan = run(
        &mut fx.core,
        fx.vault,
        plan_cmd(schedule, vec![fixed(a, 10)]),
    )
    .result_id
    .unwrap();

    assert_eq!(pending_on(&fx, day(2025, 12, 31)), None);
    assert_eq!(
        pending_on(&fx, day(2026, 6, 1)),
        Some(PendingAllocation {
            plan_id: plan,
            period_date: day(2026, 2, 1),
            missed: 1
        })
    );
    refused_as(
        &mut fx,
        "a day that is no period",
        "invalid_command",
        execute_at(plan, day(2026, 1, 2), 100, &[(a, 1)]),
    );
    refused_as(
        &mut fx,
        "a period after the end",
        "invalid_command",
        execute_at(plan, day(2026, 3, 1), 100, &[(a, 1)]),
    );

    let disable = |enabled| {
        update_plan_cmd(
            plan,
            AllocationPlanPatch {
                enabled: Some(enabled),
                ..AllocationPlanPatch::default()
            },
        )
    };
    run(&mut fx.core, fx.vault, disable(false));
    assert_eq!(pending_on(&fx, day(2026, 6, 1)), None);
    assert!(!fx.core.allocation_plan(fx.vault).unwrap().unwrap().enabled);
    refused_as(
        &mut fx,
        "a disabled plan",
        "invalid_command",
        execute_at(plan, day(2026, 2, 1), 100, &[(a, 1)]),
    );
    refused_as(
        &mut fx,
        "a disabled plan skipped",
        "invalid_command",
        skip_cmd(plan, day(2026, 2, 1)),
    );
    run(&mut fx.core, fx.vault, disable(true));
    assert_eq!(
        pending_on(&fx, day(2026, 6, 1)).map(|p| p.period_date),
        Some(day(2026, 2, 1))
    );

    run(&mut fx.core, fx.vault, skip_cmd(plan, day(2026, 2, 1)));
    assert_eq!(pending_on(&fx, day(2026, 6, 1)), None);
    assert_eq!(pending_on(&fx, day(2030, 1, 1)), None);
}

// ---------------------------------------------------------------------------
// Engine: atomicity
// ---------------------------------------------------------------------------

#[test]
fn every_refused_execution_leaves_no_transaction_no_run_and_no_balance_change() {
    let mut fx = setup();
    fund(&mut fx);
    let capped = envelope(&mut fx, "Capped", FlowMode::NetCapped { cap: 1_000 });
    let open = envelope(&mut fx, "Open", FlowMode::Unlimited);
    let other = envelope(&mut fx, "Other", FlowMode::Unlimited);
    let gone = archived_envelope(&mut fx, "Gone");
    let plan = plan_from_start(&mut fx, vec![fixed(capped, 500), fixed(open, 500)]);
    income_on(&mut fx, 10_000, day(2026, 1, 5));
    let total = 10_000;

    let err = refused(
        &mut fx,
        "a move over the cap",
        execute_at(plan, start(), total, &[(open, 10), (capped, 1_001)]),
    );
    assert_eq!(err, DomainError::MaxBalanceReached("Capped".to_string()));

    let unknown = Uuid::now_v7();
    let cases = [
        (
            "the same envelope twice",
            "invalid_command",
            execute_at(plan, start(), total, &[(open, 100), (open, 200)]),
        ),
        (
            "an archived envelope",
            "invalid_command",
            execute_at(plan, start(), total, &[(open, 100), (gone, 100)]),
        ),
        (
            "Unallocated as a target",
            "invalid_flow",
            execute_at(plan, start(), total, &[(fx.unallocated, 100)]),
        ),
        (
            "moves above the total",
            "invalid_amount",
            execute_at(plan, start(), 100, &[(open, 60), (capped, 50)]),
        ),
        (
            "a zero amount",
            "invalid_amount",
            execute_at(plan, start(), total, &[(open, 100), (capped, 0)]),
        ),
        (
            "a negative amount",
            "invalid_amount",
            execute_at(plan, start(), total, &[(open, -5)]),
        ),
        (
            "no moves",
            "invalid_command",
            execute_at(plan, start(), total, &[]),
        ),
        (
            "a negative total",
            "invalid_amount",
            execute_at(plan, start(), -1, &[(open, 1)]),
        ),
        (
            "an unknown envelope",
            "not_found",
            execute_at(plan, start(), total, &[(unknown, 100)]),
        ),
        (
            "an unknown plan",
            "not_found",
            execute_at(unknown, start(), total, &[(open, 100)]),
        ),
        (
            "moves whose sum overflows",
            "invalid_amount",
            execute_at(
                plan,
                start(),
                i64::MAX,
                &[(open, i64::MAX), (other, i64::MAX)],
            ),
        ),
    ];
    for (what, code, cmd) in cases {
        refused_as(&mut fx, what, code, cmd);
    }
    refused_as(
        &mut fx,
        "skip of an unknown plan",
        "not_found",
        skip_cmd(unknown, start()),
    );
    refused_as(
        &mut fx,
        "reopen of an unknown plan",
        "not_found",
        reopen_cmd(unknown, start()),
    );
    refused_as(
        &mut fx,
        "reopen with nothing decided",
        "not_found",
        reopen_cmd(plan, start()),
    );

    // The vault is intact: a sound execution goes through.
    run(
        &mut fx.core,
        fx.vault,
        execute_at(plan, start(), total, &[(capped, 1_000), (open, 500)]),
    );
    assert_eq!(balance_of(&fx, capped), 1_000);
    assert_eq!(balance_of(&fx, open), 500);
    assert_eq!(balance_of(&fx, fx.unallocated), 1_000_000 + 10_000 - 1_500);
    assert_eq!(runs_of(&fx).len(), 1);
}

#[test]
fn a_refused_plan_leaves_the_vault_without_one_and_a_second_plan_is_refused() {
    let mut fx = setup();
    let a = envelope(&mut fx, "A", FlowMode::Unlimited);
    let b = envelope(&mut fx, "B", FlowMode::NetCapped { cap: 100 });
    let schedule = monthly_from(1, start());
    let mut bad_schedule = schedule;
    bad_schedule.interval = 0;
    let cases = [
        ("no lines", "invalid_command", plan_cmd(schedule, vec![])),
        (
            "the same envelope twice",
            "invalid_command",
            plan_cmd(schedule, vec![fixed(a, 10), percent(a, 10)]),
        ),
        (
            "Unallocated",
            "invalid_flow",
            plan_cmd(schedule, vec![fixed(fx.unallocated, 10)]),
        ),
        (
            "a zero fixed amount",
            "invalid_amount",
            plan_cmd(schedule, vec![fixed(a, 0)]),
        ),
        (
            "a negative fixed amount",
            "invalid_amount",
            plan_cmd(schedule, vec![fixed(a, -1)]),
        ),
        (
            "zero basis points",
            "invalid_amount",
            plan_cmd(schedule, vec![percent(a, 0)]),
        ),
        (
            "more than the whole",
            "invalid_amount",
            plan_cmd(schedule, vec![percent(a, FULL_PERCENT_BP + 1)]),
        ),
        (
            "an unknown envelope",
            "not_found",
            plan_cmd(schedule, vec![fixed(Uuid::now_v7(), 10)]),
        ),
        (
            "a broken schedule",
            "invalid_command",
            plan_cmd(bad_schedule, vec![fixed(a, 10)]),
        ),
    ];
    for (what, code, cmd) in cases {
        refused_as(&mut fx, what, code, cmd);
        assert_eq!(fx.core.allocation_plan(fx.vault).unwrap(), None, "{what}");
    }

    let receipt = run(
        &mut fx.core,
        fx.vault,
        plan_cmd(schedule, vec![fixed(a, 10), fill(b)]),
    );
    let plan = fx.core.allocation_plan(fx.vault).unwrap().unwrap();
    assert_eq!(Some(plan.id), receipt.result_id);
    assert_eq!(plan.id, receipt.command_id, "the plan id is the command id");
    assert_eq!(plan.lines, vec![fixed(a, 10), fill(b)]);
    assert_eq!(plan.schedule, schedule);
    assert!(plan.enabled);
    assert_eq!(plan.created_by, "alice");

    let err = refused(
        &mut fx,
        "a second plan",
        plan_cmd(schedule, vec![percent(b, 100)]),
    );
    assert!(matches!(err, DomainError::AlreadyExists(_)), "{err}");
    assert_eq!(
        fx.core.allocation_plan(fx.vault).unwrap(),
        Some(plan.clone())
    );

    for (what, code, patch) in [
        (
            "an empty patch",
            "invalid_command",
            AllocationPlanPatch::default(),
        ),
        (
            "lines with Unallocated",
            "invalid_flow",
            AllocationPlanPatch {
                lines: Some(vec![fixed(fx.unallocated, 1)]),
                ..AllocationPlanPatch::default()
            },
        ),
        (
            "no lines",
            "invalid_command",
            AllocationPlanPatch {
                lines: Some(Vec::new()),
                ..AllocationPlanPatch::default()
            },
        ),
        (
            "a broken schedule",
            "invalid_command",
            AllocationPlanPatch {
                schedule: Some(bad_schedule),
                ..AllocationPlanPatch::default()
            },
        ),
    ] {
        refused_as(&mut fx, what, code, update_plan_cmd(plan.id, patch));
        assert_eq!(
            fx.core.allocation_plan(fx.vault).unwrap(),
            Some(plan.clone()),
            "{what}"
        );
    }
    refused_as(
        &mut fx,
        "an unknown plan",
        "not_found",
        update_plan_cmd(
            Uuid::now_v7(),
            AllocationPlanPatch {
                enabled: Some(false),
                ..AllocationPlanPatch::default()
            },
        ),
    );

    run(
        &mut fx.core,
        fx.vault,
        update_plan_cmd(
            plan.id,
            AllocationPlanPatch {
                lines: Some(vec![percent(b, 2_500)]),
                ..AllocationPlanPatch::default()
            },
        ),
    );
    let updated = fx.core.allocation_plan(fx.vault).unwrap().unwrap();
    assert_eq!(updated.lines, vec![percent(b, 2_500)]);
    assert_eq!(updated.schedule, schedule);
    assert_eq!(updated.id, plan.id);
}

// ---------------------------------------------------------------------------
// Engine: replay
// ---------------------------------------------------------------------------

#[test]
fn replaying_the_log_into_a_fresh_core_reproduces_the_plan_its_runs_and_its_base() {
    let mut fx = setup();
    fund(&mut fx);
    let a = envelope(&mut fx, "A", FlowMode::NetCapped { cap: 5_000 });
    let b = envelope(&mut fx, "B", FlowMode::Unlimited);
    let c = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("C", FlowMode::IncomeCapped { cap: 3_000 }, true, 0),
    )
    .result_id
    .unwrap();
    let plan = plan_from_start(&mut fx, vec![fixed(a, 800), percent(b, 1_000), fill(c)]);
    income_on(&mut fx, 20_000, day(2026, 1, 3));
    income_on(&mut fx, 5_000, day(2026, 1, 10));
    run(
        &mut fx.core,
        fx.vault,
        execute_at(plan, start(), 25_000, &[(a, 800), (b, 2_500), (c, 3_000)]),
    );
    let b_move = runs_of(&fx)[0]
        .moves
        .iter()
        .find(|m| m.flow_id == b)
        .unwrap()
        .transaction_id;
    void(&mut fx, b_move);
    income_on(&mut fx, 7_000, day(2026, 2, 2));
    run(&mut fx.core, fx.vault, skip_cmd(plan, day(2026, 2, 1)));
    income_on(&mut fx, 3_000, day(2026, 3, 1));
    run(&mut fx.core, fx.vault, reopen_cmd(plan, day(2026, 2, 1)));
    run(
        &mut fx.core,
        fx.vault,
        update_plan_cmd(
            plan,
            AllocationPlanPatch {
                schedule: Some(monthly_from(15, start())),
                lines: Some(vec![percent(a, 2_500), fixed(b, 100)]),
                enabled: None,
            },
        ),
    );
    run(
        &mut fx.core,
        fx.vault,
        execute_at(plan, day(2026, 3, 15), 10_000, &[(a, 2_500), (b, 100)]),
    );
    let a_move = runs_of(&fx)[0]
        .moves
        .iter()
        .find(|m| m.flow_id == a)
        .unwrap()
        .transaction_id;
    run(
        &mut fx.core,
        fx.vault,
        Command::UpdateTransaction {
            transaction_id: a_move,
            patch: TransactionPatch {
                amount: Some(1_000),
                ..TransactionPatch::default()
            },
        },
    );
    income_on(&mut fx, 900, day(2026, 3, 20));

    let original = projection(&fx.core, fx.vault);
    assert_eq!(original.runs.len(), 2);
    assert!(
        original.runs[1].moves.iter().any(|m| m.voided),
        "the voided move shows in the run"
    );
    assert_eq!(income_ids(&original.base).len(), 1);

    let log = fx.core.commands_since(fx.vault, 0).unwrap();
    let mut fresh = Core::open_in_memory().unwrap();
    replay(&log, &mut fresh).unwrap();
    assert_eq!(projection(&fresh, fx.vault), original);
    assert_eq!(
        fresh
            .pending_allocation(fx.vault, day(2026, 4, 20))
            .unwrap(),
        fx.core
            .pending_allocation(fx.vault, day(2026, 4, 20))
            .unwrap()
    );
}

// ---------------------------------------------------------------------------
// Engine: the base
// ---------------------------------------------------------------------------

#[test]
fn the_base_counts_only_live_incomes_into_unallocated_dated_from_the_plans_start() {
    let mut fx = setup();
    let e = envelope(&mut fx, "E", FlowMode::Unlimited);
    // A second wallet: from here on every entry names its wallet.
    let bank = run(
        &mut fx.core,
        fx.vault,
        Command::CreateWallet {
            name: "Bank".to_string(),
            opening_balance: 5_000,
            occurred_at: noon(day(2026, 1, 8)),
        },
    )
    .result_id
    .unwrap();
    let cash = fx.wallet;
    let income_via = |fx: &mut Fx, wallet: Uuid, amount: i64, date: NaiveDate| {
        run(
            &mut fx.core,
            fx.vault,
            Command::Income(entry(amount, Some(wallet), None, None, secs(date))),
        )
        .result_id
        .unwrap()
    };
    income_via(&mut fx, cash, 1_000, day(2025, 12, 30));
    let before_plan = income_via(&mut fx, cash, 1_200, day(2026, 1, 2));
    plan_from_start(&mut fx, vec![fixed(e, 10)]);

    let on_the_start = income_via(&mut fx, cash, 1_100, start());
    let plain = income_via(&mut fx, cash, 2_000, day(2026, 1, 5));
    let voided = income_via(&mut fx, cash, 3_000, day(2026, 1, 6));
    void(&mut fx, voided);
    run(
        &mut fx.core,
        fx.vault,
        Command::Refund(entry(400, Some(bank), None, None, secs(day(2026, 1, 9)))),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(
            600,
            Some(cash),
            Some(e),
            None,
            secs(day(2026, 1, 10)),
        )),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::TransferFlow {
            amount: 50,
            from_flow_id: e,
            to_flow_id: fx.unallocated,
            note: None,
            occurred_at: noon(day(2026, 1, 11)),
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(70, Some(cash), None, None, secs(day(2026, 1, 12)))),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::TransferWallet {
            amount: 30,
            from_wallet_id: bank,
            to_wallet_id: fx.wallet,
            note: None,
            occurred_at: noon(day(2026, 1, 12)),
        },
    );
    let on_the_bank = run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(
            700,
            Some(bank),
            None,
            Some("Salary"),
            secs(day(2026, 1, 13)),
        )),
    )
    .result_id
    .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Late", FlowMode::NetCapped { cap: 200 }, false, 150),
    );

    let base = base_of(&fx);
    assert_eq!(
        income_ids(&base),
        vec![on_the_start, before_plan, plain, on_the_bank],
        "oldest first, by date"
    );
    assert_eq!(base.total, 1_100 + 1_200 + 2_000 + 700);
    assert_eq!(
        base.total,
        base.incomes.iter().map(|t| t.amount).sum::<i64>()
    );
    for t in &base.incomes {
        assert_eq!(t.kind, TransactionKind::Income);
        assert!(!t.voided);
        assert_eq!(t.flow_id, Some(fx.unallocated));
    }
}

#[test]
fn the_base_counts_recurring_executions_batched_incomes_and_edited_amounts() {
    let mut fx = setup();
    let e = envelope(&mut fx, "E", FlowMode::Unlimited);
    plan_from_start(&mut fx, vec![fixed(e, 10)]);

    let template = run(
        &mut fx.core,
        fx.vault,
        Command::CreateRecurring {
            transaction_kind: TransactionKind::Income,
            amount: 2_500,
            wallet_id: None,
            flow_id: None,
            category: Some("Salary".to_string()),
            note: None,
            schedule: monthly_from(1, start()),
            owner: None,
        },
    )
    .result_id
    .unwrap();
    let recurring = run(
        &mut fx.core,
        fx.vault,
        Command::ExecuteRecurring {
            recurring_id: template,
            period_date: start(),
            occurred_at: noon(start()),
            person: None,
        },
    )
    .result_id
    .unwrap();

    let batch: Vec<CommandEnvelope> = [(10, 3), (20, 4), (30, 5)]
        .into_iter()
        .map(|(amount, d)| {
            CommandEnvelope::new(
                fx.vault,
                "alice",
                Command::Income(entry(amount, None, None, None, secs(day(2026, 1, d)))),
            )
        })
        .collect();
    let batched: Vec<Uuid> = fx
        .core
        .execute_batch(batch)
        .unwrap()
        .into_iter()
        .map(|r| r.result_id.unwrap())
        .collect();

    let edited = income_on(&mut fx, 1_000, day(2026, 1, 6));
    run(
        &mut fx.core,
        fx.vault,
        Command::UpdateTransaction {
            transaction_id: edited,
            patch: TransactionPatch {
                amount: Some(1_500),
                ..TransactionPatch::default()
            },
        },
    );
    let moved_out = income_on(&mut fx, 500, day(2026, 1, 7));
    run(
        &mut fx.core,
        fx.vault,
        Command::UpdateTransaction {
            transaction_id: moved_out,
            patch: TransactionPatch {
                flow_id: Some(e),
                ..TransactionPatch::default()
            },
        },
    );
    let moved_in = run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(800, None, Some(e), None, secs(day(2026, 1, 8)))),
    )
    .result_id
    .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::UpdateTransaction {
            transaction_id: moved_in,
            patch: TransactionPatch {
                flow_id: Some(fx.unallocated),
                ..TransactionPatch::default()
            },
        },
    );

    let base = base_of(&fx);
    let mut expected = vec![recurring];
    expected.extend(batched);
    expected.push(edited);
    expected.push(moved_in);
    assert_eq!(income_ids(&base), expected);
    assert_eq!(base.total, 2_500 + 60 + 1_500 + 800);
}

#[test]
fn each_income_is_counted_exactly_once_across_consecutive_decisions() {
    let mut fx = setup();
    fund(&mut fx);
    let e = envelope(&mut fx, "E", FlowMode::Unlimited);
    let plan = plan_from_start(&mut fx, vec![fixed(e, 500)]);
    let empty = AllocationBase {
        total: 0,
        incomes: Vec::new(),
    };

    let i1 = income_on(&mut fx, 1_000, day(2026, 1, 3));
    let i2 = income_on(&mut fx, 2_000, day(2026, 1, 10));
    let first = base_of(&fx);
    assert_eq!(income_ids(&first), vec![i1, i2]);
    assert_eq!(first.total, 3_000);
    run(
        &mut fx.core,
        fx.vault,
        execute_at(plan, start(), 3_000, &[(e, 500)]),
    );
    assert_eq!(base_of(&fx), empty, "nothing left after the decision");

    // Recorded after the run, dated before it: what counts is when it was
    // recorded.
    let backdated = start().and_hms_opt(6, 0, 0).unwrap().and_utc().timestamp();
    let i3 = income_in(&mut fx, 300, backdated);
    assert_eq!(income_ids(&base_of(&fx)), vec![i3]);
    let i4 = income_on(&mut fx, 4_000, day(2026, 2, 5));
    let second = base_of(&fx);
    assert_eq!(income_ids(&second), vec![i3, i4]);
    assert_eq!(second.total, 4_300);

    run(&mut fx.core, fx.vault, skip_cmd(plan, day(2026, 2, 1)));
    assert_eq!(base_of(&fx), empty, "a skip closes its incomes too");
    let i5 = income_on(&mut fx, 500, day(2026, 2, 20));
    assert_eq!(income_ids(&base_of(&fx)), vec![i5]);

    run(&mut fx.core, fx.vault, reopen_cmd(plan, day(2026, 2, 1)));
    let reopened = base_of(&fx);
    assert_eq!(
        income_ids(&reopened),
        vec![i3, i4, i5],
        "a reopen gives the incomes back to the base, the newer ones included"
    );
    assert_eq!(reopened.total, 4_800);
    run(
        &mut fx.core,
        fx.vault,
        execute_at(plan, day(2026, 2, 1), 4_800, &[(e, 500)]),
    );
    assert_eq!(base_of(&fx), empty);
    let i6 = income_on(&mut fx, 60, day(2026, 3, 3));
    assert_eq!(income_ids(&base_of(&fx)), vec![i6]);

    // The decisions that stand split the incomes with nothing shared and
    // nothing left out.
    let mut seen: Vec<Uuid> = income_ids(&first);
    seen.extend(income_ids(&reopened));
    let mut all_incomes = vec![i1, i2, i3, i4, i5];
    seen.sort();
    all_incomes.sort();
    assert_eq!(seen, all_incomes);
}

#[test]
fn the_start_of_the_plan_is_read_in_the_incomes_own_offset() {
    let mut fx = setup();
    let e = envelope(&mut fx, "E", FlowMode::Unlimited);
    plan_from_start(&mut fx, vec![fixed(e, 10)]);
    let income_at = |fx: &mut Fx, amount: i64, at: DateTime<FixedOffset>| {
        let mut income = entry(amount, None, None, None, 0);
        income.occurred_at = at;
        run(&mut fx.core, fx.vault, Command::Income(income))
            .result_id
            .unwrap()
    };
    let rome = FixedOffset::east_opt(2 * 3600).unwrap();
    let new_york = FixedOffset::west_opt(5 * 3600).unwrap();
    // Half past midnight on the start day in Rome is still the day before
    // in UTC: the income counts.
    let counted = income_at(
        &mut fx,
        100,
        rome.with_ymd_and_hms(2026, 1, 1, 0, 30, 0).unwrap(),
    );
    // Half past eleven the night before in New York is already the start
    // day in UTC: it does not.
    income_at(
        &mut fx,
        200,
        new_york.with_ymd_and_hms(2025, 12, 31, 23, 30, 0).unwrap(),
    );

    let base = base_of(&fx);
    assert_eq!(income_ids(&base), vec![counted]);
    assert_eq!(base.total, 100);
}

/// The card lists its incomes as the ledger does, so the two never disagree
/// on the order of a day: by date, then by id, whatever the log says.
#[test]
fn incomes_on_the_same_instant_keep_the_ledgers_order() {
    let mut fx = setup();
    let e = envelope(&mut fx, "E", FlowMode::Unlimited);
    plan_from_start(&mut fx, vec![fixed(e, 10)]);
    let at = secs(day(2026, 1, 5));
    // The income recorded second carries the smaller id.
    let envelope_with = |id: u128, amount: i64| CommandEnvelope {
        id: Uuid::from_u128(id),
        vault_id: fx.vault,
        author: "alice".to_string(),
        command: Command::Income(entry(amount, None, None, None, at)),
    };
    let first = envelope_with(0x0200, 100);
    let second = envelope_with(0x0100, 200);
    let first_id = fx.core.execute(first).unwrap().result_id.unwrap();
    let second_id = fx.core.execute(second).unwrap().result_id.unwrap();
    assert!(second_id < first_id);

    let ledger: Vec<Uuid> = list(
        &fx.core,
        fx.vault,
        &TransactionFilter {
            ascending: true,
            ..all()
        },
    )
    .into_iter()
    .map(|t| t.id)
    .filter(|id| *id == first_id || *id == second_id)
    .collect();
    assert_eq!(ledger, vec![second_id, first_id]);
    assert_eq!(
        income_ids(&base_of(&fx)),
        ledger,
        "oldest first, then by id, as the ledger shows them"
    );
}

// ---------------------------------------------------------------------------
// Engine: reopen
// ---------------------------------------------------------------------------

fn three_envelopes(fx: &mut Fx) -> (Uuid, Uuid, Uuid) {
    let a = envelope(fx, "A", FlowMode::NetCapped { cap: 5_000 });
    let b = envelope(fx, "B", FlowMode::Unlimited);
    let c = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("C", FlowMode::IncomeCapped { cap: 3_000 }, true, 0),
    )
    .result_id
    .unwrap();
    (a, b, c)
}

#[test]
fn reopening_the_latest_period_restores_every_balance_and_makes_it_due_again() {
    let mut fx = setup();
    fund(&mut fx);
    let (a, b, c) = three_envelopes(&mut fx);
    let plan = plan_from_start(&mut fx, vec![fixed(a, 800), fixed(b, 1_000), fill(c)]);
    income_on(&mut fx, 20_000, day(2026, 1, 3));
    income_on(&mut fx, 5_000, day(2026, 1, 10));
    let before = projection(&fx.core, fx.vault);

    run(
        &mut fx.core,
        fx.vault,
        execute_at(plan, start(), 25_000, &[(a, 800), (b, 1_000), (c, 500)]),
    );
    assert_ne!(fx.core.snapshot(fx.vault).unwrap(), before.snapshot);
    let moved: Vec<Uuid> = runs_of(&fx)[0]
        .moves
        .iter()
        .map(|m| m.transaction_id)
        .collect();
    assert_eq!(moved.len(), 3);

    run(&mut fx.core, fx.vault, reopen_cmd(plan, start()));
    let after = projection(&fx.core, fx.vault);
    assert_eq!(
        after.snapshot, before.snapshot,
        "balances, caps, income totals"
    );
    assert_eq!(after.base, before.base);
    assert_eq!(after.pending, before.pending);
    assert_eq!(
        pending_on(&fx, day(2026, 1, 20)),
        Some(PendingAllocation {
            plan_id: plan,
            period_date: start(),
            missed: 0
        })
    );
    assert!(after.runs.is_empty());
    assert_eq!(after.plan, before.plan);
    assert_eq!(after.transactions.len(), before.transactions.len() + 3);
    for id in &moved {
        let t = after.transactions.iter().find(|t| t.id == *id).unwrap();
        assert!(t.voided, "the transfer stays in the ledger, voided");
    }
    assert_eq!(after.log.len(), before.log.len() + 2);

    // The period can be decided again, from the same base.
    run(
        &mut fx.core,
        fx.vault,
        execute_at(plan, start(), 25_000, &[(a, 800), (b, 1_000), (c, 500)]),
    );
    assert_eq!(runs_of(&fx).len(), 1);
    assert_eq!(balance_of(&fx, a), 800);
    assert_eq!(balance_of(&fx, b), 1_000);
    assert_eq!(balance_of(&fx, c), 500);
    assert_eq!(transfers_of(&fx).len(), 6);
}

#[test]
fn reopen_tolerates_a_move_voided_or_edited_in_the_meantime() {
    let mut fx = setup();
    fund(&mut fx);
    let (a, b, c) = three_envelopes(&mut fx);
    let plan = plan_from_start(&mut fx, vec![fixed(a, 800), fixed(b, 1_000), fill(c)]);
    income_on(&mut fx, 20_000, day(2026, 1, 3));
    let before = projection(&fx.core, fx.vault);

    run(
        &mut fx.core,
        fx.vault,
        execute_at(plan, start(), 20_000, &[(a, 800), (b, 1_000), (c, 500)]),
    );
    let moves = runs_of(&fx)[0].moves.clone();
    let of = |flow: Uuid| {
        moves
            .iter()
            .find(|m| m.flow_id == flow)
            .unwrap()
            .transaction_id
    };
    void(&mut fx, of(b));
    run(
        &mut fx.core,
        fx.vault,
        Command::UpdateTransaction {
            transaction_id: of(a),
            patch: TransactionPatch {
                amount: Some(300),
                ..TransactionPatch::default()
            },
        },
    );
    assert_eq!(balance_of(&fx, a), 300);
    assert_eq!(balance_of(&fx, b), 0);
    let run_view = runs_of(&fx).remove(0);
    let as_now: Vec<(i64, bool)> = [a, b, c]
        .iter()
        .map(|f| {
            let m = run_view.moves.iter().find(|m| m.flow_id == *f).unwrap();
            (m.amount, m.voided)
        })
        .collect();
    assert_eq!(
        as_now,
        vec![(300, false), (1_000, true), (500, false)],
        "the run shows its moves as they are now"
    );

    run(&mut fx.core, fx.vault, reopen_cmd(plan, start()));
    let after = projection(&fx.core, fx.vault);
    assert_eq!(after.snapshot, before.snapshot);
    assert_eq!(after.base, before.base);
    assert!(after.runs.is_empty());
    assert!(transfers_of(&fx).iter().all(|t| t.voided));
}

#[test]
fn reopen_is_refused_on_a_period_that_is_not_the_latest_decided_one() {
    let mut fx = setup();
    fund(&mut fx);
    let a = envelope(&mut fx, "A", FlowMode::Unlimited);
    let plan = plan_from_start(&mut fx, vec![fixed(a, 100)]);
    income_on(&mut fx, 1_000, day(2026, 1, 3));
    run(
        &mut fx.core,
        fx.vault,
        execute_at(plan, start(), 1_000, &[(a, 100)]),
    );
    run(&mut fx.core, fx.vault, skip_cmd(plan, day(2026, 2, 1)));

    refused_as(
        &mut fx,
        "an older decided period",
        "invalid_command",
        reopen_cmd(plan, start()),
    );
    refused_as(
        &mut fx,
        "a period never decided",
        "not_found",
        reopen_cmd(plan, day(2026, 3, 1)),
    );
    refused_as(
        &mut fx,
        "a day that is no period",
        "not_found",
        reopen_cmd(plan, day(2026, 2, 2)),
    );

    let transactions = list(&fx.core, fx.vault, &all());
    run(&mut fx.core, fx.vault, reopen_cmd(plan, day(2026, 2, 1)));
    assert_eq!(
        list(&fx.core, fx.vault, &all()),
        transactions,
        "a skipped period had nothing to void"
    );
    let periods: Vec<NaiveDate> = runs_of(&fx).iter().map(|r| r.period_date).collect();
    assert_eq!(periods, vec![start()]);
    assert_eq!(balance_of(&fx, a), 100);

    run(&mut fx.core, fx.vault, reopen_cmd(plan, start()));
    assert!(runs_of(&fx).is_empty());
    assert_eq!(balance_of(&fx, a), 0);
    refused_as(
        &mut fx,
        "nothing decided any more",
        "not_found",
        reopen_cmd(plan, start()),
    );
}

#[test]
fn reopen_never_consults_the_schedule_or_the_switch() {
    let mut fx = setup();
    fund(&mut fx);
    let a = envelope(&mut fx, "A", FlowMode::Unlimited);
    let plan = plan_from_start(&mut fx, vec![fixed(a, 100)]);
    income_on(&mut fx, 1_000, day(2026, 1, 3));
    let before = projection(&fx.core, fx.vault);
    run(
        &mut fx.core,
        fx.vault,
        execute_at(plan, start(), 1_000, &[(a, 100)]),
    );

    // The 1st is no period of the plan any more, and the plan is off.
    run(
        &mut fx.core,
        fx.vault,
        update_plan_cmd(
            plan,
            AllocationPlanPatch {
                schedule: Some(monthly_from(15, start())),
                lines: None,
                enabled: Some(false),
            },
        ),
    );
    let schedule = fx.core.allocation_plan(fx.vault).unwrap().unwrap().schedule;
    assert!(!schedule.is_occurrence(start()));
    refused_as(
        &mut fx,
        "a period of a plan switched off",
        "invalid_command",
        execute_at(plan, day(2026, 1, 15), 100, &[(a, 1)]),
    );

    run(&mut fx.core, fx.vault, reopen_cmd(plan, start()));
    let after = projection(&fx.core, fx.vault);
    assert_eq!(
        after.snapshot, before.snapshot,
        "a run row is enough to undo"
    );
    assert_eq!(after.base, before.base);
    assert!(after.runs.is_empty());
    assert!(transfers_of(&fx).iter().all(|t| t.voided));

    // Switched back on, the first period of the new schedule is due.
    run(
        &mut fx.core,
        fx.vault,
        update_plan_cmd(
            plan,
            AllocationPlanPatch {
                enabled: Some(true),
                ..AllocationPlanPatch::default()
            },
        ),
    );
    assert_eq!(
        pending_on(&fx, day(2026, 1, 20)),
        Some(PendingAllocation {
            plan_id: plan,
            period_date: day(2026, 1, 15),
            missed: 0
        })
    );
}

#[test]
fn a_future_occurrence_can_be_decided_and_closes_what_lies_before_it() {
    let mut fx = setup();
    fund(&mut fx);
    let a = envelope(&mut fx, "A", FlowMode::Unlimited);
    let plan = plan_from_start(&mut fx, vec![fixed(a, 100)]);
    income_on(&mut fx, 1_000, day(2026, 1, 3));

    // The engine knows no today: any period of the schedule can be decided,
    // and the app only ever offers the one due.
    run(
        &mut fx.core,
        fx.vault,
        execute_at(plan, day(2026, 6, 1), 1_000, &[(a, 100)]),
    );
    assert_eq!(pending_on(&fx, day(2026, 3, 15)), None);
    assert_eq!(pending_on(&fx, day(2026, 6, 30)), None);
    assert_eq!(
        pending_on(&fx, day(2026, 7, 1)),
        Some(PendingAllocation {
            plan_id: plan,
            period_date: day(2026, 7, 1),
            missed: 0
        })
    );
    refused_as(
        &mut fx,
        "a period before the one decided",
        "invalid_command",
        execute_at(plan, day(2026, 3, 1), 100, &[(a, 1)]),
    );
    assert_eq!(runs_of(&fx).len(), 1);
    assert_eq!(base_of(&fx).incomes.len(), 0);
}

// ---------------------------------------------------------------------------
// Engine: two vaults
// ---------------------------------------------------------------------------

#[test]
fn a_plan_of_another_vault_is_refused_on_every_command() {
    let mut fx = setup();
    fund(&mut fx);
    let a = envelope(&mut fx, "A", FlowMode::Unlimited);
    let plan = plan_from_start(&mut fx, vec![fixed(a, 100)]);
    income_on(&mut fx, 1_000, day(2026, 1, 3));

    let other = fx
        .core
        .execute(CommandEnvelope::create_vault(
            "alice",
            "Other",
            Currency::Eur,
        ))
        .unwrap()
        .result_id
        .unwrap();
    run(&mut fx.core, other, wallet_cmd("Cash", 0));
    let theirs = run(
        &mut fx.core,
        other,
        flow_cmd("Theirs", FlowMode::Unlimited, false, 0),
    )
    .result_id
    .unwrap();

    let mine = projection(&fx.core, fx.vault);
    let not_mine = projection(&fx.core, other);
    let cases = [
        (
            "execute",
            execute_at(plan, start(), 1_000, &[(theirs, 100)]),
        ),
        (
            "execute from their vault with my envelope",
            execute_at(plan, start(), 1_000, &[(a, 100)]),
        ),
        ("skip", skip_cmd(plan, start())),
        ("reopen", reopen_cmd(plan, start())),
        (
            "update",
            update_plan_cmd(
                plan,
                AllocationPlanPatch {
                    enabled: Some(false),
                    ..AllocationPlanPatch::default()
                },
            ),
        ),
    ];
    for (what, cmd) in cases {
        let err = try_run(&mut fx.core, other, cmd).expect_err(what);
        assert!(matches!(err, DomainError::NotFound(_)), "{what}: {err}");
        assert_eq!(projection(&fx.core, fx.vault), mine, "{what}");
        assert_eq!(projection(&fx.core, other), not_mine, "{what}");
    }
    assert_eq!(fx.core.allocation_plan(other).unwrap(), None);
    assert_eq!(
        fx.core.pending_allocation(other, day(2026, 6, 1)).unwrap(),
        None
    );

    // A move into their envelope from my plan is refused too.
    refused_as(
        &mut fx,
        "their envelope as a target",
        "not_found",
        execute_at(plan, start(), 1_000, &[(a, 50), (theirs, 50)]),
    );
    assert_eq!(projection(&fx.core, other), not_mine);

    // Each vault has its own plan.
    run(
        &mut fx.core,
        other,
        plan_cmd(monthly_from(1, start()), vec![fixed(theirs, 1)]),
    );
    assert_eq!(projection(&fx.core, fx.vault), mine);
    assert!(fx.core.allocation_plan(other).unwrap().is_some());
}

// ---------------------------------------------------------------------------
// Engine: headroom agreement
// ---------------------------------------------------------------------------

#[test]
fn the_amounts_of_a_preview_executed_as_moves_are_never_refused_for_a_cap() {
    let mut g = Gen::new(31);
    let mut executed = 0;
    for case in 0..16 {
        let mut fx = setup();
        fund(&mut fx);
        let mut pool: Vec<Uuid> = (0..g.index(4) + 2)
            .map(|i| shaped_envelope(&mut g, &mut fx, i))
            .collect();
        pool.push(archived_envelope(&mut fx, "Gone"));
        let lines = plan_lines(&mut g, &pool);
        let plan = plan_from_start(&mut fx, lines.clone());
        for _ in 0..=g.index(3) {
            let amount = g.range(100, 50_000);
            let date = day(2026, 1, u32::try_from(g.range(2, 28)).unwrap());
            income_on(&mut fx, amount, date);
        }
        let base = base_of(&fx);
        let total = match g.below(10) {
            0 => 0,
            1 | 2 => base.total + g.range(1, 100_000),
            _ => base.total,
        };

        let snapshot = fx.core.snapshot(fx.vault).unwrap();
        let preview = fx.core.preview_allocation(fx.vault, &lines, total).unwrap();
        assert_eq!(
            preview,
            resolve(
                total,
                &lines,
                &snapshot.flows,
                balance_of(&fx, fx.unallocated)
            ),
            "case {case}: the preview reads the projection"
        );
        let moves: Vec<(Uuid, i64)> = preview
            .lines
            .iter()
            .filter(|l| l.amount > 0)
            .map(|l| (l.flow_id, l.amount))
            .collect();
        if moves.is_empty() {
            continue;
        }
        executed += 1;
        let result = try_run(
            &mut fx.core,
            fx.vault,
            execute_at(plan, start(), total, &moves),
        );
        assert!(
            result.is_ok(),
            "case {case}: {moves:?} refused with {:?}",
            result.err()
        );
        for line in &preview.lines {
            if line.status != LineStatus::Archived {
                assert_eq!(
                    balance_of(&fx, line.flow_id),
                    line.balance_after,
                    "case {case}: balance after"
                );
            }
        }
        assert_eq!(
            balance_of(&fx, fx.unallocated),
            preview.unallocated_after,
            "case {case}: Unallocated after"
        );
    }
    assert!(executed >= 8, "only {executed} previews had a move");
}
