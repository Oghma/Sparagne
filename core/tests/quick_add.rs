//! Integration tests for quick-add resolution against a vault.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::{FixedOffset, NaiveDate, TimeZone, Utc};
use common::*;
use sparagne_core::quick_add::{self, DateSpec, QuickAdd, QuickAddDefaults, QuickAddError};
use sparagne_core::{Command, Currency, DomainError, FlowMode, TransactionKind};
use uuid::Uuid;

/// A vault with wallets Cash, Bank, Bancoposta and flows Vacanze, Spesa.
struct Fixture {
    fx: Fx,
    cash: Uuid,
    bank: Uuid,
    bancoposta: Uuid,
    vacanze: Uuid,
    spesa: Uuid,
}

fn build_fixture() -> Fixture {
    let mut fx = setup();
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0))
        .result_id
        .unwrap();
    let bancoposta = run(&mut fx.core, fx.vault, wallet_cmd("Bancoposta", 0))
        .result_id
        .unwrap();
    let vacanze = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Vacanze", FlowMode::Unlimited, false, 5000),
    )
    .result_id
    .unwrap();
    let spesa = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Spesa", FlowMode::Unlimited, false, 0),
    )
    .result_id
    .unwrap();
    let cash = fx.wallet;
    Fixture {
        fx,
        cash,
        bank,
        bancoposta,
        vacanze,
        spesa,
    }
}

fn no_defaults() -> QuickAddDefaults {
    QuickAddDefaults {
        wallet_id: None,
        flow_id: None,
    }
}

#[test]
fn exact_beats_prefix_but_ambiguous_prefix_errors() {
    let f = build_fixture();
    let now = at(T0);

    // "@bank" is an exact match for Bank, even though it is also a prefix of
    // Bancoposta.
    let parsed = quick_add::parse("15 pizza @bank", Currency::Eur).unwrap();
    let resolved =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap();
    match resolved.command {
        Command::Expense(entry) => assert_eq!(entry.wallet_id, Some(f.bank)),
        other => panic!("unexpected {other:?}"),
    }

    // "@bancoposta" is an exact match for Bancoposta.
    let parsed = quick_add::parse("15 pizza @bancoposta", Currency::Eur).unwrap();
    let resolved =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap();
    match resolved.command {
        Command::Expense(entry) => assert_eq!(entry.wallet_id, Some(f.bancoposta)),
        other => panic!("unexpected {other:?}"),
    }

    // "@ban" matches Bank and Bancoposta at the prefix tier: ambiguous.
    let parsed = quick_add::parse("15 pizza @ban", Currency::Eur).unwrap();
    let err =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap_err();
    match err {
        QuickAddError::AmbiguousName { name, candidates } => {
            assert_eq!(name, "ban");
            let mut candidates = candidates;
            candidates.sort();
            assert_eq!(
                candidates,
                vec!["Bancoposta".to_string(), "Bank".to_string()]
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn unknown_wallet_name_is_an_error() {
    let f = build_fixture();
    let now = at(T0);
    let parsed = quick_add::parse("15 pizza @revolut", Currency::Eur).unwrap();
    let err =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap_err();
    assert_eq!(
        err,
        QuickAddError::UnknownName {
            kind: "wallet".to_string(),
            name: "revolut".to_string()
        }
    );
}

#[test]
fn defaults_apply_when_no_marker_present() {
    let f = build_fixture();
    let now = at(T0);
    let defaults = QuickAddDefaults {
        wallet_id: Some(f.bank),
        flow_id: Some(f.vacanze),
    };
    let parsed = quick_add::parse("15 pizza", Currency::Eur).unwrap();
    let resolved =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &defaults)
            .unwrap();
    // The resolution reports the ids it settled on, so the app can keep them
    // as the next defaults without taking the command apart.
    assert_eq!(resolved.wallet_id, Some(f.bank));
    assert_eq!(resolved.flow_id, Some(f.vacanze));
    assert_eq!(resolved.from_id, None);
    assert_eq!(resolved.to_id, None);
    match resolved.command {
        Command::Expense(entry) => {
            assert_eq!(entry.wallet_id, Some(f.bank));
            assert_eq!(entry.flow_id, Some(f.vacanze));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn wallet_exact_match_resolves_to_cash() {
    let f = build_fixture();
    let now = at(T0);
    let parsed = quick_add::parse("15 pizza @cash", Currency::Eur).unwrap();
    let resolved =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap();
    assert_eq!(resolved.wallet_id, Some(f.cash));
    match resolved.command {
        Command::Expense(entry) => assert_eq!(entry.wallet_id, Some(f.cash)),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn unallocated_flow_resolves_by_its_display_name() {
    let f = build_fixture();
    let now = at(T0);
    let parsed = quick_add::parse("15 pizza >unallocated", Currency::Eur).unwrap();
    let resolved =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap();
    match resolved.command {
        Command::Expense(entry) => assert_eq!(entry.flow_id, Some(f.fx.unallocated)),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn yesterday_shifts_occurred_at_by_one_day_keeping_time_and_offset() {
    let f = build_fixture();
    let offset = FixedOffset::east_opt(3600).unwrap();
    let now = offset.from_utc_datetime(&Utc.timestamp_opt(T0, 0).unwrap().naive_utc());
    let parsed = quick_add::parse("15 pizza ieri", Currency::Eur).unwrap();
    let resolved =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap();
    match resolved.command {
        Command::Expense(entry) => {
            assert_eq!(
                entry.occurred_at.date_naive(),
                now.date_naive().pred_opt().unwrap()
            );
            assert_eq!(entry.occurred_at.time(), now.time());
            assert_eq!(entry.occurred_at.offset(), now.offset());
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn archived_wallets_are_not_matched() {
    let mut f = build_fixture();
    let now = at(T0);
    run(
        &mut f.fx.core,
        f.fx.vault,
        Command::ArchiveWallet { wallet_id: f.bank },
    );
    let parsed = quick_add::parse("15 pizza @bank", Currency::Eur).unwrap();
    let err =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap_err();
    assert_eq!(
        err,
        QuickAddError::UnknownName {
            kind: "wallet".to_string(),
            name: "bank".to_string()
        }
    );
}

#[test]
fn transfer_flow_execute_produces_expected_transaction() {
    let mut f = build_fixture();
    let now = at(T0 + 10);
    let parsed = quick_add::parse("tf>25 >vacanze >spesa weekend", Currency::Eur).unwrap();
    let resolved =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap();
    match &resolved.command {
        Command::TransferFlow {
            from_flow_id,
            to_flow_id,
            amount,
            ..
        } => {
            assert_eq!(*from_flow_id, f.vacanze);
            assert_eq!(*to_flow_id, f.spesa);
            assert_eq!(*amount, 2500);
        }
        other => panic!("unexpected {other:?}"),
    }

    assert_eq!(resolved.from_id, Some(f.vacanze));
    assert_eq!(resolved.to_id, Some(f.spesa));
    assert_eq!(resolved.wallet_id, None);
    assert_eq!(resolved.flow_id, None);

    try_run(&mut f.fx.core, f.fx.vault, resolved.command).unwrap();

    let txns = list(&f.fx.core, f.fx.vault, &all());
    let transfer = txns
        .iter()
        .find(|t| t.kind == TransactionKind::TransferFlow)
        .expect("transfer transaction present");
    assert_eq!(transfer.amount, 2500);
    assert_eq!(transfer.note.as_deref(), Some("weekend"));
}

#[test]
fn same_target_transfer_is_rejected() {
    let f = build_fixture();
    let now = at(T0);
    let parsed = quick_add::parse("tw>10 @cash @cash", Currency::Eur).unwrap();
    let err =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap_err();
    assert_eq!(err, QuickAddError::same_target());
}

#[test]
fn resolve_quick_add_error_converts_to_domain_error() {
    let f = build_fixture();
    let now = at(T0);
    let parsed = quick_add::parse("15 pizza @unknown", Currency::Eur).unwrap();
    let err =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap_err();
    let domain: DomainError = err.into();
    assert_eq!(domain.code(), "invalid_command");
}

// ---------------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------------

fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

fn day_month(day: u8, month: u8, today: NaiveDate) -> Result<NaiveDate, QuickAddError> {
    DateSpec::DayMonth { day, month }.resolve(today)
}

#[test]
fn a_day_and_month_is_the_nearest_occurrence() {
    // Early January looks back at the December just gone.
    assert_eq!(day_month(31, 12, ymd(2026, 1, 2)), Ok(ymd(2025, 12, 31)));
    // Today, tomorrow and the months just gone stay in this year.
    assert_eq!(day_month(23, 9, ymd(2026, 9, 23)), Ok(ymd(2026, 9, 23)));
    assert_eq!(day_month(24, 9, ymd(2026, 9, 23)), Ok(ymd(2026, 9, 24)));
    assert_eq!(day_month(12, 4, ymd(2026, 9, 23)), Ok(ymd(2026, 4, 12)));
    // Nearest means nearest: past half a year back, next year is closer
    // (12 March 2027 is 170 days away, 12 March 2026 195).
    assert_eq!(day_month(12, 3, ymd(2026, 9, 23)), Ok(ymd(2027, 3, 12)));
    // Late December looks ahead at the January to come.
    assert_eq!(day_month(2, 1, ymd(2026, 12, 31)), Ok(ymd(2027, 1, 2)));
}

#[test]
fn a_tie_between_two_years_goes_to_the_past() {
    // 2 July 2028 sits 183 days after 1 January 2028 and 183 days before
    // 1 January 2029 (2028 is a leap year).
    assert_eq!(day_month(1, 1, ymd(2028, 7, 2)), Ok(ymd(2028, 1, 1)));
}

#[test]
fn february_29_needs_the_nearest_february_to_have_it() {
    assert_eq!(day_month(29, 2, ymd(2028, 3, 5)), Ok(ymd(2028, 2, 29)));
    assert_eq!(day_month(29, 2, ymd(2027, 12, 31)), Ok(ymd(2028, 2, 29)));
    // The nearest February is 2027's: no jump to 2028, a year further on.
    assert!(matches!(
        day_month(29, 2, ymd(2027, 1, 10)),
        Err(QuickAddError::InvalidDate { .. })
    ));
    // A day the month never has is invalid whatever the year.
    assert!(matches!(
        day_month(31, 4, ymd(2026, 9, 23)),
        Err(QuickAddError::InvalidDate { .. })
    ));
    assert!(matches!(
        day_month(1, 13, ymd(2026, 9, 23)),
        Err(QuickAddError::InvalidDate { .. })
    ));
}

#[test]
fn a_two_digit_year_is_this_century_and_other_lengths_are_invalid() {
    let date = |input: &str| match quick_add::parse(input, Currency::Eur).unwrap() {
        QuickAdd::Entry { date, .. } => date,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(
        date("10 pizza 12/03/24"),
        Some(DateSpec::Date(ymd(2024, 3, 12)))
    );
    assert_eq!(
        date("10 pizza 1/3/99"),
        Some(DateSpec::Date(ymd(2099, 3, 1)))
    );
    assert_eq!(
        date("10 pizza 12/03/2024"),
        Some(DateSpec::Date(ymd(2024, 3, 12)))
    );
    for token in ["12/03/024", "12/03/2", "12/03/20245"] {
        assert!(
            matches!(
                quick_add::parse(&format!("10 pizza {token}"), Currency::Eur),
                Err(QuickAddError::InvalidDate { .. })
            ),
            "{token}"
        );
    }
}

#[test]
fn a_day_and_month_on_a_line_lands_in_the_nearest_year() {
    let f = build_fixture();
    let now = FixedOffset::east_opt(3600)
        .unwrap()
        .with_ymd_and_hms(2026, 1, 2, 9, 30, 0)
        .unwrap();
    let parsed = quick_add::parse("15 cenone 31/12", Currency::Eur).unwrap();
    let resolved =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap();
    match resolved.command {
        Command::Expense(entry) => {
            assert_eq!(entry.occurred_at.date_naive(), ymd(2025, 12, 31));
            assert_eq!(entry.occurred_at.time(), now.time());
        }
        other => panic!("unexpected {other:?}"),
    }
}
