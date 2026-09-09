//! Acceptance tests for the v2 core, ported from the v1 engine suite.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::{TimeZone, Utc};
use common::*;
use sparagne_core::{
    Command, CommandEnvelope, Core, Currency, DomainError, FlowMode, TransactionFilter,
    TransactionKind, replay,
};
use uuid::Uuid;

#[test]
fn create_vault_creates_unallocated_and_system_categories() {
    let fx = setup();
    let s = fx.core.snapshot(fx.vault).unwrap();
    assert_eq!(s.name, "Main");
    assert_eq!(s.currency, Currency::Eur);
    assert_eq!(s.flows.len(), 1);
    assert!(s.flows[0].is_unallocated);
    assert_eq!(s.flows[0].id, s.unallocated_flow_id);
    let cats = fx.core.categories(fx.vault, false).unwrap();
    let names: Vec<_> = cats
        .iter()
        .map(|c| (c.name.as_str(), c.is_system))
        .collect();
    assert_eq!(names, vec![("Opening", true), ("Uncategorized", true)]);
}

#[test]
fn create_vault_id_is_the_command_id() {
    let mut core = Core::open_in_memory().unwrap();
    let env = CommandEnvelope::create_vault("alice", "Main", Currency::Eur);
    let id = env.id;
    let receipt = core.execute(env).unwrap();
    assert_eq!(receipt.result_id, Some(id));
    assert_eq!(receipt.seq, 1);

    let bad = CommandEnvelope::new(
        Uuid::now_v7(),
        "alice",
        Command::CreateVault {
            name: "Other".to_string(),
            currency: Currency::Eur,
        },
    );
    assert!(matches!(
        core.execute(bad),
        Err(DomainError::InvalidCommand(_))
    ));

    let dup = CommandEnvelope::create_vault("alice", " main ", Currency::Eur);
    assert_eq!(
        core.execute(dup).unwrap_err(),
        DomainError::AlreadyExists("main".to_string())
    );
}

#[test]
fn income_expense_void_reverts_balances() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(1000, None, None, Some("Salary"), T0)),
    );
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 1000);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 1000);

    let expense = run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(300, None, None, Some("Food"), T0 + 1)),
    );
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 700);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 700);

    let tx_id = expense.result_id.unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::VoidTransaction {
            transaction_id: tx_id,
        },
    );
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 1000);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 1000);

    let again = try_run(
        &mut fx.core,
        fx.vault,
        Command::VoidTransaction {
            transaction_id: tx_id,
        },
    );
    assert!(matches!(again, Err(DomainError::InvalidCommand(_))));
    let voided = list(&fx.core, fx.vault, &all());
    assert!(voided.iter().any(|t| t.id == tx_id && t.voided));
}

#[test]
fn refund_increases_balances_and_amount_is_positive() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Refund(entry(250, None, None, None, T0)),
    );
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 250);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 250);
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::Refund(entry(0, None, None, None, T0)),
    )
    .unwrap_err();
    assert!(matches!(err, DomainError::InvalidAmount(_)));
}

#[test]
fn transfer_wallet_does_not_touch_flows() {
    let mut fx = setup();
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0))
        .result_id
        .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(1000, Some(fx.wallet), None, None, T0)),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::TransferWallet {
            amount: 400,
            from_wallet_id: fx.wallet,
            to_wallet_id: bank,
            note: Some("  ".to_string()),
            occurred_at: at(T0 + 1),
        },
    );
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 600);
    assert_eq!(wallet_balance(&fx.core, fx.vault, bank), 400);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 1000);

    let same = try_run(
        &mut fx.core,
        fx.vault,
        Command::TransferWallet {
            amount: 1,
            from_wallet_id: bank,
            to_wallet_id: bank,
            note: None,
            occurred_at: at(T0),
        },
    );
    assert!(matches!(same, Err(DomainError::InvalidCommand(_))));
}

#[test]
fn flow_caps_are_enforced_and_income_capped_counts_transfers_in() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(1000, None, None, None, T0)),
    );
    let vacanze = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Vacanze", FlowMode::IncomeCapped { cap: 500 }, false, 0),
    )
    .result_id
    .unwrap();
    let transfer = |amount| Command::TransferFlow {
        amount,
        from_flow_id: fx.unallocated,
        to_flow_id: vacanze,
        note: None,
        occurred_at: at(T0 + 1),
    };
    run(&mut fx.core, fx.vault, transfer(300));
    assert_eq!(
        try_run(&mut fx.core, fx.vault, transfer(300)).unwrap_err(),
        DomainError::MaxBalanceReached("Vacanze".to_string())
    );
    assert_eq!(flow_balance(&fx.core, fx.vault, vacanze), 300);

    let casa = try_run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Casa", FlowMode::NetCapped { cap: 200 }, false, 250),
    );
    assert_eq!(
        casa.unwrap_err(),
        DomainError::MaxBalanceReached("Casa".to_string())
    );
    assert!(
        fx.core
            .snapshot(fx.vault)
            .unwrap()
            .flows
            .iter()
            .all(|f| f.name != "Casa")
    );

    let bad_cap = try_run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Zero", FlowMode::NetCapped { cap: 0 }, false, 0),
    );
    assert!(matches!(bad_cap, Err(DomainError::InvalidFlow(_))));
}

#[test]
fn expense_on_flow_without_balance_fails_atomically() {
    let mut fx = setup();
    let fun = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Fun", FlowMode::Unlimited, false, 0),
    )
    .result_id
    .unwrap();
    let before = fx.core.commands_since(fx.vault, 0).unwrap().len();
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(1, None, Some(fun), None, T0)),
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InsufficientFunds("Fun".to_string()));
    assert!(list(&fx.core, fx.vault, &all()).is_empty());
    assert_eq!(fx.core.commands_since(fx.vault, 0).unwrap().len(), before);
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 0);
}

#[test]
fn void_is_never_blocked_by_flow_rules() {
    let mut fx = setup();
    let fun = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Fun", FlowMode::NetCapped { cap: 1000 }, false, 0),
    )
    .result_id
    .unwrap();
    let income = run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(500, None, Some(fun), None, T0)),
    )
    .result_id
    .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(200, None, Some(fun), None, T0 + 1)),
    );
    assert_eq!(flow_balance(&fx.core, fx.vault, fun), 300);

    run(
        &mut fx.core,
        fx.vault,
        Command::VoidTransaction {
            transaction_id: income,
        },
    );
    assert_eq!(flow_balance(&fx.core, fx.vault, fun), -200);
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), -200);
}

#[test]
fn same_command_id_is_applied_once() {
    let mut fx = setup();
    let env = CommandEnvelope::new(
        fx.vault,
        "alice",
        Command::Income(entry(100, None, None, None, T0)),
    );
    let first = fx.core.execute(env.clone()).unwrap();
    let second = fx.core.execute(env).unwrap();
    assert!(!first.deduplicated);
    assert!(second.deduplicated);
    assert_eq!(first.seq, second.seq);
    assert_eq!(first.result_id, second.result_id);
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 100);
    assert_eq!(
        list(&fx.core, fx.vault, &TransactionFilter::default()).len(),
        1
    );
}

#[test]
fn list_defaults_filters_and_cursor_pagination() {
    let mut fx = setup();
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0))
        .result_id
        .unwrap();
    let mut ids = Vec::new();
    for i in 0..5 {
        let cmd = if i % 2 == 0 {
            Command::Income(entry(100 + i, Some(fx.wallet), None, Some("A"), T0 + i))
        } else {
            Command::Expense(entry(10, Some(fx.wallet), None, Some("B"), T0 + i))
        };
        ids.push(run(&mut fx.core, fx.vault, cmd).result_id.unwrap());
    }
    run(
        &mut fx.core,
        fx.vault,
        Command::TransferWallet {
            amount: 5,
            from_wallet_id: fx.wallet,
            to_wallet_id: bank,
            note: None,
            occurred_at: at(T0 + 10),
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::VoidTransaction {
            transaction_id: ids[0],
        },
    );

    assert_eq!(
        list(&fx.core, fx.vault, &TransactionFilter::default()).len(),
        4
    );
    assert_eq!(
        list(
            &fx.core,
            fx.vault,
            &TransactionFilter {
                include_transfers: true,
                ..Default::default()
            }
        )
        .len(),
        5
    );
    assert_eq!(
        list(
            &fx.core,
            fx.vault,
            &TransactionFilter {
                include_voided: true,
                ..Default::default()
            }
        )
        .len(),
        5
    );
    assert_eq!(list(&fx.core, fx.vault, &all()).len(), 6);

    let range = TransactionFilter {
        from: Some(Utc.timestamp_opt(T0 + 1, 0).unwrap()),
        to: Some(Utc.timestamp_opt(T0 + 3, 0).unwrap()),
        include_voided: true,
        ..Default::default()
    };
    let in_range = list(&fx.core, fx.vault, &range);
    assert_eq!(in_range.len(), 2);
    assert_eq!(in_range[0].occurred_at.timestamp(), T0 + 2);

    let only_expenses = TransactionFilter {
        kinds: Some(vec![TransactionKind::Expense]),
        ..Default::default()
    };
    assert_eq!(list(&fx.core, fx.vault, &only_expenses).len(), 2);

    let by_wallet = TransactionFilter {
        wallet_id: Some(bank),
        include_transfers: true,
        ..Default::default()
    };
    assert_eq!(list(&fx.core, fx.vault, &by_wallet).len(), 1);

    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let page = fx
            .core
            .list_transactions(fx.vault, &all(), 2, cursor.as_deref())
            .unwrap();
        assert!(page.items.len() <= 2);
        seen.extend(page.items.iter().map(|t| t.id));
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    assert_eq!(seen.len(), 6);
    let unique: std::collections::HashSet<_> = seen.iter().collect();
    assert_eq!(unique.len(), 6);

    let bad = fx.core.list_transactions(fx.vault, &all(), 2, Some("nope"));
    assert!(matches!(bad, Err(DomainError::InvalidCursor(_))));
    let empty_kinds = TransactionFilter {
        kinds: Some(vec![]),
        ..Default::default()
    };
    assert!(
        fx.core
            .list_transactions(fx.vault, &empty_kinds, 2, None)
            .is_err()
    );
}

#[test]
fn names_are_trimmed_and_unique_case_insensitive() {
    let mut fx = setup();
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("  Bank ", 0))
        .result_id
        .unwrap();
    let s = fx.core.snapshot(fx.vault).unwrap();
    assert_eq!(
        s.wallets.iter().find(|w| w.id == bank).unwrap().name,
        "Bank"
    );
    assert_eq!(
        try_run(&mut fx.core, fx.vault, wallet_cmd("bank", 0)).unwrap_err(),
        DomainError::AlreadyExists("bank".to_string())
    );
    assert!(matches!(
        try_run(&mut fx.core, fx.vault, wallet_cmd("   ", 0)),
        Err(DomainError::InvalidName(_))
    ));
    assert!(matches!(
        try_run(
            &mut fx.core,
            fx.vault,
            flow_cmd("Unallocated", FlowMode::Unlimited, false, 0)
        ),
        Err(DomainError::InvalidFlow(_))
    ));
    run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Vacanze", FlowMode::Unlimited, false, 0),
    );
    assert!(matches!(
        try_run(
            &mut fx.core,
            fx.vault,
            flow_cmd("VACANZE", FlowMode::Unlimited, false, 0)
        ),
        Err(DomainError::AlreadyExists(_))
    ));
}

#[test]
fn category_free_text_resolves_and_blank_is_uncategorized() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10, None, None, None, T0)),
    );
    let a = run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(1, None, None, Some("Spesa"), T0 + 1)),
    )
    .result_id
    .unwrap();
    let b = run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(1, None, None, Some("  spesa!!! "), T0 + 2)),
    )
    .result_id
    .unwrap();
    let c = run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(1, None, None, Some("   "), T0 + 3)),
    )
    .result_id
    .unwrap();
    let rows = list(&fx.core, fx.vault, &all());
    let find = |id| rows.iter().find(|t| t.id == id).unwrap();
    assert_eq!(find(a).category, "Spesa");
    assert_eq!(find(a).category_id, find(b).category_id);
    assert_eq!(find(c).category, "Uncategorized");
    let cats = fx.core.categories(fx.vault, false).unwrap();
    assert_eq!(cats.iter().filter(|c| !c.is_system).count(), 1);

    run(
        &mut fx.core,
        fx.vault,
        Command::CreateCategory {
            name: "Auto-Moto".to_string(),
        },
    );
    assert!(matches!(
        try_run(
            &mut fx.core,
            fx.vault,
            Command::CreateCategory {
                name: "auto moto".to_string()
            }
        ),
        Err(DomainError::AlreadyExists(_))
    ));
    assert!(matches!(
        try_run(
            &mut fx.core,
            fx.vault,
            Command::CreateCategory {
                name: "Uncategorized".to_string()
            }
        ),
        Err(DomainError::InvalidName(_))
    ));
}

#[test]
fn opening_balances_are_real_transactions() {
    let mut fx = setup();
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("Bank", 1234))
        .result_id
        .unwrap();
    let card = run(&mut fx.core, fx.vault, wallet_cmd("Card", -50))
        .result_id
        .unwrap();
    assert_eq!(wallet_balance(&fx.core, fx.vault, bank), 1234);
    assert_eq!(wallet_balance(&fx.core, fx.vault, card), -50);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 1184);

    let rows = list(&fx.core, fx.vault, &all());
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|t| t.category == "Opening"));
    assert!(
        rows.iter()
            .any(|t| t.kind == TransactionKind::Income && t.amount == 1234)
    );
    assert!(
        rows.iter()
            .any(|t| t.kind == TransactionKind::Expense && t.amount == 50)
    );

    let goal = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Goal", FlowMode::Unlimited, false, 200),
    )
    .result_id
    .unwrap();
    assert_eq!(flow_balance(&fx.core, fx.vault, goal), 200);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 984);
    assert!(matches!(
        try_run(
            &mut fx.core,
            fx.vault,
            flow_cmd("Neg", FlowMode::Unlimited, true, -1)
        ),
        Err(DomainError::InvalidAmount(_))
    ));
}

#[test]
fn wallet_defaults_to_the_only_active_wallet() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10, None, None, None, T0)),
    );
    run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0));
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10, None, None, None, T0)),
    )
    .unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));
    let missing = try_run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10, Some(Uuid::now_v7()), None, None, T0)),
    );
    assert_eq!(
        missing.unwrap_err(),
        DomainError::NotFound("wallet".to_string())
    );
}

#[test]
fn wallets_and_flows_always_balance_and_replay_rebuilds_the_projection() {
    let mut fx = setup();
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("Bank", 5000))
        .result_id
        .unwrap();
    let vacanze = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Vacanze", FlowMode::NetCapped { cap: 100_000 }, false, 1000),
    )
    .result_id
    .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(
            2000,
            Some(fx.wallet),
            None,
            Some("Stipendio"),
            T0 + 1,
        )),
    );
    let e = run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(300, Some(bank), Some(vacanze), Some("Treno"), T0 + 2)),
    )
    .result_id
    .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::Refund(entry(50, Some(bank), Some(vacanze), Some("treno"), T0 + 3)),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::TransferWallet {
            amount: 700,
            from_wallet_id: bank,
            to_wallet_id: fx.wallet,
            note: Some("prelievo".to_string()),
            occurred_at: at(T0 + 4),
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::TransferFlow {
            amount: 100,
            from_flow_id: vacanze,
            to_flow_id: fx.unallocated,
            note: None,
            occurred_at: at(T0 + 5),
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::VoidTransaction { transaction_id: e },
    );

    let (wallets, flows) = balances(&fx.core, fx.vault);
    let sum_w: i64 = wallets.iter().map(|(_, b)| b).sum();
    let sum_f: i64 = flows.iter().map(|(_, b)| b).sum();
    assert_eq!(sum_w, sum_f);
    assert_eq!(sum_w, 5000 + 2000 - 300 + 50 + 300);

    let log = fx.core.commands_since(fx.vault, 0).unwrap();
    assert_eq!(log.len(), 10);
    assert_eq!(
        log.iter().map(|r| r.seq).collect::<Vec<_>>(),
        (1..=10).collect::<Vec<_>>()
    );

    let mut fresh = Core::open_in_memory().unwrap();
    replay(&log, &mut fresh).unwrap();
    assert_eq!(
        fresh.snapshot(fx.vault).unwrap(),
        fx.core.snapshot(fx.vault).unwrap()
    );
    assert_eq!(
        list(&fresh, fx.vault, &all()),
        list(&fx.core, fx.vault, &all())
    );
    assert_eq!(
        fresh.categories(fx.vault, true).unwrap(),
        fx.core.categories(fx.vault, true).unwrap()
    );
    let replayed = fresh.commands_since(fx.vault, 0).unwrap();
    assert_eq!(
        replayed
            .iter()
            .map(|r| (r.seq, r.envelope.id, r.result_id))
            .collect::<Vec<_>>(),
        log.iter()
            .map(|r| (r.seq, r.envelope.id, r.result_id))
            .collect::<Vec<_>>()
    );
}

#[test]
fn database_file_survives_reopen() {
    let dir = std::env::temp_dir().join(format!("sparagne-core-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.sqlite3");
    let vault = {
        let mut core = Core::open(&path).unwrap();
        let vault = core
            .execute(CommandEnvelope::create_vault(
                "alice",
                "Main",
                Currency::Eur,
            ))
            .unwrap()
            .result_id
            .unwrap();
        run(&mut core, vault, wallet_cmd("Cash", 700));
        vault
    };
    let core = Core::open(&path).unwrap();
    assert_eq!(core.snapshot(vault).unwrap().wallets[0].balance, 700);
    std::fs::remove_dir_all(&dir).unwrap();
}
