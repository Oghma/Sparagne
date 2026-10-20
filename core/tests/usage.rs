//! Tests for the app-facing read queries in `usage.rs`: vault listing,
//! single-transaction lookup, recent picker usage and text search.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use sparagne_core::{Command, Currency, DomainError, FlowMode, TransactionFilter, TransactionKind};
use uuid::Uuid;

#[test]
fn vaults_lists_two_vaults_sorted_by_name_with_currency_and_owner() {
    let mut core = sparagne_core::Core::open_in_memory().unwrap();
    create_vault(&mut core, "alice", "Zoo");
    create_vault(&mut core, "bob", "Alpha");

    let vaults = core.vaults().unwrap();
    assert_eq!(vaults.len(), 2);
    assert_eq!(vaults[0].name, "Alpha");
    assert_eq!(vaults[0].owner, "bob");
    assert_eq!(vaults[0].currency, Currency::Eur);
    assert_eq!(vaults[1].name, "Zoo");
    assert_eq!(vaults[1].owner, "alice");
}

#[test]
fn transaction_returns_legs_and_voided_flag_and_notfound_on_missing_or_wrong_vault() {
    let mut fx = setup();
    let income = fx.id(Command::Income(entry(500, None, None, Some("Salary"), T0)));

    let view = fx.core.transaction(fx.vault, income).unwrap();
    assert_eq!(view.amount, 500);
    assert!(!view.voided);
    assert_eq!(view.legs.len(), 2);
    // The legs are also reported already sorted out by kind.
    assert_eq!(view.wallet_id, Some(fx.wallet));
    assert_eq!(view.flow_id, Some(fx.unallocated));
    assert_eq!(view.from_id, None);
    assert_eq!(view.to_id, None);
    assert!(!view.category_is_system);

    fx.run(Command::VoidTransaction {
        transaction_id: income,
    });
    let voided = fx.core.transaction(fx.vault, income).unwrap();
    assert!(voided.voided);
    assert_eq!(voided.legs.len(), 2);

    let missing = fx.core.transaction(fx.vault, Uuid::now_v7());
    assert_eq!(
        missing.unwrap_err(),
        DomainError::NotFound("transaction".to_string())
    );

    let other_vault = create_vault(&mut fx.core, "bob", "Other");
    let wrong_vault = fx.core.transaction(other_vault, income);
    assert_eq!(
        wrong_vault.unwrap_err(),
        DomainError::NotFound("transaction".to_string())
    );
}

#[test]
fn listed_rows_carry_their_shape() {
    let mut fx = setup();
    let w = Some(fx.wallet);
    let bank = fx.id(wallet_cmd("Bank", 1000));
    let expense = fx.id(Command::Expense(entry(100, w, None, Some("Food"), T0 + 1)));
    let transfer = fx.id(Command::TransferWallet {
        amount: 50,
        from_wallet_id: fx.wallet,
        to_wallet_id: bank,
        note: None,
        occurred_at: at(T0 + 2),
    });

    let rows = list(&fx.core, fx.vault, &all());
    let find = |id: Uuid| rows.iter().find(|t| t.id == id).unwrap().clone();

    let row = find(expense);
    assert_eq!(row.wallet_id, Some(fx.wallet));
    assert_eq!(row.flow_id, Some(fx.unallocated));
    assert_eq!(row.from_id, None);
    assert_eq!(row.to_id, None);
    assert!(!row.category_is_system);

    // Source is the negative leg, destination the positive one.
    let row = find(transfer);
    assert_eq!(row.from_id, Some(fx.wallet));
    assert_eq!(row.to_id, Some(bank));
    assert_eq!(row.wallet_id, None);
    assert_eq!(row.flow_id, None);

    // The opening balance sits on the system category `Opening`.
    let opening = rows
        .iter()
        .find(|t| t.kind == TransactionKind::Income)
        .unwrap();
    assert!(opening.category_is_system);
    assert_eq!(opening.category, "Opening");
}

#[test]
fn recent_usage_orders_by_last_use_respects_limit_and_ignores_voided_and_transfers() {
    let mut fx = setup();
    let w = Some(fx.wallet);
    let bank = fx.id(wallet_cmd("Bank", 0));
    let fun = fx.id(flow_cmd("Fun", FlowMode::Unlimited, true, 0));
    let trips = fx.id(flow_cmd("Trips", FlowMode::Unlimited, true, 0));

    // Salary: Cash / Unallocated, oldest.
    fx.run(Command::Income(entry(1000, w, None, Some("Salary"), T0)));
    // Food: Cash / Fun.
    let cmd = Command::Expense(entry(10, w, Some(fun), Some("Food"), T0 + 1));
    fx.run(cmd);
    // Transport: Bank / Trips, most recent non-voided non-transfer use.
    let e = entry(20, Some(bank), Some(trips), Some("Transport"), T0 + 2);
    fx.run(Command::Expense(e));
    // Old: Cash / Fun, newer than Food but voided, so must not count.
    let cmd = Command::Expense(entry(5, w, Some(fun), Some("Old"), T0 + 3));
    let old = fx.id(cmd);
    fx.run(Command::VoidTransaction {
        transaction_id: old,
    });
    // Newest event overall, but a transfer: must not count for Cash or Bank.
    fx.run(Command::TransferWallet {
        amount: 5,
        from_wallet_id: fx.wallet,
        to_wallet_id: bank,
        note: None,
        occurred_at: at(T0 + 4),
    });

    let cat_id = |name: &str| category_id(&fx, name);
    let usage = fx.core.recent_usage(fx.vault, utc(T0), 10).unwrap();
    assert_eq!(
        usage.categories,
        vec![cat_id("Transport"), cat_id("Food"), cat_id("Salary")]
    );

    let limited = fx.core.recent_usage(fx.vault, utc(T0), 1).unwrap();
    assert_eq!(limited.categories, vec![cat_id("Transport")]);

    // `since` cuts off everything before Transport's occurrence.
    let filtered = fx.core.recent_usage(fx.vault, utc(T0 + 2), 10).unwrap();
    assert_eq!(filtered.categories, vec![cat_id("Transport")]);
}

#[test]
fn recent_usage_excludes_system_category_even_when_most_recent() {
    let mut fx = setup();
    let w = Some(fx.wallet);
    fx.run(Command::Expense(entry(1, w, None, Some("Food"), T0)));
    // Blank category -> system "Uncategorized", most recent of the two.
    fx.run(Command::Expense(entry(1, w, None, None, T0 + 1)));

    let usage = fx.core.recent_usage(fx.vault, utc(T0), 10).unwrap();
    assert_eq!(usage.categories, vec![category_id(&fx, "Food")]);
}

#[test]
fn text_search_matches_note_and_category_case_insensitively() {
    let mut fx = setup();
    let cmd = Command::Expense(entry(1, None, None, Some("Groceries"), T0));
    fx.run(cmd);
    let mut with_note = entry(2, None, None, Some("Food"), T0 + 1);
    with_note.note = Some("Pizza night".to_string());
    fx.run(Command::Expense(with_note));
    let cmd = Command::Expense(entry(3, None, None, Some("Other"), T0 + 2));
    fx.run(cmd);

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
    fx.run(Command::Expense(a));
    let mut b = entry(2, None, None, None, T0 + 1);
    b.note = Some("50 off".to_string());
    fx.run(Command::Expense(b));

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
    fx.run(Command::Income(entry(1, None, None, Some("Coffee"), T0)));
    let cmd = Command::Expense(entry(2, None, None, Some("Coffee"), T0 + 1));
    fx.run(cmd);

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
        ids.push(fx.id(Command::Expense(e)));
    }
    // Does not match; must not appear and must not break pagination.
    fx.run(Command::Expense(entry(1, None, None, None, T0 + 10)));

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
