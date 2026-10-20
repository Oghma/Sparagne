//! Acceptance tests for the allocation plan: the plan itself, executing,
//! skipping and reopening its periods, the base each execution shares out,
//! what is pending and the history of decided periods.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone};
use common::{
    sync::{base_of, income_ids, other_vault, pending_on, runs_of, secs},
    *,
};
use sparagne_core::{
    AllocationBase, AllocationPlanPatch, AllocationPlanView, AllocationRunView, Command,
    CommandEnvelope, DomainError, FlowMode, LineStatus, PendingAllocation, Receipt, RunMove,
    RunOutcome, Schedule, TransactionKind, TransactionPatch, TransactionView,
};
use uuid::Uuid;

/// `y-m-d h:00` at `offset_hours` east of UTC.
fn local(offset_hours: i32, y: i32, m: u32, d: u32, h: u32) -> DateTime<FixedOffset> {
    FixedOffset::east_opt(offset_hours * 3600)
        .unwrap()
        .with_ymd_and_hms(y, m, d, h, 0, 0)
        .unwrap()
}

/// The id of the transfer an execution makes to `flow`.
fn move_id(command_id: Uuid, flow: Uuid) -> Uuid {
    Uuid::new_v5(&command_id, format!("allocation:{flow}").as_bytes())
}

/// Everything an allocation command may change.
#[derive(Debug, PartialEq)]
struct State {
    balances: (Balances, Balances),
    transactions: Vec<TransactionView>,
    plan: Option<AllocationPlanView>,
    runs: Vec<AllocationRunView>,
    base: AllocationBase,
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

    fn state(&self) -> State {
        State {
            balances: balances(&self.fx.core, self.fx.vault),
            transactions: list(&self.fx.core, self.fx.vault, &all()),
            plan: self.fx.core.allocation_plan(self.fx.vault).unwrap(),
            runs: runs_of(&self.fx),
            base: base_of(&self.fx),
        }
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
fn create_stores_the_plan_under_the_command_id() {
    let mut fx = setup();
    let rent = envelope(&mut fx, "Rent", FlowMode::Unlimited);
    let food = envelope(&mut fx, "Food", FlowMode::NetCapped { cap: 40_000 });
    assert_eq!(fx.core.allocation_plan(fx.vault).unwrap(), None);

    let schedule = monthly_from(27, day(2026, 1, 1));
    let lines = vec![fill(food), fixed(rent, 1)];
    let receipt = fx.run(plan_cmd(schedule, lines.clone()));
    assert_eq!(receipt.result_id, Some(receipt.command_id));

    let plan = fx.core.allocation_plan(fx.vault).unwrap().unwrap();
    assert_eq!(
        plan,
        AllocationPlanView {
            id: receipt.command_id,
            schedule,
            lines: lines.clone(),
            enabled: true,
            created_by: "alice".to_string(),
        }
    );
    assert!(fx.core.allocation_runs(fx.vault, 10).unwrap().is_empty());

    // One plan per vault.
    assert_eq!(
        fx.try_run(plan_cmd(schedule, lines)).unwrap_err(),
        DomainError::AlreadyExists("allocation plan".to_string())
    );
}

#[test]
fn create_refuses_bad_lines_and_schedules() {
    let mut fx = setup();
    let rent = envelope(&mut fx, "Rent", FlowMode::Unlimited);
    let (_, foreign) = other_vault(&mut fx.core);
    let good = monthly_from(1, day(2026, 1, 1));
    let unallocated = fx.unallocated;
    let mut refuse = |schedule: Schedule, lines| fx.try_run(plan_cmd(schedule, lines)).unwrap_err();

    assert!(matches!(
        refuse(good, vec![]),
        DomainError::InvalidCommand(_)
    ));
    assert_eq!(
        refuse(good, vec![fixed(Uuid::now_v7(), 100)]),
        DomainError::NotFound("flow".to_string())
    );
    assert_eq!(
        refuse(good, vec![fixed(foreign, 100)]),
        DomainError::NotFound("flow".to_string())
    );
    assert!(matches!(
        refuse(good, vec![fixed(rent, 100), percent(rent, 500)]),
        DomainError::InvalidCommand(_)
    ));
    assert!(matches!(
        refuse(good, vec![fixed(unallocated, 100)]),
        DomainError::InvalidFlow(_)
    ));
    for bad in [
        fixed(rent, 0),
        fixed(rent, -5),
        percent(rent, 0),
        percent(rent, 10_001),
    ] {
        assert!(matches!(
            refuse(good, vec![bad]),
            DomainError::InvalidAmount(_)
        ));
    }
    for schedule in [
        Schedule {
            interval: 0,
            ..good
        },
        monthly_from(32, day(2026, 1, 1)),
        Schedule {
            end_date: Some(day(2025, 12, 31)),
            ..good
        },
    ] {
        assert!(matches!(
            refuse(schedule, vec![fixed(rent, 100)]),
            DomainError::InvalidCommand(_)
        ));
    }
    assert_eq!(fx.core.allocation_plan(fx.vault).unwrap(), None);

    // The bounds themselves are fine.
    fx.run(plan_cmd(good, vec![percent(rent, 10_000)]));
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
fn update_refuses_an_empty_patch_bad_lines_and_a_plan_of_another_vault() {
    let mut h = household();
    let before = h.plan_view();
    let plan = h.plan;

    assert_eq!(
        h.fx.try_run(update_plan_cmd(plan, AllocationPlanPatch::default()))
            .unwrap_err(),
        DomainError::InvalidCommand("nothing to update".to_string())
    );
    let lines = |lines| AllocationPlanPatch {
        lines: Some(lines),
        ..AllocationPlanPatch::default()
    };
    assert!(matches!(
        h.fx.try_run(update_plan_cmd(plan, lines(vec![]))),
        Err(DomainError::InvalidCommand(_))
    ));
    assert!(matches!(
        h.fx.try_run(update_plan_cmd(
            plan,
            lines(vec![fill(h.food), fixed(h.food, 10)])
        )),
        Err(DomainError::InvalidCommand(_))
    ));
    assert!(matches!(
        h.fx.try_run(update_plan_cmd(plan, lines(vec![percent(h.rent, 20_000)]))),
        Err(DomainError::InvalidAmount(_))
    ));
    assert!(matches!(
        h.fx.try_run(update_plan_cmd(
            plan,
            AllocationPlanPatch {
                schedule: Some(monthly_from(0, day(2026, 1, 1))),
                ..AllocationPlanPatch::default()
            }
        )),
        Err(DomainError::InvalidCommand(_))
    ));
    assert_eq!(
        h.fx.try_run(update_plan_cmd(Uuid::now_v7(), lines(vec![fill(h.food)])))
            .unwrap_err(),
        DomainError::NotFound("allocation plan".to_string())
    );

    // The plan of another vault is out of reach, whoever names it.
    let (other, flow) = other_vault(&mut h.fx.core);
    let foreign = run(
        &mut h.fx.core,
        other,
        plan_cmd(monthly_from(1, day(2026, 1, 1)), vec![fixed(flow, 100)]),
    )
    .result_id
    .unwrap();
    assert_eq!(
        h.fx.try_run(update_plan_cmd(foreign, lines(vec![fill(h.food)])))
            .unwrap_err(),
        DomainError::NotFound("allocation plan".to_string())
    );
    assert_eq!(h.plan_view(), before);
}

#[test]
fn execute_moves_the_amounts_out_of_unallocated_and_records_the_run() {
    let mut h = household();
    h.income(150_000, secs(day(2026, 1, 1)));
    h.income(50_000, secs(day(2026, 1, 1)) + 60);
    let (total, moves) = h.moves();
    assert_eq!(total, 200_000);
    assert_eq!(
        moves,
        vec![(h.rent, 80_000), (h.savings, 20_000), (h.food, 40_000)]
    );

    let receipt =
        h.fx.run(execute_cmd(h.plan, day(2026, 1, 1), total, &moves));
    assert_eq!(receipt.result_id, None);

    assert_eq!(h.flow(h.fx.unallocated), 60_000);
    assert_eq!(h.flow(h.rent), 80_000);
    assert_eq!(h.flow(h.savings), 20_000);
    assert_eq!(h.flow(h.food), 40_000);
    assert_eq!(wallet_balance(&h.fx.core, h.fx.vault, h.fx.wallet), 200_000);

    for &(flow, amount) in &moves {
        let t =
            h.fx.core
                .transaction(h.fx.vault, move_id(receipt.command_id, flow))
                .unwrap();
        assert_eq!(t.kind, TransactionKind::TransferFlow);
        assert_eq!(t.amount, amount);
        assert_eq!(t.from_id, Some(h.fx.unallocated));
        assert_eq!(t.to_id, Some(flow));
        assert_eq!(t.category, "Uncategorized");
        assert!(t.category_is_system);
        assert_eq!(t.note, None);
        assert_eq!(t.occurred_at, noon(day(2026, 1, 1)));
        assert_eq!(t.person, "alice");
        assert!(!t.voided);
    }

    assert_eq!(
        runs_of(&h.fx),
        vec![AllocationRunView {
            period_date: day(2026, 1, 1),
            outcome: RunOutcome::Executed,
            total: 200_000,
            moves: moves
                .iter()
                .map(|&(flow_id, amount)| RunMove {
                    flow_id,
                    amount,
                    transaction_id: move_id(receipt.command_id, flow_id),
                    voided: false,
                })
                .collect(),
            created_by: "alice".to_string(),
        }]
    );
    assert_eq!(base_of(&h.fx).total, 0);
    assert_eq!(pending_on(&h.fx, day(2026, 1, 31)), None);
    assert_eq!(
        pending_on(&h.fx, day(2026, 2, 1)),
        Some(PendingAllocation {
            plan_id: h.plan,
            period_date: day(2026, 2, 1),
            missed: 0,
        })
    );
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
fn a_period_is_decided_once_and_in_order() {
    let mut h = household();
    h.income(100_000, secs(day(2026, 1, 1)));
    let plan = h.plan;
    let rent = h.rent;
    let execute = |period| execute_cmd(plan, period, 1_000, &[(rent, 1_000)]);

    // Not a date of the schedule, or before its start.
    for period in [day(2026, 2, 2), day(2025, 12, 1)] {
        assert_eq!(
            h.fx.try_run(execute(period)).unwrap_err(),
            DomainError::InvalidCommand("period_date is not a due date".to_string())
        );
        assert!(matches!(
            h.fx.try_run(skip_cmd(plan, period)),
            Err(DomainError::InvalidCommand(_))
        ));
    }

    h.fx.run(execute(day(2026, 2, 1)));
    for cmd in [execute(day(2026, 2, 1)), skip_cmd(plan, day(2026, 2, 1))] {
        assert_eq!(
            h.fx.try_run(cmd).unwrap_err(),
            DomainError::AlreadyExists("run for 2026-02-01".to_string())
        );
    }
    // January was never decided, but February already was.
    for cmd in [execute(day(2026, 1, 1)), skip_cmd(plan, day(2026, 1, 1))] {
        assert_eq!(
            h.fx.try_run(cmd).unwrap_err(),
            DomainError::InvalidCommand("a later period is already decided".to_string())
        );
    }
    h.fx.run(skip_cmd(plan, day(2026, 3, 1)));
    h.fx.run(execute(day(2026, 5, 1)));

    let decided: Vec<_> = runs_of(&h.fx)
        .iter()
        .map(|r| (r.period_date, r.outcome))
        .collect();
    assert_eq!(
        decided,
        vec![
            (day(2026, 5, 1), RunOutcome::Executed),
            (day(2026, 3, 1), RunOutcome::Skipped),
            (day(2026, 2, 1), RunOutcome::Executed),
        ]
    );
    assert_eq!(h.flow(h.rent), 2_000);
}

#[test]
fn execute_checks_the_moves_and_the_total_before_moving_anything() {
    let mut h = household();
    h.income(100_000, secs(day(2026, 1, 1)));
    let old = envelope(&mut h.fx, "Old", FlowMode::Unlimited);
    h.fx.run(Command::ArchiveFlow { flow_id: old });
    let (_, foreign) = other_vault(&mut h.fx.core);
    let before = h.state();
    let (plan, rent, food, unallocated) = (h.plan, h.rent, h.food, h.fx.unallocated);
    let period = day(2026, 1, 1);
    let execute = |total, moves: &[(Uuid, i64)]| execute_cmd(plan, period, total, moves);

    let refusals = [
        (
            execute(100, &[]),
            DomainError::InvalidCommand("nothing to allocate".to_string()),
        ),
        (
            execute(300, &[(rent, 100), (food, 100), (rent, 100)]),
            DomainError::InvalidCommand("an envelope appears twice in the moves".to_string()),
        ),
        (
            execute(100, &[(unallocated, 100)]),
            DomainError::InvalidFlow("cannot allocate to Unallocated".to_string()),
        ),
        (
            execute(100, &[(rent, 0)]),
            DomainError::InvalidAmount("amount must be > 0".to_string()),
        ),
        (
            execute(100, &[(rent, -100)]),
            DomainError::InvalidAmount("amount must be > 0".to_string()),
        ),
        (
            execute(-1, &[(rent, 100)]),
            DomainError::InvalidAmount("total must be >= 0".to_string()),
        ),
        (
            execute(150, &[(rent, 100), (food, 51)]),
            DomainError::InvalidAmount("moves add up to more than the total".to_string()),
        ),
        (
            execute(i64::MAX, &[(rent, i64::MAX), (food, 1)]),
            DomainError::InvalidAmount("moves add up to more than the total".to_string()),
        ),
        (
            execute(200, &[(rent, 100), (Uuid::now_v7(), 100)]),
            DomainError::NotFound("flow".to_string()),
        ),
        (
            execute(200, &[(rent, 100), (foreign, 100)]),
            DomainError::NotFound("flow".to_string()),
        ),
        (
            execute(200, &[(rent, 100), (old, 100)]),
            DomainError::InvalidCommand("flow is archived".to_string()),
        ),
        (
            execute_cmd(Uuid::now_v7(), period, 100, &[(rent, 100)]),
            DomainError::NotFound("allocation plan".to_string()),
        ),
    ];
    for (cmd, expected) in refusals {
        assert_eq!(h.fx.try_run(cmd).unwrap_err(), expected);
        assert_eq!(h.state(), before);
    }

    // The moves may add up to less than the total.
    h.fx.run(execute(500, &[(rent, 100)]));
    assert_eq!(h.flow(h.rent), 100);
}

#[test]
fn a_move_over_the_cap_refuses_the_whole_execution() {
    let mut h = household();
    h.income(200_000, secs(day(2026, 1, 1)));
    let before = h.state();
    let pending = pending_on(&h.fx, day(2026, 1, 1));

    let cmd = execute_cmd(
        h.plan,
        day(2026, 1, 1),
        200_000,
        &[(h.rent, 80_000), (h.savings, 20_000), (h.food, 40_001)],
    );
    assert_eq!(
        h.fx.try_run(cmd).unwrap_err(),
        DomainError::MaxBalanceReached("Food".to_string())
    );
    assert_eq!(h.state(), before);
    assert_eq!(pending_on(&h.fx, day(2026, 1, 1)), pending);

    // What the preview says always fits.
    h.confirm(day(2026, 1, 1));
    assert_eq!(h.flow(h.food), 40_000);
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
fn reopen_voids_the_transfers_and_makes_the_period_due_again() {
    let mut h = household();
    let salary = h.income(200_000, secs(day(2026, 1, 1)));
    let before = balances(&h.fx.core, h.fx.vault);
    let base = base_of(&h.fx);
    let receipt = h.confirm(day(2026, 1, 1));

    let reopen = h.fx.run(reopen_cmd(h.plan, day(2026, 1, 1)));
    assert_eq!(reopen.result_id, None);
    assert_eq!(balances(&h.fx.core, h.fx.vault), before);
    for flow in [h.rent, h.savings, h.food] {
        let t =
            h.fx.core
                .transaction(h.fx.vault, move_id(receipt.command_id, flow))
                .unwrap();
        assert!(t.voided);
    }
    assert!(runs_of(&h.fx).is_empty());
    assert_eq!(base_of(&h.fx), base);
    assert_eq!(income_ids(&base_of(&h.fx)), vec![salary]);
    assert_eq!(
        pending_on(&h.fx, day(2026, 1, 15)).map(|p| p.period_date),
        Some(day(2026, 1, 1))
    );

    // Executed again, with new transfers.
    let again = h.confirm(day(2026, 1, 1));
    assert_eq!(h.flow(h.rent), 80_000);
    assert_eq!(
        runs_of(&h.fx)[0].moves[0].transaction_id,
        move_id(again.command_id, h.rent)
    );
    assert_eq!(list(&h.fx.core, h.fx.vault, &all()).len(), 7);
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
fn the_base_is_the_new_incomes_that_reached_unallocated() {
    let mut h = household();
    let wallet = h.fx.wallet;
    let income_at = |when: DateTime<FixedOffset>| {
        Command::Income(sparagne_core::Entry {
            occurred_at: when,
            ..entry(1_000, Some(wallet), None, None, 0)
        })
    };

    // Dated by their own offset: the first is still December where it was
    // recorded, the second already January.
    h.income(5_000, secs(day(2025, 12, 31)));
    h.fx.run(income_at(local(-1, 2025, 12, 31, 23)));
    let new_year = h.fx.id(income_at(local(2, 2026, 1, 1, 0)));

    // Recorded out of order: the base lists them by date.
    let late = h.income(3_000, secs(day(2026, 1, 20)));
    let early = h.income(2_000, secs(day(2026, 1, 2)));

    let voided = h.income(9_000, secs(day(2026, 1, 3)));
    h.fx.run(Command::VoidTransaction {
        transaction_id: voided,
    });
    h.fx.run(Command::Refund(entry(
        700,
        Some(wallet),
        None,
        None,
        secs(day(2026, 1, 4)),
    )));
    let rent = h.rent;
    h.fx.run(Command::Income(entry(
        4_000,
        Some(wallet),
        Some(rent),
        None,
        secs(day(2026, 1, 5)),
    )));
    let spare =
        h.fx.id(flow_cmd("Spare", FlowMode::Unlimited, false, 5_000));
    h.fx.run(Command::TransferFlow {
        amount: 2_000,
        from_flow_id: spare,
        to_flow_id: h.fx.unallocated,
        note: None,
        occurred_at: noon(day(2026, 1, 6)),
    });

    // A recurring income, executed.
    let template = h.fx.id(Command::CreateRecurring {
        transaction_kind: TransactionKind::Income,
        amount: 6_000,
        wallet_id: Some(wallet),
        flow_id: None,
        category: Some("Salary".to_string()),
        note: None,
        schedule: monthly_from(7, day(2026, 1, 1)),
        owner: None,
    });
    let recurring = h.fx.id(Command::ExecuteRecurring {
        recurring_id: template,
        period_date: day(2026, 1, 7),
        occurred_at: noon(day(2026, 1, 7)),
        person: None,
    });

    // Two incomes imported in one batch, as a statement import may do.
    let imported: Vec<Uuid> =
        h.fx.core
            .execute_batch(
                [secs(day(2026, 1, 8)), secs(day(2026, 1, 9))]
                    .into_iter()
                    .map(|secs| {
                        CommandEnvelope::new(
                            h.fx.vault,
                            "alice",
                            Command::Income(entry(1_500, Some(wallet), None, None, secs)),
                        )
                    })
                    .collect(),
            )
            .unwrap()
            .into_iter()
            .map(|r| r.result_id.unwrap())
            .collect();

    // An edited amount counts as it is now; an income moved to an envelope
    // no longer counts.
    let edited = h.income(1_000, secs(day(2026, 1, 10)));
    h.fx.run(Command::UpdateTransaction {
        transaction_id: edited,
        patch: TransactionPatch {
            amount: Some(1_250),
            ..TransactionPatch::default()
        },
    });
    let moved = h.income(800, secs(day(2026, 1, 11)));
    h.fx.run(Command::UpdateTransaction {
        transaction_id: moved,
        patch: TransactionPatch {
            flow_id: Some(rent),
            ..TransactionPatch::default()
        },
    });

    // An opening balance never counts, whatever its date.
    h.fx.run(Command::CreateWallet {
        name: "Bank".to_string(),
        opening_balance: 50_000,
        occurred_at: noon(day(2026, 1, 12)),
    });

    let base = base_of(&h.fx);
    assert_eq!(
        income_ids(&base),
        vec![
            new_year,
            early,
            recurring,
            imported[0],
            imported[1],
            edited,
            late
        ]
    );
    assert_eq!(
        base.total,
        1_000 + 2_000 + 6_000 + 1_500 + 1_500 + 1_250 + 3_000
    );
    assert_eq!(
        base.total,
        base.incomes.iter().map(|t| t.amount).sum::<i64>()
    );
    assert!(
        base.incomes
            .iter()
            .all(|t| t.kind == TransactionKind::Income)
    );
    assert_eq!(base.incomes[2].category, "Salary");
}

#[test]
fn the_base_starts_again_after_every_decision() {
    let mut h = household();
    let january = h.income(100_000, secs(day(2026, 1, 1)));
    h.confirm(day(2026, 1, 1));
    assert!(base_of(&h.fx).incomes.is_empty());

    // Recorded after the decision, so it counts even though it is dated
    // before it.
    let morning = h.income(7_000, secs(day(2026, 1, 1)) - 3 * 3600);
    let bonus = h.income(50_000, secs(day(2026, 1, 20)));
    assert_eq!(income_ids(&base_of(&h.fx)), vec![morning, bonus]);

    // Skipped: those incomes stay where they are.
    h.fx.run(skip_cmd(h.plan, day(2026, 2, 1)));
    assert!(base_of(&h.fx).incomes.is_empty());
    let february = h.income(20_000, secs(day(2026, 2, 10)));
    assert_eq!(income_ids(&base_of(&h.fx)), vec![february]);

    // Reopened: back to the decision before it.
    h.fx.run(reopen_cmd(h.plan, day(2026, 2, 1)));
    assert_eq!(income_ids(&base_of(&h.fx)), vec![morning, bonus, february]);
    assert_eq!(base_of(&h.fx).total, 77_000);

    h.confirm(day(2026, 2, 1));
    assert!(base_of(&h.fx).incomes.is_empty());
    h.fx.run(reopen_cmd(h.plan, day(2026, 2, 1)));
    assert_eq!(income_ids(&base_of(&h.fx)), vec![morning, bonus, february]);

    // With no decision left only the start date counts.
    h.fx.run(reopen_cmd(h.plan, day(2026, 1, 1)));
    assert_eq!(
        income_ids(&base_of(&h.fx)),
        vec![morning, january, bonus, february]
    );
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
fn pending_is_the_latest_due_period_with_the_missed_ones_counted() {
    let mut h = household();
    let pending =
        |h: &Household, today| pending_on(&h.fx, today).map(|p| (p.period_date, p.missed));

    assert_eq!(pending(&h, day(2025, 12, 31)), None);
    assert_eq!(pending(&h, day(2026, 1, 1)), Some((day(2026, 1, 1), 0)));
    assert_eq!(pending(&h, day(2026, 4, 15)), Some((day(2026, 4, 1), 3)));
    assert_eq!(pending_on(&h.fx, day(2026, 4, 15)).unwrap().plan_id, h.plan);

    // Deciding the latest closes the ones missed before it.
    h.fx.run(skip_cmd(h.plan, day(2026, 4, 1)));
    assert_eq!(pending(&h, day(2026, 4, 15)), None);
    assert_eq!(pending(&h, day(2026, 5, 1)), Some((day(2026, 5, 1), 0)));
    assert_eq!(pending(&h, day(2026, 7, 2)), Some((day(2026, 7, 1), 2)));

    // The end date bounds what is due.
    h.fx.run(update_plan_cmd(
        h.plan,
        AllocationPlanPatch {
            schedule: Some(Schedule {
                end_date: Some(day(2026, 6, 15)),
                ..monthly_from(1, day(2026, 1, 1))
            }),
            ..AllocationPlanPatch::default()
        },
    ));
    assert_eq!(pending(&h, day(2027, 1, 1)), Some((day(2026, 6, 1), 1)));
    h.fx.run(skip_cmd(h.plan, day(2026, 6, 1)));
    assert_eq!(pending(&h, day(2027, 1, 1)), None);
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
