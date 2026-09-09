//! Tests for the app-facing read queries in `usage.rs`: vault listing,
//! single-transaction lookup, recent picker usage, period totals and text
//! search.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::{DateTime, TimeZone, Utc};
use common::*;
use sparagne_core::{
    Command, CommandEnvelope, Currency, DomainError, FlowMode, TransactionFilter, TransactionKind,
};
use uuid::Uuid;

fn since_utc(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).unwrap()
}

// -- vaults -------------------------------------------------------------

#[test]
fn vaults_lists_two_vaults_sorted_by_name_with_currency_and_owner() {
    let mut core = sparagne_core::Core::open_in_memory().unwrap();
    core.execute(CommandEnvelope::create_vault("alice", "Zoo", Currency::Eur))
        .unwrap();
    core.execute(CommandEnvelope::create_vault("bob", "Alpha", Currency::Eur))
        .unwrap();

    let vaults = core.vaults().unwrap();
    assert_eq!(vaults.len(), 2);
    assert_eq!(vaults[0].name, "Alpha");
    assert_eq!(vaults[0].owner, "bob");
    assert_eq!(vaults[0].currency, Currency::Eur);
    assert_eq!(vaults[1].name, "Zoo");
    assert_eq!(vaults[1].owner, "alice");
}

// -- transaction ----------------------------------------------------------

#[test]
fn transaction_returns_legs_and_voided_flag_and_notfound_on_missing_or_wrong_vault() {
    let mut fx = setup();
    let income = run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(500, None, None, Some("Salary"), T0)),
    )
    .result_id
    .unwrap();

    let view = fx.core.transaction(fx.vault, income).unwrap();
    assert_eq!(view.amount, 500);
    assert!(!view.voided);
    assert_eq!(view.legs.len(), 2);

    run(
        &mut fx.core,
        fx.vault,
        Command::VoidTransaction {
            transaction_id: income,
        },
    );
    let voided = fx.core.transaction(fx.vault, income).unwrap();
    assert!(voided.voided);
    assert_eq!(voided.legs.len(), 2);

    let missing = fx.core.transaction(fx.vault, Uuid::now_v7());
    assert_eq!(
        missing.unwrap_err(),
        DomainError::NotFound("transaction".to_string())
    );

    let other_vault = fx
        .core
        .execute(CommandEnvelope::create_vault("bob", "Other", Currency::Eur))
        .unwrap()
        .result_id
        .unwrap();
    let wrong_vault = fx.core.transaction(other_vault, income);
    assert_eq!(
        wrong_vault.unwrap_err(),
        DomainError::NotFound("transaction".to_string())
    );
}

// -- recent_usage -----------------------------------------------------------

#[test]
fn recent_usage_orders_by_last_use_respects_limit_and_ignores_voided_and_transfers() {
    let mut fx = setup();
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0))
        .result_id
        .unwrap();
    let fun = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Fun", FlowMode::Unlimited, true, 0),
    )
    .result_id
    .unwrap();
    let trips = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Trips", FlowMode::Unlimited, true, 0),
    )
    .result_id
    .unwrap();

    // Salary: Cash / Unallocated, oldest.
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(1000, Some(fx.wallet), None, Some("Salary"), T0)),
    );
    // Food: Cash / Fun.
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(10, Some(fx.wallet), Some(fun), Some("Food"), T0 + 1)),
    );
    // Transport: Bank / Trips, most recent non-voided non-transfer use.
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(
            20,
            Some(bank),
            Some(trips),
            Some("Transport"),
            T0 + 2,
        )),
    );
    // Old: Cash / Fun, newer than Food but voided, so must not count.
    let old = run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(5, Some(fx.wallet), Some(fun), Some("Old"), T0 + 3)),
    )
    .result_id
    .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::VoidTransaction {
            transaction_id: old,
        },
    );
    // Newest event overall, but a transfer: must not count for Cash or Bank.
    run(
        &mut fx.core,
        fx.vault,
        Command::TransferWallet {
            amount: 5,
            from_wallet_id: fx.wallet,
            to_wallet_id: bank,
            note: None,
            occurred_at: at(T0 + 4),
        },
    );

    let cats = fx.core.categories(fx.vault, false).unwrap();
    let cat_id = |name: &str| cats.iter().find(|c| c.name == name).unwrap().id;

    let usage = fx.core.recent_usage(fx.vault, since_utc(T0), 10).unwrap();
    assert_eq!(
        usage.categories,
        vec![cat_id("Transport"), cat_id("Food"), cat_id("Salary")]
    );
    assert_eq!(usage.wallets, vec![bank, fx.wallet]);
    assert_eq!(usage.flows, vec![trips, fun]);

    let limited = fx.core.recent_usage(fx.vault, since_utc(T0), 1).unwrap();
    assert_eq!(limited.categories, vec![cat_id("Transport")]);
    assert_eq!(limited.wallets, vec![bank]);
    assert_eq!(limited.flows, vec![trips]);

    // `since` cuts off everything before Transport's occurrence.
    let filtered = fx
        .core
        .recent_usage(fx.vault, since_utc(T0 + 2), 10)
        .unwrap();
    assert_eq!(filtered.categories, vec![cat_id("Transport")]);
    assert_eq!(filtered.wallets, vec![bank]);
    assert_eq!(filtered.flows, vec![trips]);
}

#[test]
fn recent_usage_excludes_system_category_even_when_most_recent() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(1, Some(fx.wallet), None, Some("Food"), T0)),
    );
    // Blank category -> system "Uncategorized", most recent of the two.
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(1, Some(fx.wallet), None, None, T0 + 1)),
    );

    let cats = fx.core.categories(fx.vault, false).unwrap();
    let food_id = cats.iter().find(|c| c.name == "Food").unwrap().id;

    let usage = fx.core.recent_usage(fx.vault, since_utc(T0), 10).unwrap();
    assert_eq!(usage.categories, vec![food_id]);
}

#[test]
fn recent_usage_excludes_archived_wallets_and_flows() {
    let mut fx = setup();
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0))
        .result_id
        .unwrap();
    let fun = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Fun", FlowMode::Unlimited, false, 0),
    )
    .result_id
    .unwrap();

    // A live wallet/flow used early, so it only wins because the more
    // recent users below get archived.
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10, Some(bank), Some(fun), None, T0)),
    );

    // Gone: used more recently than Bank, then archived (zero balance).
    let gone = run(&mut fx.core, fx.vault, wallet_cmd("Gone", 0))
        .result_id
        .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(100, Some(gone), None, None, T0 + 1)),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(100, Some(gone), None, None, T0 + 2)),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveWallet { wallet_id: gone },
    );

    // Ghost: used more recently than Fun (via the still-live Cash wallet),
    // then archived (zero balance).
    let ghost = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Ghost", FlowMode::Unlimited, false, 0),
    )
    .result_id
    .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(50, Some(fx.wallet), Some(ghost), None, T0 + 3)),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(50, Some(fx.wallet), Some(ghost), None, T0 + 4)),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveFlow { flow_id: ghost },
    );

    let usage = fx.core.recent_usage(fx.vault, since_utc(T0), 10).unwrap();
    // Cash is live and was used (via Ghost) more recently than Bank.
    assert_eq!(usage.wallets, vec![fx.wallet, bank]);
    // Ghost is archived and excluded even though it is the most recent flow use.
    assert_eq!(usage.flows, vec![fun]);
}

// -- period_totals ------------------------------------------------------

#[test]
fn period_totals_treats_refunds_as_expense_reduction() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(1000, None, None, Some("salary"), T0)),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(300, None, None, Some("food"), T0 + 1)),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::Refund(entry(50, None, None, Some("food"), T0 + 2)),
    );

    let totals = fx
        .core
        .period_totals(fx.vault, since_utc(T0), since_utc(T0 + 100))
        .unwrap();
    assert_eq!(totals.income, 1000);
    assert_eq!(totals.expense, 300);
    assert_eq!(totals.refund, 50);
    assert_eq!(totals.net_expense, 250);
}

#[test]
fn period_totals_ignores_transfers_and_voided() {
    let mut fx = setup();
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0))
        .result_id
        .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(1000, Some(fx.wallet), None, None, T0)),
    );
    let expense = run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(200, Some(fx.wallet), None, None, T0 + 1)),
    )
    .result_id
    .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::VoidTransaction {
            transaction_id: expense,
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::TransferWallet {
            amount: 100,
            from_wallet_id: fx.wallet,
            to_wallet_id: bank,
            note: None,
            occurred_at: at(T0 + 2),
        },
    );

    let totals = fx
        .core
        .period_totals(fx.vault, since_utc(T0), since_utc(T0 + 10))
        .unwrap();
    assert_eq!(totals.income, 1000);
    assert_eq!(totals.expense, 0);
    assert_eq!(totals.refund, 0);
    assert_eq!(totals.net_expense, 0);
}

#[test]
fn period_totals_bounds_are_from_inclusive_to_exclusive() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10, None, None, None, T0)),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(20, None, None, None, T0 + 5)),
    );

    let totals = fx
        .core
        .period_totals(fx.vault, since_utc(T0), since_utc(T0 + 5))
        .unwrap();
    assert_eq!(totals.income, 10);
}

#[test]
fn period_totals_rejects_invalid_range() {
    let fx = setup();
    let err = fx
        .core
        .period_totals(fx.vault, since_utc(T0 + 1), since_utc(T0))
        .unwrap_err();
    assert_eq!(
        err,
        DomainError::InvalidCommand("invalid range: from must be < to".to_string())
    );
    let err_eq = fx
        .core
        .period_totals(fx.vault, since_utc(T0), since_utc(T0))
        .unwrap_err();
    assert_eq!(
        err_eq,
        DomainError::InvalidCommand("invalid range: from must be < to".to_string())
    );
}

// -- text search --------------------------------------------------------

#[test]
fn text_search_matches_note_and_category_case_insensitively() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(1, None, None, Some("Groceries"), T0)),
    );
    let mut with_note = entry(2, None, None, Some("Food"), T0 + 1);
    with_note.note = Some("Pizza night".to_string());
    run(&mut fx.core, fx.vault, Command::Expense(with_note));
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(3, None, None, Some("Other"), T0 + 2)),
    );

    let by_category = list(
        &fx.core,
        fx.vault,
        &TransactionFilter {
            text: Some("groc".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(by_category.len(), 1);
    assert_eq!(by_category[0].category, "Groceries");

    let by_note = list(
        &fx.core,
        fx.vault,
        &TransactionFilter {
            text: Some("PIZZA".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(by_note.len(), 1);
    assert_eq!(by_note[0].note.as_deref(), Some("Pizza night"));
}

#[test]
fn text_search_percent_in_query_is_literal() {
    let mut fx = setup();
    let mut a = entry(1, None, None, None, T0);
    a.note = Some("50% off".to_string());
    run(&mut fx.core, fx.vault, Command::Expense(a));
    let mut b = entry(2, None, None, None, T0 + 1);
    b.note = Some("50 off".to_string());
    run(&mut fx.core, fx.vault, Command::Expense(b));

    let hits = list(
        &fx.core,
        fx.vault,
        &TransactionFilter {
            text: Some("50%".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].note.as_deref(), Some("50% off"));
}

#[test]
fn text_search_combines_with_kinds_filter() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(1, None, None, Some("Coffee"), T0)),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(2, None, None, Some("Coffee"), T0 + 1)),
    );

    let hits = list(
        &fx.core,
        fx.vault,
        &TransactionFilter {
            text: Some("coffee".to_string()),
            kinds: Some(vec![TransactionKind::Expense]),
            ..Default::default()
        },
    );
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, TransactionKind::Expense);
}

#[test]
fn text_search_works_with_cursor_pagination() {
    let mut fx = setup();
    let mut ids = Vec::new();
    for i in 0..3 {
        let mut e = entry(1, None, None, None, T0 + i);
        e.note = Some("match me".to_string());
        ids.push(
            run(&mut fx.core, fx.vault, Command::Expense(e))
                .result_id
                .unwrap(),
        );
    }
    // Does not match; must not appear and must not break pagination.
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(1, None, None, None, T0 + 10)),
    );

    let filter = TransactionFilter {
        text: Some("match".to_string()),
        ..Default::default()
    };
    let mut seen = std::collections::HashSet::new();
    let mut cursor: Option<String> = None;
    loop {
        let page = fx
            .core
            .list_transactions(fx.vault, &filter, 1, cursor.as_deref())
            .unwrap();
        assert!(page.items.len() <= 1);
        seen.extend(page.items.iter().map(|t| t.id));
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    assert_eq!(seen.len(), 3);
    for id in ids {
        assert!(seen.contains(&id));
    }
}
