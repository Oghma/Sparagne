//! Loans over sync. The server has no loan code: it runs every pushed command
//! through its own `Core`, so these tests check that what one device does with
//! loans, and with the installments of a linked template, is what another
//! device and the server end up with.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::http::StatusCode;
use chrono::NaiveDate;
use common::{
    Api, Client, basics, event_cmd, execute_recurring_cmd, expense_cmd, loan_cmd, loan_terms,
    prepayment, projection, rate_change,
};
use sparagne_core::{
    Command, CommandEnvelope, Core, Frequency, Schedule, TransactionKind,
    loan::{
        LinkedInstallment, LoanInstallment, LoanPatch, LoanSchedule, LoanTerms, LoanView,
        MAX_LOAN_AMOUNT, PrepaymentMode,
    },
    sync::PushResponse,
};
use uuid::Uuid;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// What the app asks the core on this day.
fn today() -> NaiveDate {
    day(2026, 6, 15)
}

/// A monthly expense template on the 1st from 2026-01-01, whose amount is
/// nothing like an installment.
fn template() -> Command {
    Command::CreateRecurring {
        transaction_kind: TransactionKind::Expense,
        amount: 1_000,
        wallet_id: None,
        flow_id: None,
        category: Some("Mutuo".to_string()),
        note: None,
        schedule: Schedule {
            frequency: Frequency::Monthly { day: 1 },
            interval: 1,
            start_date: day(2026, 1, 1),
            end_date: None,
        },
        owner: None,
    }
}

/// 6.000,00 at 6% in 24 months from 2026-01-01.
fn terms() -> LoanTerms {
    loan_terms(600_000, 60_000, 24, day(2026, 1, 1))
}

/// Everything the loans of a vault show, in one comparable value.
type LoanProjection = (
    Vec<LoanView>,
    Vec<LoanSchedule>,
    Vec<LinkedInstallment>,
    Vec<sparagne_core::loan::LoanOverview>,
);

fn loans_of(core: &Core, vault: Uuid) -> LoanProjection {
    let loans = core.loans(vault).unwrap();
    let schedules = loans
        .iter()
        .map(|loan| core.loan_schedule(vault, loan.id).unwrap())
        .collect();
    (
        loans,
        schedules,
        core.linked_installments(vault).unwrap(),
        core.loan_overview(vault, today()).unwrap(),
    )
}

#[tokio::test]
async fn loans_converge_across_devices_and_the_server() {
    let api = Api::new();
    let (mut alice, mut bob, vault, _wallet) = basics(&api).await;

    // The template and the loan linked to it are made in one push.
    let rent = alice.exec(vault, template());
    let mortgage = alice.exec(
        vault,
        Command::CreateLoan {
            name: "Mutuo".to_string(),
            note: Some("variabile".to_string()),
            terms: terms(),
            recurring_id: Some(rent),
        },
    );
    let rates = alice.exec(
        vault,
        event_cmd(mortgage, rate_change(day(2026, 4, 1), 30_000)),
    );
    let cut = alice.exec(
        vault,
        event_cmd(
            mortgage,
            prepayment(day(2026, 5, 10), 100_000, PrepaymentMode::Lower),
        ),
    );
    let mistaken = alice.exec(
        vault,
        event_cmd(
            mortgage,
            prepayment(day(2026, 7, 10), 50_000, PrepaymentMode::Shorten),
        ),
    );
    alice.exec(
        vault,
        Command::RemoveLoanEvent {
            loan_id: mortgage,
            event_id: mistaken,
        },
    );
    alice.exec(
        vault,
        Command::UpdateLoan {
            loan_id: mortgage,
            patch: LoanPatch {
                note: Some("fisso poi variabile".to_string()),
                ..Default::default()
            },
        },
    );

    // A second loan, linked after the fact, archived and restored, and a
    // third that is left archived.
    let car_template = alice.exec(vault, template());
    let car = alice.exec(
        vault,
        loan_cmd("Auto", loan_terms(300_000, 0, 12, day(2026, 2, 5))),
    );
    alice.exec(
        vault,
        Command::SetLoanRecurring {
            loan_id: car,
            recurring_id: Some(car_template),
        },
    );
    alice.exec(vault, Command::ArchiveLoan { loan_id: car });
    alice.exec(vault, Command::RestoreLoan { loan_id: car });
    let kitchen = alice.exec(
        vault,
        loan_cmd("Cucina", loan_terms(48_000, 0, 24, day(2025, 10, 5))),
    );
    alice.exec(vault, Command::ArchiveLoan { loan_id: kitchen });

    // January's installment, recorded at the schedule's amount, not at the
    // template's 10,00.
    let january = alice
        .core
        .loan_schedule(vault, mortgage)
        .unwrap()
        .rows
        .first()
        .unwrap()
        .payment;
    assert_ne!(january, 1_000);
    let recorded = alice.exec(
        vault,
        execute_recurring_cmd(rent, day(2026, 1, 1), Some(january)),
    );

    alice.sync(&api, vault).await;
    bob.sync(&api, vault).await;

    let expected = loans_of(&api.state.core(), vault);
    assert_eq!(expected.0.len(), 3);
    assert_eq!(loans_of(&alice.core, vault), expected);
    assert_eq!(loans_of(&bob.core, vault), expected);
    let mortgage_view = expected.0.iter().find(|l| l.id == mortgage).unwrap();
    assert_eq!(mortgage_view.recurring_id, Some(rent));
    assert_eq!(mortgage_view.note.as_deref(), Some("fisso poi variabile"));
    assert_eq!(
        mortgage_view
            .events
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        vec![rates, cut],
        "the removed event is gone, the others keep the ids of their commands"
    );
    assert!(
        expected
            .0
            .iter()
            .find(|l| l.id == kitchen)
            .unwrap()
            .archived
    );
    assert!(!expected.0.iter().find(|l| l.id == car).unwrap().archived);

    // The installment is a transaction of the schedule's amount, the same
    // everywhere.
    for core in [&api.state.core(), &bob.core, &alice.core] {
        let rows = core
            .list_transactions(vault, &common::all_txns(), 100, None)
            .unwrap()
            .items;
        let row = rows.iter().find(|t| t.id == recorded).unwrap();
        assert_eq!(row.amount, january);
    }
    assert_eq!(projection(&alice.core, vault), projection(&bob.core, vault));
    assert_eq!(
        projection(&bob.core, vault),
        projection(&api.state.core(), vault)
    );

    for client in [&alice, &bob] {
        let state = client.core.sync_state(vault).unwrap();
        assert_eq!(state.outbox, 0);
        assert_eq!(state.rejected, 0);
    }
}

#[tokio::test]
async fn a_loan_edit_made_offline_is_rebased_over_a_rival_one() {
    let api = Api::new();
    let (mut alice, mut bob, vault, _wallet) = basics(&api).await;
    let loan = alice.exec(vault, loan_cmd("Mutuo", terms()));
    alice.sync(&api, vault).await;
    bob.sync(&api, vault).await;

    // Both add an early repayment before either syncs; the server's order
    // settles the schedule and both devices redraw it the same way.
    bob.exec(
        vault,
        event_cmd(
            loan,
            prepayment(day(2026, 3, 1), 100_000, PrepaymentMode::Shorten),
        ),
    );
    alice.exec(
        vault,
        event_cmd(
            loan,
            prepayment(day(2026, 3, 1), 100_000, PrepaymentMode::Lower),
        ),
    );
    for _ in 0..2 {
        bob.sync(&api, vault).await;
        alice.sync(&api, vault).await;
    }

    let expected = loans_of(&api.state.core(), vault);
    assert_eq!(expected.0[0].events.len(), 2);
    assert_eq!(loans_of(&alice.core, vault), expected);
    assert_eq!(loans_of(&bob.core, vault), expected);
}

#[tokio::test]
async fn a_refused_loan_command_does_not_stop_the_push() {
    let api = Api::new();
    let (mut alice, _bob, vault, wallet) = basics(&api).await;

    let good = CommandEnvelope::new(vault, "alice", loan_cmd("Mutuo", terms()));
    let loan = good.id;
    let too_big = CommandEnvelope::new(
        vault,
        "alice",
        loan_cmd(
            "Troppo",
            LoanTerms {
                principal: MAX_LOAN_AMOUNT + 1,
                ..terms()
            },
        ),
    );
    let on_the_refused = CommandEnvelope::new(
        vault,
        "alice",
        event_cmd(
            too_big.id,
            prepayment(day(2026, 3, 1), 100, PrepaymentMode::Lower),
        ),
    );
    let event = CommandEnvelope::new(
        vault,
        "alice",
        event_cmd(loan, rate_change(day(2026, 3, 1), 20_000)),
    );
    let below_interest = CommandEnvelope::new(
        vault,
        "alice",
        loan_cmd(
            "Rata bassa",
            LoanTerms {
                installment: LoanInstallment::Contract { amount: 1 },
                ..terms()
            },
        ),
    );
    let spesa = CommandEnvelope::new(
        vault,
        "alice",
        expense_cmd(100, wallet, None, "spesa", common::T0 + 60),
    );

    let before = api.state.core().last_seq(vault).unwrap();
    let res = api
        .push(
            &alice.token,
            vault,
            vec![good, too_big, on_the_refused, event, below_interest, spesa],
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let response: PushResponse = res.json();
    assert_eq!(response.results.len(), 6);

    assert_eq!(common::applied(&response.results[0]), before + 1);
    assert_eq!(common::rejection(&response.results[1]).0, "invalid_amount");
    assert_eq!(common::rejection(&response.results[2]).0, "not_found");
    // A refusal takes no seq: the commands after it line up right behind.
    assert_eq!(common::applied(&response.results[3]), before + 2);
    assert_eq!(common::rejection(&response.results[4]).0, "invalid_amount");
    assert_eq!(common::applied(&response.results[5]), before + 3);

    let loans = api.state.core().loans(vault).unwrap();
    assert_eq!(loans.len(), 1);
    assert_eq!(loans[0].id, loan);
    assert_eq!(loans[0].events.len(), 1);

    // The device that sent them pulls the log and agrees.
    alice
        .core
        .integrate_pull(
            vault,
            &api.get(&format!("/vaults/{vault}/pull?since=0"), &alice.token)
                .await
                .json(),
        )
        .unwrap();
    assert_eq!(
        loans_of(&alice.core, vault),
        loans_of(&api.state.core(), vault)
    );
}

#[tokio::test]
async fn a_viewer_cannot_push_loan_commands() {
    let api = Api::new();
    let (mut alice, _bob, vault, _wallet) = basics(&api).await;
    let loan = alice.exec(vault, loan_cmd("Mutuo", terms()));
    alice.sync(&api, vault).await;

    let mut carol = Client::register(&api, "carol").await;
    api.add_member(&alice.token, vault, "carol", "viewer").await;
    carol.join(&api, vault).await;
    assert_eq!(
        carol.core.loans(vault).unwrap().len(),
        1,
        "a viewer sees the loans"
    );

    // None of the loan commands gets through, one by one.
    let commands = vec![
        loan_cmd("Altro", terms()),
        event_cmd(loan, rate_change(day(2026, 3, 1), 20_000)),
        Command::ArchiveLoan { loan_id: loan },
        Command::SetLoanRecurring {
            loan_id: loan,
            recurring_id: None,
        },
    ];
    let before = api.state.core().last_seq(vault).unwrap();
    for command in commands {
        let res = api
            .push(
                &carol.token,
                vault,
                vec![CommandEnvelope::new(vault, "carol", command)],
            )
            .await;
        assert_eq!(res.status, StatusCode::FORBIDDEN);
        assert_eq!(res.code(), "forbidden");
    }
    assert_eq!(api.state.core().last_seq(vault).unwrap(), before);
    assert_eq!(api.state.core().loans(vault).unwrap().len(), 1);
}
