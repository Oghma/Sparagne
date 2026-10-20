//! Acceptance tests for the engine: entries, transfers, caps, listing and replay.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use sparagne_core::{
    Command, CommandEnvelope, Core, Currency, DomainError, FlowMode, TransactionFilter,
    TransactionKind,
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
fn create_vault_id_is_the_command_id_and_the_name_may_repeat() {
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

    // The name is a label: the same owner may give it to a second vault.
    let twin = CommandEnvelope::create_vault("alice", " main ", Currency::Eur);
    let twin_id = twin.id;
    assert_eq!(core.execute(twin).unwrap().result_id, Some(twin_id));
    assert_eq!(core.vault(twin_id).unwrap().unwrap().name, "main");
    assert_eq!(core.vault(id).unwrap().unwrap().name, "Main");
}

#[test]
fn income_expense_void_reverts_balances() {
    let mut fx = setup();
    fx.run(Command::Income(entry(1000, None, None, Some("Salary"), T0)));
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 1000);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 1000);

    let tx_id = fx.id(Command::Expense(entry(
        300,
        None,
        None,
        Some("Food"),
        T0 + 1,
    )));
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 700);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 700);

    fx.run(Command::VoidTransaction {
        transaction_id: tx_id,
    });
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 1000);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 1000);

    let again = fx.try_run(Command::VoidTransaction {
        transaction_id: tx_id,
    });
    assert!(matches!(again, Err(DomainError::InvalidCommand(_))));
    let voided = list(&fx.core, fx.vault, &all());
    assert!(voided.iter().any(|t| t.id == tx_id && t.voided));
}

#[test]
fn refund_increases_balances_and_amount_is_positive() {
    let mut fx = setup();
    fx.run(Command::Refund(entry(250, None, None, None, T0)));
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 250);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 250);
    let err = fx
        .try_run(Command::Refund(entry(0, None, None, None, T0)))
        .unwrap_err();
    assert!(matches!(err, DomainError::InvalidAmount(_)));
}

#[test]
fn transfer_wallet_does_not_touch_flows() {
    let mut fx = setup();
    let w = Some(fx.wallet);
    let bank = fx.id(wallet_cmd("Bank", 0));
    fx.run(Command::Income(entry(1000, w, None, None, T0)));
    fx.run(Command::TransferWallet {
        amount: 400,
        from_wallet_id: fx.wallet,
        to_wallet_id: bank,
        note: Some("  ".to_string()),
        occurred_at: at(T0 + 1),
    });
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 600);
    assert_eq!(wallet_balance(&fx.core, fx.vault, bank), 400);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 1000);

    let same = fx.try_run(Command::TransferWallet {
        amount: 1,
        from_wallet_id: bank,
        to_wallet_id: bank,
        note: None,
        occurred_at: at(T0),
    });
    assert!(matches!(same, Err(DomainError::InvalidCommand(_))));
}

#[test]
fn flow_caps_are_enforced_and_income_capped_counts_transfers_in() {
    let mut fx = setup();
    fx.run(Command::Income(entry(1000, None, None, None, T0)));
    let vacanze = fx.id(flow_cmd(
        "Vacanze",
        FlowMode::IncomeCapped { cap: 500 },
        false,
        0,
    ));
    let unallocated = fx.unallocated;
    let transfer = |amount| Command::TransferFlow {
        amount,
        from_flow_id: unallocated,
        to_flow_id: vacanze,
        note: None,
        occurred_at: at(T0 + 1),
    };
    fx.run(transfer(300));
    assert_eq!(
        fx.try_run(transfer(300)).unwrap_err(),
        DomainError::MaxBalanceReached("Vacanze".to_string())
    );
    assert_eq!(flow_balance(&fx.core, fx.vault, vacanze), 300);

    let casa = fx.try_run(flow_cmd(
        "Casa",
        FlowMode::NetCapped { cap: 200 },
        false,
        250,
    ));
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

    let bad_cap = fx.try_run(flow_cmd("Zero", FlowMode::NetCapped { cap: 0 }, false, 0));
    assert!(matches!(bad_cap, Err(DomainError::InvalidFlow(_))));
}

#[test]
fn expense_on_flow_without_balance_fails_atomically() {
    let mut fx = setup();
    let fun = fx.id(flow_cmd("Fun", FlowMode::Unlimited, false, 0));
    let before = fx.core.commands_since(fx.vault, 0).unwrap().len();
    let err = fx
        .try_run(Command::Expense(entry(1, None, Some(fun), None, T0)))
        .unwrap_err();
    assert_eq!(err, DomainError::InsufficientFunds("Fun".to_string()));
    assert!(list(&fx.core, fx.vault, &all()).is_empty());
    assert_eq!(fx.core.commands_since(fx.vault, 0).unwrap().len(), before);
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 0);
}

#[test]
fn void_is_never_blocked_by_flow_rules() {
    let mut fx = setup();
    let fun = fx.id(flow_cmd("Fun", FlowMode::NetCapped { cap: 1000 }, false, 0));
    let income = fx.id(Command::Income(entry(500, None, Some(fun), None, T0)));
    fx.run(Command::Expense(entry(200, None, Some(fun), None, T0 + 1)));
    assert_eq!(flow_balance(&fx.core, fx.vault, fun), 300);

    fx.run(Command::VoidTransaction {
        transaction_id: income,
    });
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
    let bank = fx.id(wallet_cmd("Bank", 0));
    let mut ids = Vec::new();
    for i in 0..5 {
        let cmd = if i % 2 == 0 {
            Command::Income(entry(100 + i, Some(fx.wallet), None, Some("A"), T0 + i))
        } else {
            Command::Expense(entry(10, Some(fx.wallet), None, Some("B"), T0 + i))
        };
        ids.push(fx.id(cmd));
    }
    fx.run(Command::TransferWallet {
        amount: 5,
        from_wallet_id: fx.wallet,
        to_wallet_id: bank,
        note: None,
        occurred_at: at(T0 + 10),
    });
    fx.run(Command::VoidTransaction {
        transaction_id: ids[0],
    });

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
        from: Some(utc(T0 + 1)),
        to: Some(utc(T0 + 3)),
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
    let bank = fx.id(wallet_cmd("  Bank ", 0));
    let s = fx.core.snapshot(fx.vault).unwrap();
    assert_eq!(
        s.wallets.iter().find(|w| w.id == bank).unwrap().name,
        "Bank"
    );
    assert_eq!(
        fx.try_run(wallet_cmd("bank", 0)).unwrap_err(),
        DomainError::AlreadyExists("bank".to_string())
    );
    assert!(matches!(
        fx.try_run(wallet_cmd("   ", 0)),
        Err(DomainError::InvalidName(_))
    ));
    assert!(matches!(
        fx.try_run(flow_cmd("Unallocated", FlowMode::Unlimited, false, 0)),
        Err(DomainError::InvalidFlow(_))
    ));
    fx.run(flow_cmd("Vacanze", FlowMode::Unlimited, false, 0));
    assert!(matches!(
        fx.try_run(flow_cmd("VACANZE", FlowMode::Unlimited, false, 0)),
        Err(DomainError::AlreadyExists(_))
    ));
}

#[test]
fn category_free_text_resolves_and_blank_is_uncategorized() {
    let mut fx = setup();
    fx.run(Command::Income(entry(10, None, None, None, T0)));
    let a = fx.id(Command::Expense(entry(
        1,
        None,
        None,
        Some("Spesa"),
        T0 + 1,
    )));
    let b = fx.id(Command::Expense(entry(
        1,
        None,
        None,
        Some("  spesa!!! "),
        T0 + 2,
    )));
    let c = fx.id(Command::Expense(entry(1, None, None, Some("   "), T0 + 3)));
    let rows = list(&fx.core, fx.vault, &all());
    let find = |id| rows.iter().find(|t| t.id == id).unwrap();
    assert_eq!(find(a).category, "Spesa");
    assert_eq!(find(a).category_id, find(b).category_id);
    assert_eq!(find(c).category, "Uncategorized");
    let cats = fx.core.categories(fx.vault, false).unwrap();
    assert_eq!(cats.iter().filter(|c| !c.is_system).count(), 1);

    fx.run(Command::CreateCategory {
        name: "Auto-Moto".to_string(),
    });
    assert!(matches!(
        fx.try_run(Command::CreateCategory {
            name: "auto moto".to_string()
        }),
        Err(DomainError::AlreadyExists(_))
    ));
    assert!(matches!(
        fx.try_run(Command::CreateCategory {
            name: "Uncategorized".to_string()
        }),
        Err(DomainError::InvalidName(_))
    ));
}

#[test]
fn opening_balances_are_real_transactions() {
    let mut fx = setup();
    let bank = fx.id(wallet_cmd("Bank", 1234));
    let card = fx.id(wallet_cmd("Card", -50));
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

    let goal = fx.id(flow_cmd("Goal", FlowMode::Unlimited, false, 200));
    assert_eq!(flow_balance(&fx.core, fx.vault, goal), 200);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 984);
    assert!(matches!(
        fx.try_run(flow_cmd("Neg", FlowMode::Unlimited, true, -1)),
        Err(DomainError::InvalidAmount(_))
    ));
}

#[test]
fn wallet_defaults_to_the_only_active_wallet() {
    let mut fx = setup();
    fx.run(Command::Income(entry(10, None, None, None, T0)));
    fx.run(wallet_cmd("Bank", 0));
    let err = fx
        .try_run(Command::Income(entry(10, None, None, None, T0)))
        .unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));
    let missing = fx.try_run(Command::Income(entry(
        10,
        Some(Uuid::now_v7()),
        None,
        None,
        T0,
    )));
    assert_eq!(
        missing.unwrap_err(),
        DomainError::NotFound("wallet".to_string())
    );
}

#[test]
fn wallets_and_flows_always_balance_and_replay_rebuilds_the_projection() {
    let mut fx = setup();
    let bank = fx.id(wallet_cmd("Bank", 5000));
    let vacanze = fx.id(flow_cmd(
        "Vacanze",
        FlowMode::NetCapped { cap: 100_000 },
        false,
        1000,
    ));
    fx.run(Command::Income(entry(
        2000,
        Some(fx.wallet),
        None,
        Some("Stipendio"),
        T0 + 1,
    )));
    let e = fx.id(Command::Expense(entry(
        300,
        Some(bank),
        Some(vacanze),
        Some("Treno"),
        T0 + 2,
    )));
    fx.run(Command::Refund(entry(
        50,
        Some(bank),
        Some(vacanze),
        Some("treno"),
        T0 + 3,
    )));
    fx.run(Command::TransferWallet {
        amount: 700,
        from_wallet_id: bank,
        to_wallet_id: fx.wallet,
        note: Some("prelievo".to_string()),
        occurred_at: at(T0 + 4),
    });
    fx.run(Command::TransferFlow {
        amount: 100,
        from_flow_id: vacanze,
        to_flow_id: fx.unallocated,
        note: None,
        occurred_at: at(T0 + 5),
    });
    fx.run(Command::VoidTransaction { transaction_id: e });

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
    replayed(&fx.core, fx.vault);
}

#[test]
fn database_file_survives_reopen() {
    let db = TempDb::new("core");
    let vault = {
        let mut core = Core::open(db.path()).unwrap();
        let vault = create_vault(&mut core, "alice", "Main");
        run(&mut core, vault, wallet_cmd("Cash", 700));
        vault
    };
    let core = Core::open(db.path()).unwrap();
    assert_eq!(core.snapshot(vault).unwrap().wallets[0].balance, 700);
}
