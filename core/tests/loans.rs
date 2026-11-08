//! Acceptance tests for loans: the seven commands, what each refuses (a
//! refusal leaves the loans and the log as they were), the link to a
//! recurring template, archiving, events, the log replayed, a deleted vault,
//! an installment recorded with its own amount, and the queries the
//! Patrimonio tab reads, worked out from the schedule.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::NaiveDate;
use common::{sync::other_vault, *};
use rusqlite::Connection;
use sparagne_core::{
    Command, CommandEnvelope, Core, DomainError, RunOutcome, TransactionKind,
    loan::{
        LinkedInstallment, LoanEvent, LoanEventView, LoanInstallment, LoanOverview, LoanPatch,
        LoanTerms, LoanView, MAX_MONTHS, MAX_RATE_PPM, PrepaymentMode,
    },
};
use uuid::Uuid;

/// 200,000.00 at 3% in 300 months from 2026-01-15, the last due on
/// 2050-12-15.
fn mortgage() -> LoanTerms {
    loan_terms(20_000_000, 30_000, 300, day(2026, 1, 15))
}

fn linked_loan_cmd(name: &str, terms: LoanTerms, recurring_id: Uuid) -> Command {
    Command::CreateLoan {
        name: name.to_string(),
        note: None,
        terms,
        recurring_id: Some(recurring_id),
    }
}

fn update(loan_id: Uuid, patch: LoanPatch) -> Command {
    Command::UpdateLoan { loan_id, patch }
}

fn link(loan_id: Uuid, recurring_id: Option<Uuid>) -> Command {
    Command::SetLoanRecurring {
        loan_id,
        recurring_id,
    }
}

fn remove(loan_id: Uuid, event_id: Uuid) -> Command {
    Command::RemoveLoanEvent { loan_id, event_id }
}

/// A monthly expense template of `amount` on `day_of_month` from `start`.
fn expense_template(amount: i64, day_of_month: u8, start: NaiveDate) -> Command {
    Command::CreateRecurring {
        transaction_kind: TransactionKind::Expense,
        amount,
        wallet_id: None,
        flow_id: None,
        category: Some("Mutuo".to_string()),
        note: None,
        schedule: monthly_from(day_of_month, start),
        owner: None,
    }
}

fn salary_template() -> Command {
    Command::CreateRecurring {
        transaction_kind: TransactionKind::Income,
        amount: 200_000,
        wallet_id: None,
        flow_id: None,
        category: None,
        note: None,
        schedule: monthly_from(27, day(2026, 1, 1)),
        owner: None,
    }
}

/// Runs `cmd`, expecting `err`, and checks that the loans and the log are
/// as they were.
fn refused(fx: &mut Fx, cmd: Command, err: &DomainError) {
    let loans = loans_of(fx);
    let seq = fx.core.last_seq(fx.vault).unwrap();
    assert_eq!(&fx.try_run(cmd).unwrap_err(), err);
    assert_eq!(loans_of(fx), loans);
    assert_eq!(fx.core.last_seq(fx.vault).unwrap(), seq);
}

fn invalid(message: &str) -> DomainError {
    DomainError::InvalidCommand(message.to_string())
}

fn invalid_amount(message: &str) -> DomainError {
    DomainError::InvalidAmount(message.to_string())
}

fn not_found(what: &str) -> DomainError {
    DomainError::NotFound(what.to_string())
}

fn overview_on(fx: &Fx, today: NaiveDate) -> Vec<LoanOverview> {
    fx.core.loan_overview(fx.vault, today).unwrap()
}

// ---------------------------------------------------------------------------
// CreateLoan
// ---------------------------------------------------------------------------

#[test]
fn create_keeps_what_the_command_said_and_lists_loans_by_name() {
    let mut fx = setup();
    let terms = LoanTerms {
        upfront_fees: 150_000,
        installment_fee: 250,
        ..mortgage()
    };
    let receipt = fx.run(Command::CreateLoan {
        name: "  Mutuo casa ".to_string(),
        note: Some("  Banca del territorio ".to_string()),
        terms,
        recurring_id: None,
    });
    assert_eq!(receipt.result_id, Some(receipt.command_id));
    assert_eq!(
        loans_of(&fx),
        vec![LoanView {
            id: receipt.command_id,
            name: "Mutuo casa".to_string(),
            note: Some("Banca del territorio".to_string()),
            terms,
            events: Vec::new(),
            recurring_id: None,
            archived: false,
            created_by: "alice".to_string(),
        }]
    );

    // A blank note is no note; names sort case aside, then by id.
    let auto = fx.id(Command::CreateLoan {
        name: "auto".to_string(),
        note: Some("   ".to_string()),
        terms: mortgage(),
        recurring_id: None,
    });
    let cucina = fx.id(loan_cmd("Cucina", mortgage()));
    let auto_too = fx.id(loan_cmd("Auto", mortgage()));
    let loans = loans_of(&fx);
    assert_eq!(loans[0].id, auto);
    assert_eq!(loans[0].note, None);
    assert_eq!(
        loans.iter().map(|l| l.id).collect::<Vec<_>>(),
        vec![auto, auto_too, cucina, receipt.command_id]
    );
}

#[test]
fn create_refuses_terms_out_of_range_and_leaves_nothing() {
    let mut fx = setup();
    let good = mortgage();
    let contract = |amount| LoanInstallment::Contract { amount };
    let cases = [
        (
            LoanTerms {
                principal: 0,
                ..good
            },
            invalid_amount("principal must be > 0"),
        ),
        (
            LoanTerms {
                principal: -1,
                ..good
            },
            invalid_amount("principal must be > 0"),
        ),
        (
            LoanTerms {
                annual_rate_ppm: MAX_RATE_PPM + 1,
                ..good
            },
            invalid_amount("annual rate must be at most 500000 ppm"),
        ),
        (
            LoanTerms { months: 0, ..good },
            invalid("months must be 1..=600"),
        ),
        (
            LoanTerms {
                months: MAX_MONTHS + 1,
                ..good
            },
            invalid("months must be 1..=600"),
        ),
        (
            LoanTerms {
                upfront_fees: -1,
                ..good
            },
            invalid_amount("upfront fees must be >= 0"),
        ),
        (
            LoanTerms {
                installment_fee: -1,
                ..good
            },
            invalid_amount("installment fee must be >= 0"),
        ),
        (
            LoanTerms {
                installment: contract(0),
                ..good
            },
            invalid_amount("amount must be > 0"),
        ),
        (
            LoanTerms {
                installment: contract(-100),
                ..good
            },
            invalid_amount("amount must be > 0"),
        ),
        (
            LoanTerms {
                first_due: NaiveDate::MAX,
                months: 2,
                ..good
            },
            invalid("the loan ends past the last date"),
        ),
    ];
    for (terms, err) in cases {
        refused(&mut fx, loan_cmd("Mutuo", terms), &err);
    }
    refused(
        &mut fx,
        loan_cmd("   ", good),
        &DomainError::InvalidName("loan name must not be empty".to_string()),
    );
    assert!(loans_of(&fx).is_empty());

    // The bounds themselves are fine.
    for terms in [
        LoanTerms {
            annual_rate_ppm: 0,
            ..good
        },
        LoanTerms {
            annual_rate_ppm: MAX_RATE_PPM,
            ..good
        },
        LoanTerms { months: 1, ..good },
        LoanTerms {
            months: MAX_MONTHS,
            ..good
        },
        LoanTerms {
            first_due: day(262_000, 1, 31),
            months: MAX_MONTHS,
            ..good
        },
    ] {
        fx.run(loan_cmd("Mutuo", terms));
    }
    assert_eq!(loans_of(&fx).len(), 5);

    // No vault, no loan.
    assert_eq!(
        fx.core
            .execute(CommandEnvelope::new(
                Uuid::now_v7(),
                "alice",
                loan_cmd("Mutuo", good)
            ))
            .unwrap_err(),
        not_found("vault")
    );
}

/// 100,000.00 at 6%: the first month's interest is 500.00.
#[test]
#[ignore = "waits for loan::math"]
fn a_contract_installment_must_pay_more_than_the_first_months_interest() {
    let mut fx = setup();
    let terms = loan_terms(10_000_000, 60_000, 120, day(2026, 1, 15));
    let contract = |amount| LoanInstallment::Contract { amount };
    let below = invalid_amount("the installment must be above the first month's interest");
    refused(
        &mut fx,
        loan_cmd(
            "Mutuo",
            LoanTerms {
                installment: contract(50_000),
                ..terms
            },
        ),
        &below,
    );
    let id = fx.id(loan_cmd(
        "Mutuo",
        LoanTerms {
            installment: contract(50_001),
            ..terms
        },
    ));
    // At a zero rate any installment pays more than the interest.
    fx.run(loan_cmd(
        "Zero",
        LoanTerms {
            annual_rate_ppm: 0,
            installment: contract(1),
            ..terms
        },
    ));

    // An update that touches the principal, the rate or the installment is
    // checked again.
    for patch in [
        LoanPatch {
            principal: Some(20_000_000),
            ..LoanPatch::default()
        },
        LoanPatch {
            annual_rate_ppm: Some(70_000),
            ..LoanPatch::default()
        },
        LoanPatch {
            installment: Some(contract(40_000)),
            ..LoanPatch::default()
        },
    ] {
        refused(&mut fx, update(id, patch), &below);
    }
    fx.run(update(
        id,
        LoanPatch {
            months: Some(240),
            upfront_fees: Some(10_000),
            ..LoanPatch::default()
        },
    ));
    fx.run(update(
        id,
        LoanPatch {
            installment: Some(LoanInstallment::Computed),
            ..LoanPatch::default()
        },
    ));
}

// ---------------------------------------------------------------------------
// The link to a recurring template
// ---------------------------------------------------------------------------

#[test]
fn create_links_only_an_active_expense_template_of_the_vault_no_loan_has() {
    let mut fx = setup();
    let rent = fx.id(rent_cmd());
    let id = fx.id(linked_loan_cmd("Mutuo", mortgage(), rent));
    assert_eq!(loans_of(&fx)[0].id, id);
    assert_eq!(loans_of(&fx)[0].recurring_id, Some(rent));

    let salary = fx.id(salary_template());
    let archived = fx.id(rent_cmd());
    fx.run(Command::ArchiveRecurring {
        recurring_id: archived,
    });
    let (elsewhere_vault, _) = other_vault(&mut fx.core);
    let elsewhere = run(&mut fx.core, elsewhere_vault, rent_cmd())
        .result_id
        .unwrap();
    for (recurring_id, err) in [
        (Uuid::now_v7(), not_found("recurring")),
        (elsewhere, not_found("recurring")),
        (salary, invalid("a loan links an expense recurring")),
        (archived, invalid("recurring is archived")),
        // Checked before the row is written: never a storage error.
        (rent, invalid("recurring is linked to another loan")),
    ] {
        refused(
            &mut fx,
            linked_loan_cmd("Cucina", mortgage(), recurring_id),
            &err,
        );
    }
}

#[test]
fn set_loan_recurring_links_moves_and_unlinks() {
    let mut fx = setup();
    let rent = fx.id(rent_cmd());
    let other = fx.id(rent_cmd());
    let a = fx.id(loan_cmd("A", mortgage()));
    let b = fx.id(loan_cmd("B", mortgage()));
    let linked =
        |fx: &Fx| -> Vec<Option<Uuid>> { loans_of(fx).iter().map(|l| l.recurring_id).collect() };

    assert_eq!(fx.run(link(a, Some(rent))).result_id, None);
    assert_eq!(linked(&fx), vec![Some(rent), None]);
    // The template it already has: nothing changes.
    fx.run(link(a, Some(rent)));
    assert_eq!(linked(&fx), vec![Some(rent), None]);
    refused(
        &mut fx,
        link(b, Some(rent)),
        &invalid("recurring is linked to another loan"),
    );

    // Moving to another template frees the first one.
    fx.run(link(a, Some(other)));
    fx.run(link(b, Some(rent)));
    assert_eq!(linked(&fx), vec![Some(other), Some(rent)]);

    // Unlinking, twice: the second changes nothing.
    fx.run(link(a, None));
    fx.run(link(a, None));
    assert_eq!(linked(&fx), vec![None, Some(rent)]);
    fx.run(link(b, Some(other)));
    fx.run(link(a, Some(rent)));
    assert_eq!(linked(&fx), vec![Some(rent), Some(other)]);

    let salary = fx.id(salary_template());
    let archived = fx.id(rent_cmd());
    fx.run(Command::ArchiveRecurring {
        recurring_id: archived,
    });
    let (elsewhere_vault, _) = other_vault(&mut fx.core);
    let foreign = run(&mut fx.core, elsewhere_vault, loan_cmd("Altro", mortgage()))
        .result_id
        .unwrap();
    let free = fx.id(rent_cmd());
    for (cmd, err) in [
        (link(Uuid::now_v7(), Some(free)), not_found("loan")),
        (link(foreign, Some(free)), not_found("loan")),
        (link(a, Some(Uuid::now_v7())), not_found("recurring")),
        (
            link(a, Some(salary)),
            invalid("a loan links an expense recurring"),
        ),
        (link(a, Some(archived)), invalid("recurring is archived")),
    ] {
        refused(&mut fx, cmd, &err);
    }
    assert_eq!(linked(&fx), vec![Some(rent), Some(other)]);
}

// ---------------------------------------------------------------------------
// UpdateLoan
// ---------------------------------------------------------------------------

#[test]
fn update_changes_only_the_fields_the_patch_carries() {
    let mut fx = setup();
    let rent = fx.id(rent_cmd());
    let id = fx.id(Command::CreateLoan {
        name: "Mutuo".to_string(),
        note: Some("Banca".to_string()),
        terms: mortgage(),
        recurring_id: Some(rent),
    });
    let event = fx.id(event_cmd(id, rate_change(day(2027, 1, 15), 40_000)));

    assert_eq!(
        fx.run(update(
            id,
            LoanPatch {
                name: Some(" Mutuo casa ".to_string()),
                principal: Some(19_000_000),
                months: Some(240),
                first_due: Some(day(2026, 2, 28)),
                upfront_fees: Some(100),
                installment_fee: Some(200),
                ..LoanPatch::default()
            },
        ))
        .result_id,
        None
    );
    let loan = loans_of(&fx).remove(0);
    assert_eq!(loan.name, "Mutuo casa");
    assert_eq!(loan.note.as_deref(), Some("Banca"));
    assert_eq!(
        loan.terms,
        LoanTerms {
            principal: 19_000_000,
            annual_rate_ppm: 30_000,
            months: 240,
            first_due: day(2026, 2, 28),
            installment: LoanInstallment::Computed,
            upfront_fees: 100,
            installment_fee: 200,
        }
    );
    assert_eq!(loan.recurring_id, Some(rent));
    assert_eq!(
        loan.events.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![event]
    );

    // A blank note clears it.
    fx.run(update(
        id,
        LoanPatch {
            note: Some("  ".to_string()),
            annual_rate_ppm: Some(0),
            ..LoanPatch::default()
        },
    ));
    let loan = loans_of(&fx).remove(0);
    assert_eq!(loan.note, None);
    assert_eq!(loan.terms.annual_rate_ppm, 0);

    // Terms that leave an event outside the loan are kept, the event too:
    // the schedule warns.
    fx.run(update(
        id,
        LoanPatch {
            months: Some(2),
            ..LoanPatch::default()
        },
    ));
    assert_eq!(loans_of(&fx)[0].events.len(), 1);

    let (elsewhere_vault, _) = other_vault(&mut fx.core);
    let foreign = run(&mut fx.core, elsewhere_vault, loan_cmd("Altro", mortgage()))
        .result_id
        .unwrap();
    let name = |name: &str| LoanPatch {
        name: Some(name.to_string()),
        ..LoanPatch::default()
    };
    for (cmd, err) in [
        (
            update(id, LoanPatch::default()),
            invalid("nothing to update"),
        ),
        (
            update(id, name("  ")),
            DomainError::InvalidName("loan name must not be empty".to_string()),
        ),
        (
            update(
                id,
                LoanPatch {
                    principal: Some(0),
                    ..LoanPatch::default()
                },
            ),
            invalid_amount("principal must be > 0"),
        ),
        (
            update(
                id,
                LoanPatch {
                    annual_rate_ppm: Some(MAX_RATE_PPM + 1),
                    ..LoanPatch::default()
                },
            ),
            invalid_amount("annual rate must be at most 500000 ppm"),
        ),
        (
            update(
                id,
                LoanPatch {
                    months: Some(0),
                    ..LoanPatch::default()
                },
            ),
            invalid("months must be 1..=600"),
        ),
        (
            update(
                id,
                LoanPatch {
                    installment_fee: Some(-1),
                    ..LoanPatch::default()
                },
            ),
            invalid_amount("installment fee must be >= 0"),
        ),
        (
            update(
                id,
                LoanPatch {
                    installment: Some(LoanInstallment::Contract { amount: 0 }),
                    ..LoanPatch::default()
                },
            ),
            invalid_amount("amount must be > 0"),
        ),
        (update(Uuid::now_v7(), name("Altro")), not_found("loan")),
        (update(foreign, name("Altro")), not_found("loan")),
    ] {
        refused(&mut fx, cmd, &err);
    }
}

// ---------------------------------------------------------------------------
// ArchiveLoan, RestoreLoan
// ---------------------------------------------------------------------------

#[test]
fn an_archived_loan_keeps_its_template_and_takes_nothing_but_a_restore() {
    let mut fx = setup();
    let rent = fx.id(rent_cmd());
    let id = fx.id(linked_loan_cmd("Mutuo", mortgage(), rent));
    let event = fx.id(event_cmd(
        id,
        prepayment(day(2027, 3, 1), 100_000, PrepaymentMode::Shorten),
    ));

    assert_eq!(fx.run(Command::ArchiveLoan { loan_id: id }).result_id, None);
    let loan = loans_of(&fx).remove(0);
    assert!(loan.archived);
    assert_eq!(loan.recurring_id, Some(rent));
    assert_eq!(loan.events.len(), 1);

    refused(
        &mut fx,
        Command::ArchiveLoan { loan_id: id },
        &invalid("loan is already archived"),
    );
    for cmd in [
        update(
            id,
            LoanPatch {
                name: Some("Casa".to_string()),
                ..LoanPatch::default()
            },
        ),
        link(id, None),
        event_cmd(id, rate_change(day(2027, 1, 15), 40_000)),
        remove(id, event),
    ] {
        refused(&mut fx, cmd, &invalid("loan is archived"));
    }
    // Its template is still its own.
    let other = fx.id(loan_cmd("Altro", mortgage()));
    refused(
        &mut fx,
        link(other, Some(rent)),
        &invalid("recurring is linked to another loan"),
    );

    assert_eq!(fx.run(Command::RestoreLoan { loan_id: id }).result_id, None);
    let loan = loans_of(&fx).into_iter().find(|l| l.id == id).unwrap();
    assert!(!loan.archived);
    assert_eq!(loan.recurring_id, Some(rent));
    refused(
        &mut fx,
        Command::RestoreLoan { loan_id: id },
        &invalid("loan is already active"),
    );
    for cmd in [
        Command::ArchiveLoan {
            loan_id: Uuid::now_v7(),
        },
        Command::RestoreLoan {
            loan_id: Uuid::now_v7(),
        },
    ] {
        refused(&mut fx, cmd, &not_found("loan"));
    }
}

// ---------------------------------------------------------------------------
// AddLoanEvent, RemoveLoanEvent
// ---------------------------------------------------------------------------

#[test]
fn events_are_kept_by_date_then_id_and_removed_one_by_one() {
    let mut fx = setup();
    let id = fx.id(loan_cmd("Mutuo", mortgage()));

    let receipt = fx.run(event_cmd(id, rate_change(day(2027, 1, 15), 40_000)));
    assert_eq!(receipt.result_id, Some(receipt.command_id));
    let rate = receipt.command_id;
    let june = prepayment(day(2026, 6, 1), 1_000_000, PrepaymentMode::Shorten);
    let also_june = LoanEvent::Prepayment {
        date: day(2026, 6, 1),
        amount: 500_000,
        mode: PrepaymentMode::Lower,
        fee: 5_000,
    };
    let before_first = prepayment(day(2025, 12, 1), 100, PrepaymentMode::Shorten);
    let later = fx.id(event_cmd(id, june));
    let same_day = fx.id(event_cmd(id, also_june));
    let early = fx.id(event_cmd(id, before_first));
    let view = |id, event| LoanEventView { id, event };
    assert_eq!(
        loans_of(&fx)[0].events,
        vec![
            view(early, before_first),
            view(later, june),
            view(same_day, also_june),
            view(rate, rate_change(day(2027, 1, 15), 40_000)),
        ]
    );

    assert_eq!(fx.run(remove(id, later)).result_id, None);
    assert_eq!(
        loans_of(&fx)[0]
            .events
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        vec![early, same_day, rate]
    );

    let other = fx.id(loan_cmd("Altro", mortgage()));
    for (cmd, err) in [
        (remove(id, later), not_found("loan event")),
        (remove(other, same_day), not_found("loan event")),
        (remove(Uuid::now_v7(), same_day), not_found("loan")),
        (
            event_cmd(Uuid::now_v7(), rate_change(day(2027, 1, 15), 1)),
            not_found("loan"),
        ),
    ] {
        refused(&mut fx, cmd, &err);
    }
}

/// 10,000.00 in two installments from 2026-01-31: the second is due on
/// 2026-02-28, the month's last day.
#[test]
fn an_event_is_refused_on_its_own_values_and_outside_the_loan() {
    let mut fx = setup();
    let id = fx.id(loan_cmd(
        "Cucina",
        loan_terms(1_000_000, 75_000, 2, day(2026, 1, 31)),
    ));
    let contract = |amount| LoanInstallment::Contract { amount };
    let shorten = PrepaymentMode::Shorten;
    for event in [
        rate_change(day(2026, 1, 31), 0),
        rate_change(day(2026, 2, 28), MAX_RATE_PPM),
        LoanEvent::RateChange {
            date: day(2026, 2, 1),
            annual_rate_ppm: 80_000,
            installment: contract(1),
        },
        prepayment(day(2026, 2, 28), 1, shorten),
        // An early repayment may come before the first installment, any day.
        prepayment(day(2020, 1, 1), 50_000, shorten),
        prepayment(day(2026, 1, 30), 50_000, PrepaymentMode::Lower),
    ] {
        fx.run(event_cmd(id, event));
    }
    assert_eq!(loans_of(&fx)[0].events.len(), 6);

    let outside = invalid("the event's date is outside the loan");
    let penalty = |fee| LoanEvent::Prepayment {
        date: day(2026, 2, 1),
        amount: 100,
        mode: shorten,
        fee,
    };
    for (event, err) in [
        (rate_change(day(2026, 1, 30), 30_000), outside.clone()),
        (rate_change(day(2026, 3, 1), 30_000), outside.clone()),
        (prepayment(day(2026, 3, 1), 100, shorten), outside.clone()),
        (
            prepayment(day(2026, 2, 1), 0, shorten),
            invalid_amount("amount must be > 0"),
        ),
        (
            prepayment(day(2026, 2, 1), -5, shorten),
            invalid_amount("amount must be > 0"),
        ),
        (penalty(-1), invalid_amount("penalty must be >= 0")),
        (
            rate_change(day(2026, 2, 1), MAX_RATE_PPM + 1),
            invalid_amount("annual rate must be at most 500000 ppm"),
        ),
        (
            LoanEvent::RateChange {
                date: day(2026, 2, 1),
                annual_rate_ppm: 30_000,
                installment: contract(0),
            },
            invalid_amount("amount must be > 0"),
        ),
    ] {
        refused(&mut fx, event_cmd(id, event), &err);
    }
    fx.run(event_cmd(id, penalty(0)));

    // The window follows the terms as they are now.
    fx.run(update(
        id,
        LoanPatch {
            months: Some(3),
            ..LoanPatch::default()
        },
    ));
    fx.run(event_cmd(id, rate_change(day(2026, 3, 31), 30_000)));
    refused(
        &mut fx,
        event_cmd(id, rate_change(day(2026, 4, 1), 30_000)),
        &outside,
    );
}

// ---------------------------------------------------------------------------
// Replay and deletion
// ---------------------------------------------------------------------------

#[test]
fn a_replayed_log_gives_the_same_loans() {
    let mut fx = setup();
    let rent = fx.id(rent_cmd());
    let other = fx.id(rent_cmd());
    let mutuo = fx.id(Command::CreateLoan {
        name: "Mutuo".to_string(),
        note: Some("Banca".to_string()),
        terms: LoanTerms {
            upfront_fees: 120_000,
            installment_fee: 150,
            ..mortgage()
        },
        recurring_id: Some(rent),
    });
    let cucina = fx.id(loan_cmd(
        "Cucina",
        loan_terms(500_000, 0, 24, day(2026, 3, 10)),
    ));
    fx.run(event_cmd(mutuo, rate_change(day(2028, 1, 15), 35_000)));
    let gone = fx.id(event_cmd(
        mutuo,
        prepayment(day(2027, 1, 20), 2_000_000, PrepaymentMode::Lower),
    ));
    fx.run(event_cmd(
        mutuo,
        prepayment(day(2029, 5, 2), 1_000_000, PrepaymentMode::Shorten),
    ));
    fx.run(remove(mutuo, gone));
    fx.run(update(
        cucina,
        LoanPatch {
            name: Some("Rata cucina".to_string()),
            months: Some(20),
            ..LoanPatch::default()
        },
    ));
    fx.run(link(cucina, Some(other)));
    fx.run(Command::ArchiveLoan { loan_id: cucina });
    fx.run(link(mutuo, None));
    fx.run(link(mutuo, Some(rent)));

    let fresh = replayed(&fx.core, fx.vault);
    let loans = loans_of(&fx);
    assert_eq!(loans.len(), 2);
    assert_eq!(loans[0].events.len(), 2);
    assert_eq!(fresh.loans(fx.vault).unwrap(), loans);
}

#[test]
fn deleting_the_vault_deletes_its_loans_and_their_events() {
    let db = TempDb::new("loans");
    let mut core = Core::open(db.path()).unwrap();
    let casa = create_vault(&mut core, "alice", "Casa");
    let altro = create_vault(&mut core, "alice", "Altro");
    for vault in [casa, altro] {
        let id = run(&mut core, vault, loan_cmd("Mutuo", mortgage()))
            .result_id
            .unwrap();
        run(
            &mut core,
            vault,
            event_cmd(id, rate_change(day(2027, 1, 15), 40_000)),
        );
    }

    run(&mut core, casa, Command::DeleteVault);
    assert!(core.loans(casa).unwrap().is_empty());
    assert_eq!(core.loans(altro).unwrap().len(), 1);
    let conn = Connection::open(db.path()).unwrap();
    let count = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(count("loans"), 1);
    assert_eq!(count("loan_events"), 1);
}

// ---------------------------------------------------------------------------
// ExecuteRecurring with an amount
// ---------------------------------------------------------------------------

#[test]
fn an_execution_records_the_amount_it_carries_or_the_templates() {
    let mut fx = setup();
    let rent = fx.id(rent_cmd());
    let january = fx.run(execute_recurring_cmd(rent, day(2026, 1, 1), Some(94_842)));
    let february = fx.run(execute_recurring_cmd(rent, day(2026, 2, 1), None));
    let amount = |fx: &Fx, id: Uuid| fx.core.transaction(fx.vault, id).unwrap().amount;
    assert_eq!(amount(&fx, january.result_id.unwrap()), 94_842);
    assert_eq!(amount(&fx, february.result_id.unwrap()), 2_500);
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), -97_342);

    // Zero or less is refused like any entry's amount, and nothing is
    // recorded: the period is still due.
    let transactions = list(&fx.core, fx.vault, &all());
    let seq = fx.core.last_seq(fx.vault).unwrap();
    for refused_amount in [0, -1] {
        assert_eq!(
            fx.try_run(execute_recurring_cmd(
                rent,
                day(2026, 3, 1),
                Some(refused_amount)
            ))
            .unwrap_err(),
            invalid_amount("amount must be > 0")
        );
    }
    assert_eq!(list(&fx.core, fx.vault, &all()), transactions);
    assert_eq!(fx.core.last_seq(fx.vault).unwrap(), seq);
    assert_eq!(fx.core.recurring_runs(fx.vault, rent).unwrap().len(), 2);
    let pending = fx
        .core
        .pending_recurring(fx.vault, day(2026, 3, 1))
        .unwrap();
    assert_eq!(pending[0].due, vec![day(2026, 3, 1)]);

    // The amount travels in the log, and only when there is one; a replay
    // records it again.
    let payloads: Vec<String> = fx
        .core
        .commands_since(fx.vault, 0)
        .unwrap()
        .iter()
        .filter(|record| matches!(record.envelope.command, Command::ExecuteRecurring { .. }))
        .map(|record| serde_json::to_string(&record.envelope.command).unwrap())
        .collect();
    assert_eq!(payloads.len(), 2);
    assert!(payloads[0].contains("\"amount\":94842"), "{}", payloads[0]);
    assert!(!payloads[1].contains("amount"), "{}", payloads[1]);
    let fresh = replayed(&fx.core, fx.vault);
    assert_eq!(
        fresh
            .transaction(fx.vault, january.result_id.unwrap())
            .unwrap()
            .amount,
        94_842
    );
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

#[test]
fn the_queries_skip_archived_loans_and_read_only_their_vault() {
    let mut fx = setup();
    let rent = fx.id(rent_cmd());
    let id = fx.id(linked_loan_cmd("Mutuo", mortgage(), rent));
    fx.run(Command::ArchiveLoan { loan_id: id });
    assert!(overview_on(&fx, day(2027, 1, 1)).is_empty());
    assert!(fx.core.linked_installments(fx.vault).unwrap().is_empty());

    let (elsewhere, _) = other_vault(&mut fx.core);
    for (vault, loan) in [(fx.vault, Uuid::now_v7()), (elsewhere, id)] {
        assert_eq!(
            fx.core.loan_schedule(vault, loan).unwrap_err(),
            not_found("loan")
        );
    }
    assert!(fx.core.loans(elsewhere).unwrap().is_empty());
    assert!(fx.core.loans(Uuid::now_v7()).unwrap().is_empty());
}

#[test]
fn preview_refuses_what_the_commands_refuse() {
    let fx = setup();
    let good = mortgage();
    let event = |event| {
        vec![LoanEventView {
            id: Uuid::now_v7(),
            event,
        }]
    };
    for (terms, events, err) in [
        (
            LoanTerms { months: 0, ..good },
            Vec::new(),
            invalid("months must be 1..=600"),
        ),
        (
            LoanTerms {
                principal: 0,
                ..good
            },
            Vec::new(),
            invalid_amount("principal must be > 0"),
        ),
        (
            good,
            event(prepayment(day(2027, 1, 1), 0, PrepaymentMode::Shorten)),
            invalid_amount("amount must be > 0"),
        ),
        (
            good,
            event(rate_change(day(2027, 1, 15), MAX_RATE_PPM + 1)),
            invalid_amount("annual rate must be at most 500000 ppm"),
        ),
    ] {
        assert_eq!(fx.core.preview_loan(&terms, &events).unwrap_err(), err);
    }
}

/// The plan's mortgage: 948.42 a month, the last 949.60, 84,527.18 of
/// interest. The first month's interest is 500.00, so the first installment
/// repays 448.42.
#[test]
#[ignore = "waits for loan::math"]
fn a_schedule_is_the_previews_of_the_same_terms_and_events() {
    let mut fx = setup();
    let id = fx.id(loan_cmd("Mutuo", mortgage()));
    let schedule = fx.core.loan_schedule(fx.vault, id).unwrap();
    assert_eq!(schedule.rows.len(), 300);
    let first = &schedule.rows[0];
    assert_eq!(
        (first.due_date, first.payment, first.interest, first.capital),
        (day(2026, 1, 15), 94_842, 50_000, 44_842)
    );
    assert_eq!(first.residual, 19_955_158);
    let last = schedule.rows.last().unwrap();
    assert_eq!((last.due_date, last.payment), (day(2050, 12, 15), 94_960));
    assert_eq!(schedule.total_interest, 8_452_718);
    assert_eq!(fx.core.preview_loan(&mortgage(), &[]).unwrap(), schedule);

    fx.run(event_cmd(id, rate_change(day(2027, 1, 15), 40_000)));
    fx.run(event_cmd(
        id,
        prepayment(day(2028, 2, 1), 1_000_000, PrepaymentMode::Lower),
    ));
    let loan = loans_of(&fx).remove(0);
    assert_eq!(
        fx.core.loan_schedule(fx.vault, id).unwrap(),
        fx.core.preview_loan(&loan.terms, &loan.events).unwrap()
    );
    // An archived loan still has its schedule.
    fx.run(Command::ArchiveLoan { loan_id: id });
    assert_eq!(
        fx.core.loan_schedule(fx.vault, id).unwrap(),
        fx.core.preview_loan(&loan.terms, &loan.events).unwrap()
    );
}

#[test]
#[ignore = "waits for loan::math"]
fn the_overview_counts_the_installments_due_by_the_day() {
    let mut fx = setup();
    let terms = LoanTerms {
        installment_fee: 250,
        upfront_fees: 100_000,
        ..mortgage()
    };
    let id = fx.id(loan_cmd("Mutuo", terms));
    let schedule = fx.core.loan_schedule(fx.vault, id).unwrap();
    let total_interest = schedule.total_interest;

    // Before the first installment: nothing is paid.
    assert_eq!(
        overview_on(&fx, day(2026, 1, 14)),
        vec![LoanOverview {
            loan_id: id,
            residual: 20_000_000,
            paid_count: 0,
            total_count: 300,
            next_due: Some(day(2026, 1, 15)),
            next_payment: Some(94_842 + 250),
            capital_repaid: 0,
            interest_paid: 0,
            interest_left: total_interest,
            end_date: Some(day(2050, 12, 15)),
            taeg_ppm: schedule.taeg_ppm,
            total_cost: schedule.total_cost,
            registered_count: None,
            warnings: schedule.warnings.clone(),
        }]
    );

    // On the third due date, three are.
    let third = overview_on(&fx, day(2026, 3, 15)).remove(0);
    let paid = &schedule.rows[..3];
    let interest_paid: i64 = paid.iter().map(|r| r.interest).sum();
    assert_eq!(third.paid_count, 3);
    assert_eq!(third.residual, paid[2].residual);
    assert_eq!(third.capital_repaid, 20_000_000 - paid[2].residual);
    assert_eq!(
        third.capital_repaid,
        paid.iter().map(|r| r.capital).sum::<i64>()
    );
    assert_eq!(third.interest_paid, interest_paid);
    assert_eq!(third.interest_left, total_interest - interest_paid);
    assert_eq!(third.next_due, Some(day(2026, 4, 15)));
    assert_eq!(
        third.next_payment,
        Some(schedule.rows[3].payment + schedule.rows[3].fee)
    );

    // After the end, all of them, and no next one.
    let end = overview_on(&fx, day(2051, 1, 1)).remove(0);
    assert_eq!(
        (end.residual, end.paid_count, end.next_due, end.next_payment),
        (0, 300, None, None)
    );
    assert_eq!(
        (end.capital_repaid, end.interest_paid, end.interest_left),
        (20_000_000, total_interest, 0)
    );
}

/// 10,000.00 at 7.5% in 60 months from 2026-01-10, with early repayments
/// before the first installment and between the first two.
#[test]
#[ignore = "waits for loan::math"]
fn the_overview_counts_only_the_repayments_dated_by_the_day() {
    let mut fx = setup();
    let id = fx.id(loan_cmd(
        "Auto",
        loan_terms(1_000_000, 75_000, 60, day(2026, 1, 10)),
    ));
    let before = fx.id(event_cmd(
        id,
        prepayment(day(2026, 1, 1), 50_000, PrepaymentMode::Shorten),
    ));
    let between = fx.id(event_cmd(
        id,
        prepayment(day(2026, 1, 20), 100_000, PrepaymentMode::Shorten),
    ));
    let schedule = fx.core.loan_schedule(fx.vault, id).unwrap();
    assert_eq!(schedule.event_ids_before_first, vec![before]);
    assert_eq!(schedule.rows[0].event_ids, vec![between]);
    let residual = |fx: &Fx, today| overview_on(fx, today)[0].residual;

    assert_eq!(residual(&fx, day(2025, 12, 31)), 1_000_000);
    assert_eq!(residual(&fx, day(2026, 1, 5)), 950_000);
    assert_eq!(
        residual(&fx, day(2026, 1, 15)),
        950_000 - schedule.rows[0].capital
    );
    assert_eq!(residual(&fx, day(2026, 1, 20)), schedule.rows[0].residual);
    assert_eq!(
        schedule.rows[0].residual,
        950_000 - schedule.rows[0].capital - 100_000
    );

    // A repayment beyond the residual pays it off, as the schedule clips it.
    fx.run(remove(id, between));
    let all_of_it = fx.id(event_cmd(
        id,
        prepayment(day(2026, 1, 20), 5_000_000, PrepaymentMode::Shorten),
    ));
    let schedule = fx.core.loan_schedule(fx.vault, id).unwrap();
    assert_eq!(schedule.rows.len(), 1);
    assert_eq!(schedule.rows[0].event_ids, vec![all_of_it]);
    assert_eq!(
        residual(&fx, day(2026, 1, 15)),
        950_000 - schedule.rows[0].capital
    );
    let paid_off = overview_on(&fx, day(2026, 1, 20)).remove(0);
    assert_eq!(
        (
            paid_off.residual,
            paid_off.capital_repaid,
            paid_off.next_due
        ),
        (0, 1_000_000, None)
    );
    assert_eq!(paid_off.end_date, Some(day(2026, 1, 10)));
}

/// The template starts a month before the loan: its run of 2025-12-15 is
/// not one of the loan's.
#[test]
#[ignore = "waits for loan::math"]
fn registered_counts_the_live_executions_from_the_first_due_date_to_the_day() {
    let mut fx = setup();
    let template = fx.id(expense_template(94_842, 15, day(2025, 12, 1)));
    let id = fx.id(linked_loan_cmd("Mutuo", mortgage(), template));
    let execute = |fx: &mut Fx, period: NaiveDate| -> Uuid {
        fx.run(execute_recurring_cmd(template, period, Some(94_842)))
            .result_id
            .unwrap()
    };
    execute(&mut fx, day(2025, 12, 15));
    execute(&mut fx, day(2026, 1, 15));
    let february = execute(&mut fx, day(2026, 2, 15));
    execute(&mut fx, day(2026, 3, 15));
    fx.run(Command::SkipRecurring {
        recurring_id: template,
        period_date: day(2026, 4, 15),
    });
    execute(&mut fx, day(2026, 5, 15));
    fx.run(Command::VoidTransaction {
        transaction_id: february,
    });
    let runs = fx.core.recurring_runs(fx.vault, template).unwrap();
    assert_eq!(
        runs.iter()
            .filter(|r| r.outcome == RunOutcome::Executed)
            .count(),
        5
    );

    let registered = |fx: &Fx, today| overview_on(fx, today)[0].registered_count;
    // January and March: February is voided, April skipped, May not due.
    assert_eq!(registered(&fx, day(2026, 4, 20)), Some(2));
    assert_eq!(registered(&fx, day(2026, 5, 15)), Some(3));
    assert_eq!(registered(&fx, day(2026, 1, 14)), Some(0));
    let overview = overview_on(&fx, day(2026, 4, 20)).remove(0);
    assert_eq!(overview.loan_id, id);
    assert_eq!(overview.paid_count, 4);

    // Unlinked, nothing is counted.
    fx.run(link(id, None));
    assert_eq!(registered(&fx, day(2026, 4, 20)), None);
}

#[test]
#[ignore = "waits for loan::math"]
fn linked_installments_list_every_due_date_of_the_active_linked_loans() {
    let mut fx = setup();
    let mutuo_template = fx.id(expense_template(95_092, 15, day(2026, 1, 1)));
    let cucina_template = fx.id(expense_template(25_000, 10, day(2026, 3, 1)));
    let old_template = fx.id(expense_template(10_000, 5, day(2026, 1, 1)));
    let mutuo = fx.id(linked_loan_cmd(
        "Mutuo",
        LoanTerms {
            installment_fee: 250,
            ..mortgage()
        },
        mutuo_template,
    ));
    let cucina = fx.id(linked_loan_cmd(
        "Cucina",
        loan_terms(500_000, 0, 20, day(2026, 3, 10)),
        cucina_template,
    ));
    // Unlinked, archived: neither is listed.
    fx.run(loan_cmd(
        "Auto",
        loan_terms(1_000_000, 75_000, 60, day(2026, 1, 10)),
    ));
    let old = fx.id(linked_loan_cmd(
        "Vecchio",
        loan_terms(100_000, 50_000, 12, day(2026, 1, 5)),
        old_template,
    ));
    fx.run(Command::ArchiveLoan { loan_id: old });
    fx.run(event_cmd(mutuo, rate_change(day(2027, 1, 15), 40_000)));

    let expected: Vec<LinkedInstallment> = [(cucina, cucina_template), (mutuo, mutuo_template)]
        .into_iter()
        .flat_map(|(loan, recurring_id)| {
            fx.core
                .loan_schedule(fx.vault, loan)
                .unwrap()
                .rows
                .into_iter()
                .map(move |row| LinkedInstallment {
                    recurring_id,
                    due_date: row.due_date,
                    amount: row.payment + row.fee,
                })
        })
        .collect();
    let linked = fx.core.linked_installments(fx.vault).unwrap();
    assert_eq!(linked, expected);
    assert_eq!(linked.len(), 20 + 300);
    assert_eq!(linked[0].amount, 25_000);
    assert_eq!(linked[20].amount, 94_842 + 250);
}
