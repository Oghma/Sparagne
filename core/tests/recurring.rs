//! Acceptance tests for recurring templates: schedules, pending periods,
//! execution and skipping.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::NaiveDate;
use common::*;
use sparagne_core::{
    Command, DomainError, FlowMode, Frequency, RecurringPatch, RunOutcome, Schedule,
    TransactionKind,
};
use uuid::Uuid;

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
        owner: None,
    }
}

fn execute(recurring_id: Uuid, period_date: NaiveDate, secs: i64) -> Command {
    Command::ExecuteRecurring {
        recurring_id,
        period_date,
        occurred_at: at(secs),
        person: None,
    }
}

fn skip(recurring_id: Uuid, period_date: NaiveDate) -> Command {
    Command::SkipRecurring {
        recurring_id,
        period_date,
    }
}

fn due_dates(fx: &Fx, today: NaiveDate) -> Vec<(Uuid, Vec<NaiveDate>)> {
    let pending = fx.core.pending_recurring(fx.vault, today).unwrap();
    pending
        .into_iter()
        .map(|p| (p.template.id, p.due))
        .collect()
}

#[test]
fn create_lists_the_template_with_its_schedule() {
    let mut fx = setup();
    let id = fx.id(rent_cmd());

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
    assert_eq!(t.schedule, monthly(1, day(2026, 1, 1), None));
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
    use TransactionKind::{Income, Refund, TransferWallet};
    let mut fx = setup();
    let old = fx.id(flow_cmd("Old", FlowMode::Unlimited, false, 0));
    fx.run(Command::ArchiveFlow { flow_id: old });
    let good = monthly(1, day(2026, 1, 1), None);
    let every_zero_months = Schedule {
        interval: 0,
        ..good
    };
    let mut create = |kind, amount, flow, schedule| {
        fx.try_run(recurring_cmd(kind, amount, flow, None, schedule))
    };

    for refused in [
        create(TransferWallet, 100, None, good),
        create(Refund, 100, None, good),
        create(Income, 100, None, every_zero_months),
        create(Income, 100, Some(old), good),
    ] {
        assert!(matches!(refused, Err(DomainError::InvalidCommand(_))));
    }
    assert!(matches!(
        create(Income, 0, None, good),
        Err(DomainError::InvalidAmount(_))
    ));
    assert_eq!(
        create(Income, 100, Some(Uuid::now_v7()), good).unwrap_err(),
        DomainError::NotFound("flow".to_string())
    );
    assert!(fx.core.list_recurring(fx.vault, true).unwrap().is_empty());
}

#[test]
fn update_changes_the_schedule_and_what_is_pending() {
    let mut fx = setup();
    let id = fx.id(rent_cmd());
    assert_eq!(
        due_dates(&fx, day(2026, 3, 15)),
        vec![(id, vec![day(2026, 1, 1), day(2026, 2, 1), day(2026, 3, 1)])]
    );

    let moved = monthly(20, day(2026, 2, 1), None);
    fx.run(Command::UpdateRecurring {
        recurring_id: id,
        patch: RecurringPatch {
            amount: Some(3000),
            category: Some("  ".to_string()),
            note: Some("monthly rent".to_string()),
            schedule: Some(moved),
            ..RecurringPatch::default()
        },
    });

    let t = fx.core.list_recurring(fx.vault, false).unwrap().remove(0);
    assert_eq!(t.amount, 3000);
    assert_eq!(t.category, None);
    assert_eq!(t.note.as_deref(), Some("monthly rent"));
    assert_eq!(t.schedule, moved);
    assert_eq!(
        due_dates(&fx, day(2026, 3, 15)),
        vec![(id, vec![day(2026, 2, 20)])]
    );
}

#[test]
fn update_and_archive_refuse_the_impossible() {
    let mut fx = setup();
    let id = fx.id(rent_cmd());
    let empty = Command::UpdateRecurring {
        recurring_id: id,
        patch: RecurringPatch::default(),
    };
    assert!(matches!(
        fx.try_run(empty),
        Err(DomainError::InvalidCommand(_))
    ));
    assert_eq!(
        fx.try_run(Command::ArchiveRecurring {
            recurring_id: Uuid::now_v7()
        })
        .unwrap_err(),
        DomainError::NotFound("recurring".to_string())
    );

    fx.run(Command::ArchiveRecurring { recurring_id: id });
    assert!(matches!(
        fx.try_run(Command::ArchiveRecurring { recurring_id: id }),
        Err(DomainError::InvalidCommand(_))
    ));
    assert!(matches!(
        fx.try_run(Command::UpdateRecurring {
            recurring_id: id,
            patch: RecurringPatch {
                amount: Some(10),
                ..RecurringPatch::default()
            },
        }),
        Err(DomainError::InvalidCommand(_))
    ));
}

#[test]
fn restore_reactivates_an_archived_template() {
    let mut fx = setup();
    let id = fx.id(rent_cmd());
    assert!(matches!(
        fx.try_run(Command::RestoreRecurring { recurring_id: id }),
        Err(DomainError::InvalidCommand(_))
    ));
    fx.run(Command::ArchiveRecurring { recurring_id: id });
    assert!(fx.core.list_recurring(fx.vault, false).unwrap().is_empty());
    fx.run(Command::RestoreRecurring { recurring_id: id });
    let listed = fx.core.list_recurring(fx.vault, false).unwrap();
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].archived);
    assert_eq!(
        fx.try_run(Command::RestoreRecurring {
            recurring_id: Uuid::now_v7()
        })
        .unwrap_err(),
        DomainError::NotFound("recurring".to_string())
    );
}

#[test]
fn pending_backfills_missed_periods_and_stops_at_today() {
    let mut fx = setup();
    let id = fx.id(rent_cmd());

    assert!(due_dates(&fx, day(2025, 12, 31)).is_empty());
    assert_eq!(
        due_dates(&fx, day(2026, 1, 1)),
        vec![(id, vec![day(2026, 1, 1)])]
    );
    assert_eq!(
        due_dates(&fx, day(2026, 4, 30)),
        vec![(
            id,
            vec![
                day(2026, 1, 1),
                day(2026, 2, 1),
                day(2026, 3, 1),
                day(2026, 4, 1)
            ]
        )]
    );
}

#[test]
fn pending_respects_end_date_disabled_and_archived() {
    let mut fx = setup();
    let bounded = fx.id(recurring_cmd(
        TransactionKind::Income,
        100,
        None,
        None,
        monthly(1, day(2026, 1, 1), Some(day(2026, 2, 15))),
    ));
    let off = fx.id(rent_cmd());
    let gone = fx.id(rent_cmd());

    fx.run(Command::UpdateRecurring {
        recurring_id: off,
        patch: RecurringPatch {
            enabled: Some(false),
            ..RecurringPatch::default()
        },
    });
    fx.run(Command::ArchiveRecurring { recurring_id: gone });

    assert_eq!(
        due_dates(&fx, day(2026, 6, 1)),
        vec![(bounded, vec![day(2026, 1, 1), day(2026, 2, 1)])]
    );
    assert_eq!(fx.core.list_recurring(fx.vault, false).unwrap().len(), 2);
    assert_eq!(fx.core.list_recurring(fx.vault, true).unwrap().len(), 3);

    // A disabled or archived template cannot be executed either.
    for id in [off, gone] {
        assert!(matches!(
            fx.try_run(execute(id, day(2026, 1, 1), T0)),
            Err(DomainError::InvalidCommand(_))
        ));
    }
}

#[test]
fn execute_posts_the_transaction_and_records_the_run() {
    let mut fx = setup();
    let cmd = Command::Income(entry(10_000, None, None, Some("Salary"), T0));
    fx.run(cmd);
    let id = fx.id(rent_cmd());

    let tx_id = fx.id(execute(id, day(2026, 1, 1), T0 + 100));

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
    assert_eq!(runs[0].period_date, day(2026, 1, 1));
    assert_eq!(runs[0].outcome, RunOutcome::Executed);
    assert_eq!(runs[0].transaction_id, Some(tx_id));

    // The executed period is no longer pending; the next one still is.
    assert_eq!(
        due_dates(&fx, day(2026, 2, 5)),
        vec![(id, vec![day(2026, 2, 1)])]
    );
}

#[test]
fn a_period_can_be_handled_only_once_and_only_when_due() {
    let mut fx = setup();
    fx.run(Command::Income(entry(10_000, None, None, None, T0)));
    let id = fx.id(rent_cmd());
    let on = |period: NaiveDate| execute(id, period, T0 + 100);

    // Not an occurrence of the schedule.
    assert!(matches!(
        fx.try_run(on(day(2026, 1, 2))),
        Err(DomainError::InvalidCommand(_))
    ));
    // Before start_date, even though it is a 1st of the month.
    assert!(matches!(
        fx.try_run(on(day(2025, 12, 1))),
        Err(DomainError::InvalidCommand(_))
    ));

    fx.run(on(day(2026, 1, 1)));
    assert_eq!(
        fx.try_run(on(day(2026, 1, 1))).unwrap_err(),
        DomainError::AlreadyExists("run for 2026-01-01".to_string())
    );
    assert!(matches!(
        fx.try_run(skip(id, day(2026, 1, 1))),
        Err(DomainError::AlreadyExists(_))
    ));
    assert_eq!(fx.core.recurring_runs(fx.vault, id).unwrap().len(), 1);
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 7500);
}

#[test]
fn skip_handles_the_period_without_a_transaction() {
    let mut fx = setup();
    let id = fx.id(rent_cmd());

    let receipt = fx.run(skip(id, day(2026, 1, 1)));
    assert_eq!(receipt.result_id, None);
    assert!(list(&fx.core, fx.vault, &all()).is_empty());
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 0);

    let runs = fx.core.recurring_runs(fx.vault, id).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].outcome, RunOutcome::Skipped);
    assert_eq!(runs[0].transaction_id, None);
    assert_eq!(
        due_dates(&fx, day(2026, 2, 5)),
        vec![(id, vec![day(2026, 2, 1)])]
    );
}

#[test]
fn a_refused_execution_leaves_no_run_and_no_transaction() {
    let mut fx = setup();
    fx.run(Command::Income(entry(10_000, None, None, None, T0)));
    let capped = fx.id(flow_cmd("Fun", FlowMode::NetCapped { cap: 1000 }, false, 0));
    let id = fx.id(recurring_cmd(
        TransactionKind::Income,
        5000,
        Some(capped),
        Some("Bonus"),
        monthly(1, day(2026, 1, 1), None),
    ));

    let before = balances(&fx.core, fx.vault);
    assert_eq!(
        fx.try_run(execute(id, day(2026, 1, 1), T0 + 100))
            .unwrap_err(),
        DomainError::MaxBalanceReached("Fun".to_string())
    );

    assert_eq!(balances(&fx.core, fx.vault), before);
    assert!(fx.core.recurring_runs(fx.vault, id).unwrap().is_empty());
    assert_eq!(list(&fx.core, fx.vault, &all()).len(), 1);
    assert_eq!(
        due_dates(&fx, day(2026, 1, 31)),
        vec![(id, vec![day(2026, 1, 1)])]
    );
}

#[test]
fn wallet_none_resolves_to_the_only_active_wallet() {
    let mut fx = setup();
    fx.run(Command::Income(entry(10_000, None, None, None, T0)));
    let id = fx.id(rent_cmd());
    let tx_id = fx.id(execute(id, day(2026, 1, 1), T0 + 100));
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
    fx.run(wallet_cmd("Bank", 0));
    assert!(matches!(
        fx.try_run(execute(id, day(2026, 2, 1), T0 + 200)),
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

#[test]
fn replay_rebuilds_templates_runs_and_transactions() {
    let mut fx = setup();
    let cmd = Command::Income(entry(50_000, None, None, Some("Salary"), T0));
    fx.run(cmd);
    let id = fx.id(rent_cmd());
    fx.run(execute(id, day(2026, 1, 1), T0 + 100));
    fx.run(skip(id, day(2026, 2, 1)));
    fx.run(Command::UpdateRecurring {
        recurring_id: id,
        patch: RecurringPatch {
            amount: Some(2600),
            wallet_id: Some(fx.wallet),
            note: Some("rent".to_string()),
            schedule: Some(monthly(3, day(2026, 1, 1), Some(day(2026, 12, 31)))),
            ..RecurringPatch::default()
        },
    });
    fx.run(execute(id, day(2026, 3, 3), T0 + 300));
    let archived = fx.id(rent_cmd());
    fx.run(Command::ArchiveRecurring {
        recurring_id: archived,
    });

    let fresh = replayed(&fx.core, fx.vault);
    assert_eq!(
        fresh.list_recurring(fx.vault, true).unwrap(),
        fx.core.list_recurring(fx.vault, true).unwrap()
    );
    assert_eq!(
        fresh.recurring_runs(fx.vault, id).unwrap(),
        fx.core.recurring_runs(fx.vault, id).unwrap()
    );
    assert_eq!(
        fresh.pending_recurring(fx.vault, day(2026, 6, 1)).unwrap(),
        fx.core
            .pending_recurring(fx.vault, day(2026, 6, 1))
            .unwrap()
    );
    assert_eq!(fx.core.recurring_runs(fx.vault, id).unwrap().len(), 3);
}

fn schedule(frequency: Frequency, interval: u32, start: NaiveDate) -> Schedule {
    Schedule {
        frequency,
        interval,
        start_date: start,
        end_date: None,
    }
}

#[test]
fn preview_monthly_31st_across_february() {
    let s = monthly(31, day(2027, 1, 1), None);
    assert_eq!(
        s.occurrences_from(day(2027, 1, 1), 4).unwrap(),
        [
            day(2027, 1, 31),
            day(2027, 2, 28),
            day(2027, 3, 31),
            day(2027, 4, 30)
        ]
    );
    let leap = monthly(31, day(2028, 1, 1), None);
    assert_eq!(
        leap.occurrences_from(day(2028, 1, 1), 3).unwrap(),
        [day(2028, 1, 31), day(2028, 2, 29), day(2028, 3, 31)]
    );
}

#[test]
fn preview_end_date_cuts_the_list() {
    let s = monthly(15, day(2026, 1, 1), Some(day(2026, 3, 20)));
    assert_eq!(
        s.occurrences_from(day(2026, 1, 1), 10).unwrap(),
        [day(2026, 1, 15), day(2026, 2, 15), day(2026, 3, 15)]
    );
}

#[test]
fn preview_weekly_every_two_weeks() {
    // 2026-03-02 is a Monday.
    let s = schedule(Frequency::Weekly { weekday: 1 }, 2, day(2026, 3, 2));
    assert_eq!(
        s.occurrences_from(day(2026, 3, 2), 3).unwrap(),
        [day(2026, 3, 2), day(2026, 3, 16), day(2026, 3, 30)]
    );
}

#[test]
fn preview_yearly_feb_29() {
    let s = schedule(Frequency::Yearly { month: 2, day: 29 }, 1, day(2027, 1, 1));
    assert_eq!(
        s.occurrences_from(day(2027, 1, 1), 3).unwrap(),
        [day(2027, 2, 28), day(2028, 2, 29), day(2029, 2, 28)]
    );
}

#[test]
fn preview_from_before_start_begins_at_first_occurrence() {
    let s = monthly(10, day(2026, 5, 20), None);
    assert_eq!(
        s.occurrences_from(day(2020, 1, 1), 2).unwrap(),
        [day(2026, 6, 10), day(2026, 7, 10)]
    );
}

#[test]
fn preview_from_on_an_occurrence_includes_it() {
    let s = monthly(10, day(2026, 1, 1), None);
    assert_eq!(
        s.occurrences_from(day(2026, 4, 10), 2).unwrap(),
        [day(2026, 4, 10), day(2026, 5, 10)]
    );
    assert_eq!(
        s.occurrences_from(day(2026, 4, 11), 1).unwrap(),
        [day(2026, 5, 10)]
    );
}

#[test]
fn preview_limit_zero_is_empty() {
    let s = monthly(10, day(2026, 1, 1), None);
    assert!(s.occurrences_from(day(2026, 1, 1), 0).unwrap().is_empty());
}

#[test]
fn preview_rejects_invalid_schedules() {
    let from = day(2026, 1, 1);
    let start = day(2026, 1, 1);
    for bad in [
        schedule(Frequency::Daily, 0, start),
        monthly(0, start, None),
        monthly(32, start, None),
        schedule(Frequency::Weekly { weekday: 8 }, 1, start),
        monthly(5, start, Some(day(2025, 1, 1))),
    ] {
        assert!(matches!(
            bad.occurrences_from(from, 3),
            Err(DomainError::InvalidCommand(_))
        ));
    }
}
