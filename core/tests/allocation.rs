//! Scenario tests for the allocation plan on a household of three
//! envelopes, for what the generated cases of `allocation_invariants.rs` do
//! not reach: a note kept, a move voided on its own, a long backlog, the ids
//! of the moves and the history of decided periods.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::NaiveDate;
use common::{
    sync::{base_of, pending_on, runs_of, secs},
    *,
};
use sparagne_core::{
    AllocationBase, AllocationPlanPatch, AllocationPlanView, AllocationRunView, Command,
    DomainError, FlowMode, LineStatus, Receipt, RunMove, RunOutcome, Schedule,
};
use uuid::Uuid;

/// The id of the transfer an execution makes to `flow`.
fn move_id(command_id: Uuid, flow: Uuid) -> Uuid {
    Uuid::new_v5(&command_id, format!("allocation:{flow}").as_bytes())
}

/// A vault with three envelopes and the plan of the examples: rent 800,
/// savings 10% and groceries up to their 400 cap, on the 1st of every month
/// from 2026-01-01.
struct Household {
    fx: Fx,
    plan: Uuid,
    rent: Uuid,
    savings: Uuid,
    food: Uuid,
}

fn household() -> Household {
    let mut fx = setup();
    let rent = envelope(&mut fx, "Rent", FlowMode::Unlimited);
    let savings = envelope(&mut fx, "Savings", FlowMode::Unlimited);
    let food = envelope(&mut fx, "Food", FlowMode::NetCapped { cap: 40_000 });
    let plan = fx.id(plan_cmd(
        monthly_from(1, day(2026, 1, 1)),
        vec![fixed(rent, 80_000), percent(savings, 1000), fill(food)],
    ));
    Household {
        fx,
        plan,
        rent,
        savings,
        food,
    }
}

impl Household {
    fn income(&mut self, amount: i64, secs: i64) -> Uuid {
        income_in(&mut self.fx, amount, secs)
    }

    fn plan_view(&self) -> AllocationPlanView {
        self.fx
            .core
            .allocation_plan(self.fx.vault)
            .unwrap()
            .unwrap()
    }

    fn flow(&self, id: Uuid) -> i64 {
        flow_balance(&self.fx.core, self.fx.vault, id)
    }

    /// The moves the app sends for `period`: the plan worked out on the
    /// base, without the lines that get nothing.
    fn moves(&self) -> (i64, Vec<(Uuid, i64)>) {
        let total = base_of(&self.fx).total;
        let preview = self
            .fx
            .core
            .preview_allocation(self.fx.vault, &self.plan_view().lines, total)
            .unwrap();
        let moves = preview
            .lines
            .iter()
            .filter(|line| line.amount > 0)
            .map(|line| (line.flow_id, line.amount))
            .collect();
        (total, moves)
    }

    /// Executes `period` with what the preview says.
    fn confirm(&mut self, period: NaiveDate) -> Receipt {
        let (total, moves) = self.moves();
        self.fx.run(execute_cmd(self.plan, period, total, &moves))
    }
}

#[test]
fn create_takes_archived_and_uncapped_envelopes_and_the_preview_skips_them() {
    let mut fx = setup();
    let old = envelope(&mut fx, "Old", FlowMode::NetCapped { cap: 5_000 });
    let open = envelope(&mut fx, "Open", FlowMode::Unlimited);
    let rent = envelope(&mut fx, "Rent", FlowMode::Unlimited);
    fx.run(Command::ArchiveFlow { flow_id: old });

    let lines = vec![fixed(old, 1_000), fill(open), fixed(rent, 1_000)];
    fx.run(plan_cmd(monthly_from(1, day(2026, 1, 1)), lines.clone()));

    let preview = fx.core.preview_allocation(fx.vault, &lines, 5_000).unwrap();
    let statuses: Vec<_> = preview.lines.iter().map(|l| (l.amount, l.status)).collect();
    assert_eq!(
        statuses,
        vec![
            (0, LineStatus::Archived),
            (0, LineStatus::NoCap),
            (1_000, LineStatus::Full),
        ]
    );
    assert_eq!(preview.remainder, 4_000);
}

#[test]
fn update_changes_the_plan_but_never_its_runs() {
    let mut h = household();
    h.income(200_000, secs(day(2026, 1, 1)));
    h.confirm(day(2026, 1, 1));
    let runs = runs_of(&h.fx);

    let schedule = monthly_from(15, day(2026, 2, 1));
    h.fx.run(update_plan_cmd(
        h.plan,
        AllocationPlanPatch {
            schedule: Some(schedule),
            lines: Some(vec![percent(h.savings, 2500), fixed(h.rent, 70_000)]),
            enabled: None,
        },
    ));
    let plan = h.plan_view();
    assert_eq!(plan.schedule, schedule);
    assert_eq!(
        plan.lines,
        vec![percent(h.savings, 2500), fixed(h.rent, 70_000)]
    );
    assert!(plan.enabled);
    assert_eq!(runs_of(&h.fx), runs);
    assert_eq!(
        pending_on(&h.fx, day(2026, 3, 20)).map(|p| p.period_date),
        Some(day(2026, 3, 15))
    );

    // Disabled: nothing is due and nothing can be decided.
    h.fx.run(update_plan_cmd(
        h.plan,
        AllocationPlanPatch {
            enabled: Some(false),
            ..AllocationPlanPatch::default()
        },
    ));
    assert!(!h.plan_view().enabled);
    assert_eq!(h.plan_view().lines.len(), 2);
    assert_eq!(pending_on(&h.fx, day(2026, 3, 20)), None);
    let plan = h.plan;
    for cmd in [
        execute_cmd(plan, day(2026, 2, 15), 100, &[(h.rent, 100)]),
        skip_cmd(plan, day(2026, 2, 15)),
    ] {
        assert_eq!(
            h.fx.try_run(cmd).unwrap_err(),
            DomainError::InvalidCommand("allocation plan is disabled".to_string())
        );
    }
    assert_eq!(runs_of(&h.fx), runs);
}

#[test]
fn execute_keeps_the_note_and_may_reach_envelopes_outside_the_plan() {
    let mut h = household();
    let extra = envelope(&mut h.fx, "Extra", FlowMode::Unlimited);
    h.income(10_000, secs(day(2026, 1, 1)));
    let receipt = h.fx.run(Command::ExecuteAllocation {
        plan_id: h.plan,
        period_date: day(2026, 1, 1),
        occurred_at: noon(day(2026, 1, 2)),
        total: 10_000,
        moves: vec![sparagne_core::AllocationMove {
            flow_id: extra,
            amount: 2_500,
        }],
        note: Some("  January  ".to_string()),
    });
    let t =
        h.fx.core
            .transaction(h.fx.vault, move_id(receipt.command_id, extra))
            .unwrap();
    assert_eq!(t.note.as_deref(), Some("January"));
    assert_eq!(t.occurred_at, noon(day(2026, 1, 2)));
    assert_eq!(h.flow(extra), 2_500);
    assert_eq!(h.flow(h.fx.unallocated), 7_500);
}

#[test]
fn skip_decides_the_period_without_moving_anything() {
    let mut h = household();
    h.income(100_000, secs(day(2026, 1, 1)));
    let before = balances(&h.fx.core, h.fx.vault);
    let transactions = list(&h.fx.core, h.fx.vault, &all());

    let receipt = h.fx.run(skip_cmd(h.plan, day(2026, 1, 1)));
    assert_eq!(receipt.result_id, None);
    assert_eq!(balances(&h.fx.core, h.fx.vault), before);
    assert_eq!(list(&h.fx.core, h.fx.vault, &all()), transactions);
    assert_eq!(
        runs_of(&h.fx),
        vec![AllocationRunView {
            period_date: day(2026, 1, 1),
            outcome: RunOutcome::Skipped,
            total: 0,
            moves: Vec::new(),
            created_by: "alice".to_string(),
        }]
    );
    // The skipped period's incomes stay in Unallocated for good.
    assert_eq!(base_of(&h.fx).total, 0);
    assert_eq!(
        pending_on(&h.fx, day(2026, 2, 3)).map(|p| p.period_date),
        Some(day(2026, 2, 1))
    );
}

#[test]
fn voiding_one_move_leaves_the_run_and_the_others() {
    let mut h = household();
    h.income(200_000, secs(day(2026, 1, 1)));
    let receipt = h.confirm(day(2026, 1, 1));
    h.fx.run(Command::VoidTransaction {
        transaction_id: move_id(receipt.command_id, h.rent),
    });

    assert_eq!(h.flow(h.rent), 0);
    assert_eq!(h.flow(h.savings), 20_000);
    assert_eq!(h.flow(h.fx.unallocated), 140_000);
    let run = &runs_of(&h.fx)[0];
    assert_eq!(run.outcome, RunOutcome::Executed);
    assert_eq!(
        run.moves.iter().map(|m| m.voided).collect::<Vec<_>>(),
        vec![true, false, false]
    );
    // The period stays decided, and the base stays empty.
    assert_eq!(pending_on(&h.fx, day(2026, 1, 20)), None);
    assert_eq!(base_of(&h.fx).total, 0);
}

#[test]
fn the_base_and_the_rest_are_empty_without_a_plan() {
    let mut fx = setup();
    income_in(&mut fx, 10_000, secs(day(2026, 1, 1)));
    assert_eq!(
        fx.core.allocation_base(fx.vault).unwrap(),
        AllocationBase {
            total: 0,
            incomes: Vec::new(),
        }
    );
    assert_eq!(fx.core.allocation_plan(fx.vault).unwrap(), None);
    assert_eq!(
        fx.core
            .pending_allocation(fx.vault, day(2026, 6, 1))
            .unwrap(),
        None
    );
    assert!(fx.core.allocation_runs(fx.vault, 10).unwrap().is_empty());
}

#[test]
fn pending_counts_a_long_daily_backlog() {
    let mut fx = setup();
    let rent = envelope(&mut fx, "Rent", FlowMode::Unlimited);
    let start = day(2000, 1, 1);
    fx.run(plan_cmd(
        Schedule {
            frequency: sparagne_core::Frequency::Daily,
            interval: 1,
            start_date: start,
            end_date: None,
        },
        vec![fixed(rent, 100)],
    ));
    let today = day(2026, 10, 10);
    let pending = fx
        .core
        .pending_allocation(fx.vault, today)
        .unwrap()
        .unwrap();
    assert_eq!(pending.period_date, today);
    assert_eq!(i64::from(pending.missed), (today - start).num_days());
}

#[test]
fn runs_list_the_decided_periods_most_recent_first() {
    let mut h = household();
    h.income(200_000, secs(day(2026, 1, 1)));
    let january = h.confirm(day(2026, 1, 1));
    h.fx.run(skip_cmd(h.plan, day(2026, 2, 1)));
    h.income(10_000, secs(day(2026, 2, 20)));
    let rent = h.rent;
    let march = h.fx.run(execute_cmd(
        h.plan,
        day(2026, 3, 1),
        10_000,
        &[(rent, 9_000)],
    ));

    let runs = runs_of(&h.fx);
    let summary: Vec<_> = runs
        .iter()
        .map(|r| (r.period_date, r.outcome, r.total, r.moves.len()))
        .collect();
    assert_eq!(
        summary,
        vec![
            (day(2026, 3, 1), RunOutcome::Executed, 10_000, 1),
            (day(2026, 2, 1), RunOutcome::Skipped, 0, 0),
            (day(2026, 1, 1), RunOutcome::Executed, 200_000, 3),
        ]
    );
    assert_eq!(
        runs[0].moves,
        vec![RunMove {
            flow_id: rent,
            amount: 9_000,
            transaction_id: move_id(march.command_id, rent),
            voided: false,
        }]
    );
    let january_moves: Vec<_> = runs[2]
        .moves
        .iter()
        .map(|m| (m.flow_id, m.transaction_id))
        .collect();
    assert_eq!(
        january_moves,
        [h.rent, h.savings, h.food]
            .into_iter()
            .map(|flow| (flow, move_id(january.command_id, flow)))
            .collect::<Vec<_>>()
    );

    let latest_two = h.fx.core.allocation_runs(h.fx.vault, 2).unwrap();
    assert_eq!(latest_two, runs[..2].to_vec());
    assert!(h.fx.core.allocation_runs(h.fx.vault, 0).unwrap().is_empty());
}

#[test]
fn preview_reads_the_envelopes_and_unallocated_as_they_are_now() {
    let mut h = household();
    h.income(100_000, secs(day(2026, 1, 1)));
    let lines = h.plan_view().lines;
    let preview =
        h.fx.core
            .preview_allocation(h.fx.vault, &lines, 150_000)
            .unwrap();
    let snapshot = h.fx.core.snapshot(h.fx.vault).unwrap();
    assert_eq!(
        preview,
        sparagne_core::allocation::resolve(150_000, &lines, &snapshot.flows, 100_000)
    );
    assert_eq!(preview.distributed, 135_000);
    assert_eq!(preview.unallocated_after, -35_000);

    // A draft that is not saved is previewed the same way.
    let draft = vec![fill(h.food), fixed(h.rent, 10)];
    let preview =
        h.fx.core
            .preview_allocation(h.fx.vault, &draft, 50)
            .unwrap();
    assert_eq!(
        preview.lines.iter().map(|l| l.amount).collect::<Vec<_>>(),
        vec![50, 0]
    );

    assert_eq!(
        h.fx.core
            .preview_allocation(Uuid::now_v7(), &lines, 100)
            .unwrap_err(),
        DomainError::NotFound("vault".to_string())
    );
}
