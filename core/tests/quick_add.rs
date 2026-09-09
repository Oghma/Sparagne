//! Integration tests for quick-add resolution against a vault.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::{FixedOffset, TimeZone, Utc};
use common::*;
use sparagne_core::quick_add::{self, QuickAddDefaults, QuickAddError};
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
    let cmd =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap();
    match cmd {
        Command::Expense(entry) => assert_eq!(entry.wallet_id, Some(f.bank)),
        other => panic!("unexpected {other:?}"),
    }

    // "@bancoposta" is an exact match for Bancoposta.
    let parsed = quick_add::parse("15 pizza @bancoposta", Currency::Eur).unwrap();
    let cmd =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap();
    match cmd {
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
            kind: "wallet",
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
    let cmd =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &defaults)
            .unwrap();
    match cmd {
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
    let cmd =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap();
    match cmd {
        Command::Expense(entry) => assert_eq!(entry.wallet_id, Some(f.cash)),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn unallocated_flow_resolves_by_its_display_name() {
    let f = build_fixture();
    let now = at(T0);
    let parsed = quick_add::parse("15 pizza >unallocated", Currency::Eur).unwrap();
    let cmd =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap();
    match cmd {
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
    let cmd =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap();
    match cmd {
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
            kind: "wallet",
            name: "bank".to_string()
        }
    );
}

#[test]
fn transfer_flow_execute_produces_expected_transaction() {
    let mut f = build_fixture();
    let now = at(T0 + 10);
    let parsed = quick_add::parse("tf>25 >vacanze >spesa weekend", Currency::Eur).unwrap();
    let cmd =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, now, &no_defaults())
            .unwrap();
    match &cmd {
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

    try_run(&mut f.fx.core, f.fx.vault, cmd).unwrap();

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
    assert_eq!(err, QuickAddError::SameTarget);
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
