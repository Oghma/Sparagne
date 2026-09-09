//! Acceptance tests for recurring templates: schedules, pending periods,
//! execution and skipping.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::NaiveDate;
use common::*;
use sparagne_core::{
    Command, Core, DomainError, FlowMode, Frequency, RunOutcome, Schedule, TransactionKind, replay,
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Local helpers
// ---------------------------------------------------------------------------

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn monthly(day: u8, start: NaiveDate, end: Option<NaiveDate>) -> Schedule {
    Schedule {
        frequency: Frequency::Monthly { day },
        interval: 1,
        start_date: start,
        end_date: end,
    }
}

fn recurring_cmd(
    kind: TransactionKind,
    amount: i64,
    flow: Option<Uuid>,
    category: Option<&str>,
    schedule: Schedule,
) -> Command {
    Command::CreateRecurring {
        transaction_kind: kind,
        amount,
        wallet_id: None,
        flow_id: flow,
        category: category.map(str::to_string),
        note: None,
        schedule,
    }
}

/// An expense template of 2500 on the 1st of every month from 2026-01-01.
fn rent_cmd() -> Command {
    recurring_cmd(
        TransactionKind::Expense,
        2500,
        None,
        Some("Rent"),
        monthly(1, date(2026, 1, 1), None),
    )
}

fn due_dates(core: &Core, vault: Uuid, today: NaiveDate) -> Vec<(Uuid, Vec<NaiveDate>)> {
    core.pending_recurring(vault, today)
        .unwrap()
        .into_iter()
        .map(|p| (p.template.id, p.due))
        .collect()
}

// ---------------------------------------------------------------------------
// Create, list, update, archive
// ---------------------------------------------------------------------------

#[test]
fn create_lists_the_template_with_its_schedule() {
    let mut fx = setup();
    let id = run(&mut fx.core, fx.vault, rent_cmd()).result_id.unwrap();

    let templates = fx.core.list_recurring(fx.vault, false).unwrap();
    assert_eq!(templates.len(), 1);
    let t = &templates[0];
    assert_eq!(t.id, id);
    assert_eq!(t.kind, TransactionKind::Expense);
    assert_eq!(t.amount, 2500);
    assert_eq!(t.wallet_id, None);
    assert_eq!(t.flow_id, None);
    assert_eq!(t.category.as_deref(), Some("Rent"));
    assert_eq!(t.note, None);
    assert_eq!(t.schedule, monthly(1, date(2026, 1, 1), None));
    assert!(t.enabled);
    assert!(!t.archived);

    // The category is only resolved at execution time.
    let names: Vec<_> = fx
        .core
        .categories(fx.vault, true)
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(names, vec!["Opening", "Uncategorized"]);
    assert!(fx.core.recurring_runs(fx.vault, id).unwrap().is_empty());
}

#[test]
fn create_rejects_bad_kind_amount_schedule_and_targets() {
    let mut fx = setup();
    let good = monthly(1, date(2026, 1, 1), None);

    assert!(matches!(
        try_run(
            &mut fx.core,
            fx.vault,
            recurring_cmd(TransactionKind::TransferWallet, 100, None, None, good),
        ),
        Err(DomainError::InvalidCommand(_))
    ));
    assert!(matches!(
        try_run(
            &mut fx.core,
            fx.vault,
            recurring_cmd(TransactionKind::Refund, 100, None, None, good),
        ),
        Err(DomainError::InvalidCommand(_))
    ));
    assert!(matches!(
        try_run(
            &mut fx.core,
            fx.vault,
            recurring_cmd(TransactionKind::Income, 0, None, None, good),
        ),
        Err(DomainError::InvalidAmount(_))
    ));
    assert!(matches!(
        try_run(
            &mut fx.core,
            fx.vault,
            recurring_cmd(
                TransactionKind::Income,
                100,
                None,
                None,
                Schedule {
                    interval: 0,
                    ..good
                },
            ),
        ),
        Err(DomainError::InvalidCommand(_))
    ));
    assert_eq!(
        try_run(
            &mut fx.core,
            fx.vault,
            recurring_cmd(
                TransactionKind::Income,
                100,
                Some(Uuid::now_v7()),
                None,
                good
            ),
        )
        .unwrap_err(),
        DomainError::NotFound("flow".to_string())
    );

    let flow = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Old", FlowMode::Unlimited, false, 0),
    )
    .result_id
    .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveFlow { flow_id: flow },
    );
    assert!(matches!(
        try_run(
            &mut fx.core,
            fx.vault,
            recurring_cmd(TransactionKind::Income, 100, Some(flow), None, good),
        ),
        Err(DomainError::InvalidCommand(_))
    ));
    assert!(fx.core.list_recurring(fx.vault, true).unwrap().is_empty());
}

#[test]
fn update_changes_the_schedule_and_what_is_pending() {
    let mut fx = setup();
    let id = run(&mut fx.core, fx.vault, rent_cmd()).result_id.unwrap();
    assert_eq!(
        due_dates(&fx.core, fx.vault, date(2026, 3, 15)),
        vec![(
            id,
            vec![date(2026, 1, 1), date(2026, 2, 1), date(2026, 3, 1)]
        )]
    );

    let moved = monthly(20, date(2026, 2, 1), None);
    run(
        &mut fx.core,
        fx.vault,
        Command::UpdateRecurring {
            recurring_id: id,
            amount: Some(3000),
            wallet_id: None,
            flow_id: None,
            category: Some("  ".to_string()),
            note: Some("monthly rent".to_string()),
            schedule: Some(moved),
            enabled: None,
        },
    );

    let t = fx.core.list_recurring(fx.vault, false).unwrap().remove(0);
    assert_eq!(t.amount, 3000);
    assert_eq!(t.category, None);
    assert_eq!(t.note.as_deref(), Some("monthly rent"));
    assert_eq!(t.schedule, moved);
    assert_eq!(
        due_dates(&fx.core, fx.vault, date(2026, 3, 15)),
        vec![(id, vec![date(2026, 2, 20)])]
    );
}

#[test]
fn update_and_archive_refuse_the_impossible() {
    let mut fx = setup();
    let id = run(&mut fx.core, fx.vault, rent_cmd()).result_id.unwrap();
    let empty = Command::UpdateRecurring {
        recurring_id: id,
        amount: None,
        wallet_id: None,
        flow_id: None,
        category: None,
        note: None,
        schedule: None,
        enabled: None,
    };
    assert!(matches!(
        try_run(&mut fx.core, fx.vault, empty),
        Err(DomainError::InvalidCommand(_))
    ));
    assert_eq!(
        try_run(
            &mut fx.core,
            fx.vault,
            Command::ArchiveRecurring {
                recurring_id: Uuid::now_v7()
            },
        )
        .unwrap_err(),
        DomainError::NotFound("recurring".to_string())
    );

    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveRecurring { recurring_id: id },
    );
    assert!(matches!(
        try_run(
            &mut fx.core,
            fx.vault,
            Command::ArchiveRecurring { recurring_id: id },
        ),
        Err(DomainError::InvalidCommand(_))
    ));
    assert!(matches!(
        try_run(
            &mut fx.core,
            fx.vault,
            Command::UpdateRecurring {
                recurring_id: id,
                amount: Some(10),
                wallet_id: None,
                flow_id: None,
                category: None,
                note: None,
                schedule: None,
                enabled: None,
            },
        ),
        Err(DomainError::InvalidCommand(_))
    ));
}

// ---------------------------------------------------------------------------
// Pending
// ---------------------------------------------------------------------------

#[test]
fn pending_backfills_missed_periods_and_stops_at_today() {
    let mut fx = setup();
    let id = run(&mut fx.core, fx.vault, rent_cmd()).result_id.unwrap();

    assert!(due_dates(&fx.core, fx.vault, date(2025, 12, 31)).is_empty());
    assert_eq!(
        due_dates(&fx.core, fx.vault, date(2026, 1, 1)),
        vec![(id, vec![date(2026, 1, 1)])]
    );
    assert_eq!(
        due_dates(&fx.core, fx.vault, date(2026, 4, 30)),
        vec![(
            id,
            vec![
                date(2026, 1, 1),
                date(2026, 2, 1),
                date(2026, 3, 1),
                date(2026, 4, 1)
            ]
        )]
    );
}

#[test]
fn pending_respects_end_date_disabled_and_archived() {
    let mut fx = setup();
    let bounded = run(
        &mut fx.core,
        fx.vault,
        recurring_cmd(
            TransactionKind::Income,
            100,
            None,
            None,
            monthly(1, date(2026, 1, 1), Some(date(2026, 2, 15))),
        ),
    )
    .result_id
    .unwrap();
    let off = run(&mut fx.core, fx.vault, rent_cmd()).result_id.unwrap();
    let gone = run(&mut fx.core, fx.vault, rent_cmd()).result_id.unwrap();

    run(
        &mut fx.core,
        fx.vault,
        Command::UpdateRecurring {
            recurring_id: off,
            amount: None,
            wallet_id: None,
            flow_id: None,
            category: None,
            note: None,
            schedule: None,
            enabled: Some(false),
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveRecurring { recurring_id: gone },
    );

    assert_eq!(
        due_dates(&fx.core, fx.vault, date(2026, 6, 1)),
        vec![(bounded, vec![date(2026, 1, 1), date(2026, 2, 1)])]
    );
    assert_eq!(fx.core.list_recurring(fx.vault, false).unwrap().len(), 2);
    assert_eq!(fx.core.list_recurring(fx.vault, true).unwrap().len(), 3);

    // A disabled or archived template cannot be executed either.
    for id in [off, gone] {
        assert!(matches!(
            try_run(
                &mut fx.core,
                fx.vault,
                Command::ExecuteRecurring {
                    recurring_id: id,
                    period_date: date(2026, 1, 1),
                    occurred_at: at(T0),
                },
            ),
            Err(DomainError::InvalidCommand(_))
        ));
    }
}

// ---------------------------------------------------------------------------
// Execute and skip
// ---------------------------------------------------------------------------

#[test]
fn execute_posts_the_transaction_and_records_the_run() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10_000, None, None, Some("Salary"), T0)),
    );
    let id = run(&mut fx.core, fx.vault, rent_cmd()).result_id.unwrap();

    let tx_id = run(
        &mut fx.core,
        fx.vault,
        Command::ExecuteRecurring {
            recurring_id: id,
            period_date: date(2026, 1, 1),
            occurred_at: at(T0 + 100),
        },
    )
    .result_id
    .unwrap();

    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 7500);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 7500);

    let posted = list(&fx.core, fx.vault, &all())
        .into_iter()
        .find(|t| t.id == tx_id)
        .unwrap();
    assert_eq!(posted.kind, TransactionKind::Expense);
    assert_eq!(posted.amount, 2500);
    assert_eq!(posted.category, "Rent");
    assert!(!posted.voided);
    let legs: Vec<i64> = posted.legs.iter().map(|l| l.amount).collect();
    assert_eq!(legs, vec![-2500, -2500]);

    let runs = fx.core.recurring_runs(fx.vault, id).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].period_date, date(2026, 1, 1));
    assert_eq!(runs[0].outcome, RunOutcome::Executed);
    assert_eq!(runs[0].transaction_id, Some(tx_id));

    // The executed period is no longer pending; the next one still is.
    assert_eq!(
        due_dates(&fx.core, fx.vault, date(2026, 2, 5)),
        vec![(id, vec![date(2026, 2, 1)])]
    );
}

#[test]
fn a_period_can_be_handled_only_once_and_only_when_due() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10_000, None, None, None, T0)),
    );
    let id = run(&mut fx.core, fx.vault, rent_cmd()).result_id.unwrap();
    let execute = |period: NaiveDate| Command::ExecuteRecurring {
        recurring_id: id,
        period_date: period,
        occurred_at: at(T0 + 100),
    };

    // Not an occurrence of the schedule.
    assert!(matches!(
        try_run(&mut fx.core, fx.vault, execute(date(2026, 1, 2))),
        Err(DomainError::InvalidCommand(_))
    ));
    // Before start_date, even though it is a 1st of the month.
    assert!(matches!(
        try_run(&mut fx.core, fx.vault, execute(date(2025, 12, 1))),
        Err(DomainError::InvalidCommand(_))
    ));

    run(&mut fx.core, fx.vault, execute(date(2026, 1, 1)));
    assert_eq!(
        try_run(&mut fx.core, fx.vault, execute(date(2026, 1, 1))).unwrap_err(),
        DomainError::AlreadyExists("run for 2026-01-01".to_string())
    );
    assert!(matches!(
        try_run(
            &mut fx.core,
            fx.vault,
            Command::SkipRecurring {
                recurring_id: id,
                period_date: date(2026, 1, 1),
            },
        ),
        Err(DomainError::AlreadyExists(_))
    ));
    assert_eq!(fx.core.recurring_runs(fx.vault, id).unwrap().len(), 1);
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 7500);
}

#[test]
fn skip_handles_the_period_without_a_transaction() {
    let mut fx = setup();
    let id = run(&mut fx.core, fx.vault, rent_cmd()).result_id.unwrap();

    let receipt = run(
        &mut fx.core,
        fx.vault,
        Command::SkipRecurring {
            recurring_id: id,
            period_date: date(2026, 1, 1),
        },
    );
    assert_eq!(receipt.result_id, None);
    assert!(list(&fx.core, fx.vault, &all()).is_empty());
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 0);

    let runs = fx.core.recurring_runs(fx.vault, id).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].outcome, RunOutcome::Skipped);
    assert_eq!(runs[0].transaction_id, None);
    assert_eq!(
        due_dates(&fx.core, fx.vault, date(2026, 2, 5)),
        vec![(id, vec![date(2026, 2, 1)])]
    );
}

#[test]
fn a_refused_execution_leaves_no_run_and_no_transaction() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10_000, None, None, None, T0)),
    );
    let capped = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Fun", FlowMode::NetCapped { cap: 1000 }, false, 0),
    )
    .result_id
    .unwrap();
    let id = run(
        &mut fx.core,
        fx.vault,
        recurring_cmd(
            TransactionKind::Income,
            5000,
            Some(capped),
            Some("Bonus"),
            monthly(1, date(2026, 1, 1), None),
        ),
    )
    .result_id
    .unwrap();

    let before = balances(&fx.core, fx.vault);
    assert_eq!(
        try_run(
            &mut fx.core,
            fx.vault,
            Command::ExecuteRecurring {
                recurring_id: id,
                period_date: date(2026, 1, 1),
                occurred_at: at(T0 + 100),
            },
        )
        .unwrap_err(),
        DomainError::MaxBalanceReached("Fun".to_string())
    );

    assert_eq!(balances(&fx.core, fx.vault), before);
    assert!(fx.core.recurring_runs(fx.vault, id).unwrap().is_empty());
    assert_eq!(list(&fx.core, fx.vault, &all()).len(), 1);
    assert_eq!(
        due_dates(&fx.core, fx.vault, date(2026, 1, 31)),
        vec![(id, vec![date(2026, 1, 1)])]
    );
}

#[test]
fn wallet_none_resolves_to_the_only_active_wallet() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10_000, None, None, None, T0)),
    );
    let id = run(&mut fx.core, fx.vault, rent_cmd()).result_id.unwrap();
    let tx_id = run(
        &mut fx.core,
        fx.vault,
        Command::ExecuteRecurring {
            recurring_id: id,
            period_date: date(2026, 1, 1),
            occurred_at: at(T0 + 100),
        },
    )
    .result_id
    .unwrap();
    assert_eq!(
        list(&fx.core, fx.vault, &all())
            .into_iter()
            .find(|t| t.id == tx_id)
            .unwrap()
            .legs
            .len(),
        2
    );
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 7500);

    // With a second active wallet the template no longer knows where to post.
    run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0));
    assert!(matches!(
        try_run(
            &mut fx.core,
            fx.vault,
            Command::ExecuteRecurring {
                recurring_id: id,
                period_date: date(2026, 2, 1),
                occurred_at: at(T0 + 200),
            },
        ),
        Err(DomainError::InvalidCommand(_))
    ));
    assert_eq!(fx.core.recurring_runs(fx.vault, id).unwrap().len(), 1);
}

#[test]
fn runs_of_an_unknown_template_are_not_found() {
    let fx = setup();
    assert_eq!(
        fx.core
            .recurring_runs(fx.vault, Uuid::now_v7())
            .unwrap_err(),
        DomainError::NotFound("recurring".to_string())
    );
}

// ---------------------------------------------------------------------------
// Replay
// ---------------------------------------------------------------------------

#[test]
fn replay_rebuilds_templates_runs_and_transactions() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(50_000, None, None, Some("Salary"), T0)),
    );
    let id = run(&mut fx.core, fx.vault, rent_cmd()).result_id.unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::ExecuteRecurring {
            recurring_id: id,
            period_date: date(2026, 1, 1),
            occurred_at: at(T0 + 100),
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::SkipRecurring {
            recurring_id: id,
            period_date: date(2026, 2, 1),
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::UpdateRecurring {
            recurring_id: id,
            amount: Some(2600),
            wallet_id: Some(fx.wallet),
            flow_id: None,
            category: None,
            note: Some("rent".to_string()),
            schedule: Some(monthly(3, date(2026, 1, 1), Some(date(2026, 12, 31)))),
            enabled: None,
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::ExecuteRecurring {
            recurring_id: id,
            period_date: date(2026, 3, 3),
            occurred_at: at(T0 + 300),
        },
    );
    let archived = run(&mut fx.core, fx.vault, rent_cmd()).result_id.unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveRecurring {
            recurring_id: archived,
        },
    );

    let log = fx.core.commands_since(fx.vault, 0).unwrap();
    let mut fresh = Core::open_in_memory().unwrap();
    replay(&log, &mut fresh).unwrap();

    assert_eq!(
        fresh.snapshot(fx.vault).unwrap(),
        fx.core.snapshot(fx.vault).unwrap()
    );
    assert_eq!(
        fresh.list_recurring(fx.vault, true).unwrap(),
        fx.core.list_recurring(fx.vault, true).unwrap()
    );
    assert_eq!(
        fresh.recurring_runs(fx.vault, id).unwrap(),
        fx.core.recurring_runs(fx.vault, id).unwrap()
    );
    assert_eq!(
        list(&fresh, fx.vault, &all()),
        list(&fx.core, fx.vault, &all())
    );
    assert_eq!(
        fresh.pending_recurring(fx.vault, date(2026, 6, 1)).unwrap(),
        fx.core
            .pending_recurring(fx.vault, date(2026, 6, 1))
            .unwrap()
    );
    assert_eq!(fx.core.recurring_runs(fx.vault, id).unwrap().len(), 3);
}
