#![allow(clippy::expect_used, clippy::unwrap_used)]

use chrono::{TimeZone, Utc};
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, Statement};

use engine::{Currency, Engine, EngineError, TransactionKind, TransactionListFilter};
use migration::MigratorTrait;
use uuid::Uuid;

async fn engine_with_db() -> (Engine, DatabaseConnection) {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    migration::Migrator::up(&db, None).await.unwrap();
    let backend = db.get_database_backend();
    for username in ["alice", "bob", "charlie"] {
        db.execute(Statement::from_sql_and_values(
            backend,
            "INSERT INTO users (username, password) VALUES (?, ?)",
            vec![username.into(), "password".into()],
        ))
        .await
        .unwrap();
    }
    let engine = Engine::builder()
        .database(db.clone())
        .build()
        .await
        .unwrap();
    (engine, db)
}

async fn engine_with_file_db() -> (Engine, DatabaseConnection, String, std::path::PathBuf) {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test_dbs");
    std::fs::create_dir_all(&root).unwrap();

    let path = root.join(format!("engine_{}.db", Uuid::new_v4()));
    let url = format!("sqlite:{}?mode=rwc", path.display());

    let db = Database::connect(&url).await.unwrap();
    migration::Migrator::up(&db, None).await.unwrap();
    let backend = db.get_database_backend();
    for username in ["alice", "bob", "charlie"] {
        db.execute(Statement::from_sql_and_values(
            backend,
            "INSERT INTO users (username, password) VALUES (?, ?)",
            vec![username.into(), "password".into()],
        ))
        .await
        .unwrap();
    }
    let engine = Engine::builder()
        .database(db.clone())
        .build()
        .await
        .unwrap();

    (engine, db, url, path)
}

fn default_wallet_id(vault: &engine::Vault) -> uuid::Uuid {
    *vault
        .wallet
        .iter()
        .find_map(|(id, wallet)| (wallet.name == "Cash").then_some(id))
        .expect("default wallet Cash missing")
}

#[tokio::test]
async fn new_vault_creates_unallocated_and_default_wallet() {
    let (engine, _db) = engine_with_db().await;

    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let vault = engine
        .vault_snapshot(Some(&vault_id), None, "alice")
        .await
        .unwrap();
    assert!(vault.cash_flow.values().any(|f| f.is_unallocated()));
    assert!(vault.wallet.values().any(|w| w.name == "Cash"));
}

#[tokio::test]
async fn income_expense_void_reverts_balances() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let flow_id = engine
        .new_cash_flow(&vault_id, "Vacanze", 0, None, None, "alice")
        .await
        .unwrap();

    let wallet_id = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };

    engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 1000, Utc::now())
                .flow_id(flow_id)
                .wallet_id(wallet_id)
                .category("salary")
                .note("January"),
        )
        .await
        .unwrap();

    let flow = engine.cash_flow(flow_id, &vault_id, "alice").await.unwrap();
    assert_eq!(flow.balance, 1000);
    let wallet = engine.wallet(wallet_id, &vault_id, "alice").await.unwrap();
    assert_eq!(wallet.balance, 1000);

    let expense_id = engine
        .expense(
            engine::ExpenseCmd::new(&vault_id, "alice", 200, Utc::now())
                .flow_id(flow_id)
                .wallet_id(wallet_id)
                .category("food")
                .note("Lunch"),
        )
        .await
        .unwrap();

    let flow = engine.cash_flow(flow_id, &vault_id, "alice").await.unwrap();
    assert_eq!(flow.balance, 800);
    let wallet = engine.wallet(wallet_id, &vault_id, "alice").await.unwrap();
    assert_eq!(wallet.balance, 800);

    engine
        .void_transaction(&vault_id, expense_id, "alice", Utc::now())
        .await
        .unwrap();

    let flow = engine.cash_flow(flow_id, &vault_id, "alice").await.unwrap();
    assert_eq!(flow.balance, 1000);
    let wallet = engine.wallet(wallet_id, &vault_id, "alice").await.unwrap();
    assert_eq!(wallet.balance, 1000);
}

#[tokio::test]
async fn refund_increases_balances() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let flow_id = engine
        .new_cash_flow(&vault_id, "Vacanze", 0, None, None, "alice")
        .await
        .unwrap();

    let wallet_id = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };

    engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 1000, Utc::now())
                .flow_id(flow_id)
                .wallet_id(wallet_id)
                .category("salary"),
        )
        .await
        .unwrap();
    engine
        .expense(
            engine::ExpenseCmd::new(&vault_id, "alice", 200, Utc::now())
                .flow_id(flow_id)
                .wallet_id(wallet_id)
                .category("food"),
        )
        .await
        .unwrap();
    engine
        .refund(
            engine::RefundCmd::new(&vault_id, "alice", 50, Utc::now())
                .flow_id(flow_id)
                .wallet_id(wallet_id)
                .category("food")
                .note("refund"),
        )
        .await
        .unwrap();

    let flow = engine.cash_flow(flow_id, &vault_id, "alice").await.unwrap();
    assert_eq!(flow.balance, 850);
    let wallet = engine.wallet(wallet_id, &vault_id, "alice").await.unwrap();
    assert_eq!(wallet.balance, 850);
}

#[tokio::test]
async fn transfer_wallet_does_not_touch_flows() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let wallet_cash = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };
    let wallet_bank = engine
        .new_wallet(&vault_id, "Bank", 0, "alice")
        .await
        .unwrap();

    let unallocated_flow_id = engine
        .vault_snapshot(Some(&vault_id), None, "alice")
        .await
        .unwrap()
        .unallocated_flow_id()
        .unwrap();

    engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 1000, Utc::now())
                .wallet_id(wallet_cash)
                .category("salary"),
        )
        .await
        .unwrap();

    engine
        .transfer_wallet(
            engine::TransferWalletCmd::new(&vault_id, "alice", 250, wallet_cash, wallet_bank, Utc::now())
                .note("move"),
        )
        .await
        .unwrap();

    let cash = engine
        .wallet(wallet_cash, &vault_id, "alice")
        .await
        .unwrap();
    let bank = engine
        .wallet(wallet_bank, &vault_id, "alice")
        .await
        .unwrap();
    assert_eq!(cash.balance, 750);
    assert_eq!(bank.balance, 250);

    let unallocated = engine
        .cash_flow(unallocated_flow_id, &vault_id, "alice")
        .await
        .unwrap();
    assert_eq!(unallocated.balance, 1000);
}

#[tokio::test]
async fn income_capped_counts_transfers_in() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let wallet_id = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };

    let from_flow = engine
        .vault_snapshot(Some(&vault_id), None, "alice")
        .await
        .unwrap()
        .unallocated_flow_id()
        .unwrap();
    let capped_flow = engine
        .new_cash_flow(&vault_id, "Capped", 0, Some(500), Some(true), "alice")
        .await
        .unwrap();

    engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 600, Utc::now())
                .wallet_id(wallet_id)
                .category("salary"),
        )
        .await
        .unwrap();

    let err = engine
        .transfer_flow(
            engine::TransferFlowCmd::new(&vault_id, "alice", 600, from_flow, capped_flow, Utc::now())
                .note("allocate"),
        )
        .await
        .unwrap_err();

    assert_eq!(err, EngineError::MaxBalanceReached("Capped".to_string()));
}

#[tokio::test]
async fn update_transaction_updates_balances() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let flow_id = engine
        .new_cash_flow(&vault_id, "Vacanze", 0, None, None, "alice")
        .await
        .unwrap();
    let wallet_id = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };

    engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 1000, Utc::now())
                .flow_id(flow_id)
                .wallet_id(wallet_id),
        )
        .await
        .unwrap();

    let expense_id = engine
        .expense(
            engine::ExpenseCmd::new(&vault_id, "alice", 100, Utc::now())
                .flow_id(flow_id)
                .wallet_id(wallet_id)
                .category("food"),
        )
        .await
        .unwrap();

    engine
        .update_transaction(
            engine::UpdateTransactionCmd::new(&vault_id, expense_id, "alice")
                .amount_minor(150)
                .category("food")
                .note("bigger lunch"),
        )
        .await
        .unwrap();

    let flow = engine.cash_flow(flow_id, &vault_id, "alice").await.unwrap();
    assert_eq!(flow.balance, 850);
    let wallet = engine.wallet(wallet_id, &vault_id, "alice").await.unwrap();
    assert_eq!(wallet.balance, 850);
}

#[tokio::test]
async fn update_income_can_retarget_wallet_and_flow_and_keeps_metadata_when_omitted() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let flow1 = engine
        .new_cash_flow(&vault_id, "F1", 0, None, None, "alice")
        .await
        .unwrap();
    let flow2 = engine
        .new_cash_flow(&vault_id, "F2", 0, None, None, "alice")
        .await
        .unwrap();

    let wallet1 = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };
    let wallet2 = engine
        .new_wallet(&vault_id, "Bank", 0, "alice")
        .await
        .unwrap();

    let tx_id = engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 100, Utc::now())
                .flow_id(flow1)
                .wallet_id(wallet1)
                .category("salary")
                .note("  hi  "),
        )
        .await
        .unwrap();

    engine
        .update_transaction(
            engine::UpdateTransactionCmd::new(&vault_id, tx_id, "alice")
                .wallet_id(wallet2)
                .flow_id(flow2),
        )
        .await
        .unwrap();

    let w1 = engine.wallet(wallet1, &vault_id, "alice").await.unwrap();
    let w2 = engine.wallet(wallet2, &vault_id, "alice").await.unwrap();
    assert_eq!(w1.balance, 0);
    assert_eq!(w2.balance, 100);

    let f1 = engine.cash_flow(flow1, &vault_id, "alice").await.unwrap();
    let f2 = engine.cash_flow(flow2, &vault_id, "alice").await.unwrap();
    assert_eq!(f1.balance, 0);
    assert_eq!(f2.balance, 100);

    let txs = engine
        .list_transactions_for_wallet(
            &vault_id,
            wallet2,
            "alice",
            10,
            &TransactionListFilter {
                include_voided: false,
                include_transfers: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let updated = txs.into_iter().find(|(tx, _)| tx.id == tx_id).unwrap().0;
    assert_eq!(updated.category.as_deref(), Some("salary"));
    assert_eq!(updated.note.as_deref(), Some("hi"));
}

#[tokio::test]
async fn update_expense_retarget_flow_fails_if_insufficient_and_is_atomic() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let flow1 = engine
        .new_cash_flow(&vault_id, "F1", 0, None, None, "alice")
        .await
        .unwrap();
    let flow2 = engine
        .new_cash_flow(&vault_id, "F2", 0, None, None, "alice")
        .await
        .unwrap();

    let wallet1 = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };

    engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 100, Utc::now())
                .flow_id(flow1)
                .wallet_id(wallet1),
        )
        .await
        .unwrap();
    let expense_id = engine
        .expense(
            engine::ExpenseCmd::new(&vault_id, "alice", 80, Utc::now())
                .flow_id(flow1)
                .wallet_id(wallet1),
        )
        .await
        .unwrap();

    let err = engine
        .update_transaction(
            engine::UpdateTransactionCmd::new(&vault_id, expense_id, "alice").flow_id(flow2),
        )
        .await
        .unwrap_err();
    assert_eq!(err, EngineError::InsufficientFunds("F2".to_string()));

    // No state change on failure.
    let w1 = engine.wallet(wallet1, &vault_id, "alice").await.unwrap();
    assert_eq!(w1.balance, 20);
    let f1 = engine.cash_flow(flow1, &vault_id, "alice").await.unwrap();
    assert_eq!(f1.balance, 20);
    let f2 = engine.cash_flow(flow2, &vault_id, "alice").await.unwrap();
    assert_eq!(f2.balance, 0);
}

#[tokio::test]
async fn update_transfer_wallet_can_change_endpoints_and_amount() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let wallet_cash = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };
    let wallet_bank = engine
        .new_wallet(&vault_id, "Bank", 0, "alice")
        .await
        .unwrap();
    let wallet_card = engine
        .new_wallet(&vault_id, "Card", 0, "alice")
        .await
        .unwrap();

    engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 100, Utc::now()).wallet_id(wallet_cash),
        )
        .await
        .unwrap();

    let tx_id = engine
        .transfer_wallet(
            engine::TransferWalletCmd::new(&vault_id, "alice", 50, wallet_cash, wallet_bank, Utc::now())
                .note(" move "),
        )
        .await
        .unwrap();

    engine
        .update_transaction(
            engine::UpdateTransactionCmd::new(&vault_id, tx_id, "alice")
                .amount_minor(30)
                .from_wallet_id(wallet_bank)
                .to_wallet_id(wallet_card)
                .note("   "),
        )
        .await
        .unwrap();

    let cash = engine
        .wallet(wallet_cash, &vault_id, "alice")
        .await
        .unwrap();
    let bank = engine
        .wallet(wallet_bank, &vault_id, "alice")
        .await
        .unwrap();
    let card = engine
        .wallet(wallet_card, &vault_id, "alice")
        .await
        .unwrap();
    assert_eq!(cash.balance, 100);
    assert_eq!(bank.balance, -30);
    assert_eq!(card.balance, 30);

    // Note cleared by whitespace patch.
    let txs = engine
        .list_transactions_for_wallet(
            &vault_id,
            wallet_card,
            "alice",
            10,
            &TransactionListFilter {
                include_voided: false,
                include_transfers: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let updated = txs.into_iter().find(|(tx, _)| tx.id == tx_id).unwrap().0;
    assert_eq!(updated.note, None);
}

#[tokio::test]
async fn update_transfer_flow_can_change_endpoints_and_amount() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let wallet_cash = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };
    // Seed Unallocated with funds so we can allocate.
    engine
        .income(engine::IncomeCmd::new(&vault_id, "alice", 100, Utc::now()).wallet_id(wallet_cash))
        .await
        .unwrap();

    let f1 = engine
        .new_cash_flow(&vault_id, "F1", 0, None, None, "alice")
        .await
        .unwrap();
    let f2 = engine
        .new_cash_flow(&vault_id, "F2", 0, None, None, "alice")
        .await
        .unwrap();
    let f3 = engine
        .new_cash_flow(&vault_id, "F3", 0, None, None, "alice")
        .await
        .unwrap();

    let unallocated = engine
        .vault_snapshot(Some(&vault_id), None, "alice")
        .await
        .unwrap()
        .unallocated_flow_id()
        .unwrap();
    engine
        .transfer_flow(
            engine::TransferFlowCmd::new(&vault_id, "alice", 60, unallocated, f1, Utc::now())
                .note("seed"),
        )
        .await
        .unwrap();

    let tx_id = engine
        .transfer_flow(
            engine::TransferFlowCmd::new(&vault_id, "alice", 40, f1, f2, Utc::now()).note("move"),
        )
        .await
        .unwrap();

    engine
        .update_transaction(
            engine::UpdateTransactionCmd::new(&vault_id, tx_id, "alice")
                .amount_minor(10)
                .from_flow_id(f1)
                .to_flow_id(f3),
        )
        .await
        .unwrap();

    let f1m = engine.cash_flow(f1, &vault_id, "alice").await.unwrap();
    let f2m = engine.cash_flow(f2, &vault_id, "alice").await.unwrap();
    let f3m = engine.cash_flow(f3, &vault_id, "alice").await.unwrap();
    assert_eq!(f1m.balance, 50);
    assert_eq!(f2m.balance, 0);
    assert_eq!(f3m.balance, 10);
}

#[tokio::test]
async fn recompute_balances_restores_denormalized_state_and_ignores_voided() {
    let (engine, db) = engine_with_db().await;
    let backend = db.get_database_backend();

    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let wallet_cash = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };
    let unallocated_flow = engine
        .vault_snapshot(Some(&vault_id), None, "alice")
        .await
        .unwrap()
        .unallocated_flow_id()
        .unwrap();

    // Flow allocation (no wallets involved).
    let capped_flow = engine
        .new_cash_flow(&vault_id, "Capped", 0, Some(1000), Some(true), "alice")
        .await
        .unwrap();
    engine
        .transfer_flow(
            engine::TransferFlowCmd::new(
                &vault_id,
                "alice",
                300,
                unallocated_flow,
                capped_flow,
                Utc::now(),
            )
            .note("allocate"),
        )
        .await
        .unwrap();

    // Normal spend+void path (should be ignored by recompute).
    let vacanze_flow = engine
        .new_cash_flow(&vault_id, "Vacanze", 0, None, None, "alice")
        .await
        .unwrap();
    engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 1000, Utc::now())
                .flow_id(vacanze_flow)
                .wallet_id(wallet_cash)
                .category("salary"),
        )
        .await
        .unwrap();
    let expense_id = engine
        .expense(
            engine::ExpenseCmd::new(&vault_id, "alice", 200, Utc::now())
                .flow_id(vacanze_flow)
                .wallet_id(wallet_cash)
                .category("food"),
        )
        .await
        .unwrap();
    engine
        .void_transaction(&vault_id, expense_id, "alice", Utc::now())
        .await
        .unwrap();

    // Corrupt denormalized balances directly in DB.
    db.execute(Statement::from_sql_and_values(
        backend,
        "UPDATE wallets SET balance = ? WHERE id = ?;",
        vec![999i64.into(), wallet_cash.to_string().into()],
    ))
    .await
    .unwrap();
    for flow_id in [unallocated_flow, capped_flow, vacanze_flow] {
        if flow_id == capped_flow {
            db.execute(Statement::from_sql_and_values(
                backend,
                "UPDATE cash_flows SET balance = ?, income_balance = ? WHERE id = ?;",
                vec![999i64.into(), 0i64.into(), flow_id.to_string().into()],
            ))
            .await
            .unwrap();
        } else {
            db.execute(Statement::from_sql_and_values(
                backend,
                "UPDATE cash_flows SET balance = ?, income_balance = NULL WHERE id = ?;",
                vec![999i64.into(), flow_id.to_string().into()],
            ))
            .await
            .unwrap();
        }
    }

    engine.recompute_balances(&vault_id, "alice").await.unwrap();

    // Expected balances:
    // - wallet_cash: +1000 (income), voided expense ignored
    // - vacanze_flow: +1000 (income), voided expense ignored
    // - capped_flow: +300 (transfer in)
    // - unallocated: -300 (transfer out); untouched by wallet+vacanze income
    let wallet = engine
        .wallet(wallet_cash, &vault_id, "alice")
        .await
        .unwrap();
    assert_eq!(wallet.balance, 1000);

    let vacanze = engine
        .cash_flow(vacanze_flow, &vault_id, "alice")
        .await
        .unwrap();
    assert_eq!(vacanze.balance, 1000);

    let capped = engine
        .cash_flow(capped_flow, &vault_id, "alice")
        .await
        .unwrap();
    assert_eq!(capped.balance, 300);
    assert_eq!(capped.income_balance, Some(300));

    let unallocated = engine
        .cash_flow(unallocated_flow, &vault_id, "alice")
        .await
        .unwrap();
    assert_eq!(unallocated.balance, -300);

    // Verify DB matches recompute results too.
    let row = db
        .query_one(Statement::from_sql_and_values(
            backend,
            "SELECT balance FROM wallets WHERE id = ?;",
            vec![wallet_cash.to_string().into()],
        ))
        .await
        .unwrap()
        .unwrap();
    let db_balance: i64 = row.try_get("", "balance").unwrap();
    assert_eq!(db_balance, 1000);
}

#[tokio::test]
async fn expense_on_flow_without_balance_fails() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let flow_id = engine
        .new_cash_flow(&vault_id, "Vacanze", 0, None, None, "alice")
        .await
        .unwrap();
    let wallet_id = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };

    let err = engine
        .expense(
            engine::ExpenseCmd::new(&vault_id, "alice", 1, Utc::now())
                .flow_id(flow_id)
                .wallet_id(wallet_id)
                .category("food"),
        )
        .await
        .unwrap_err();
    assert_eq!(err, EngineError::InsufficientFunds("Vacanze".to_string()));
}

#[tokio::test]
async fn list_transactions_excludes_voided_and_transfers_by_default() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();
    let wallet_id = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };

    engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 1000, Utc::now())
                .wallet_id(wallet_id)
                .category("salary"),
        )
        .await
        .unwrap();

    let spend_id = engine
        .expense(
            engine::ExpenseCmd::new(&vault_id, "alice", 100, Utc::now())
                .wallet_id(wallet_id)
                .category("food"),
        )
        .await
        .unwrap();
    engine
        .void_transaction(&vault_id, spend_id, "alice", Utc::now())
        .await
        .unwrap();

    // Transfers should be excluded when include_transfers=false.
    let other_wallet = engine
        .new_wallet(&vault_id, "Bank", 0, "alice")
        .await
        .unwrap();
    engine
        .transfer_wallet(
            engine::TransferWalletCmd::new(
                &vault_id,
                "alice",
                50,
                wallet_id,
                other_wallet,
                Utc::now(),
            )
            .note("move"),
        )
        .await
        .unwrap();

    let txs = engine
        .list_transactions_for_wallet(
            &vault_id,
            wallet_id,
            "alice",
            50,
            &TransactionListFilter {
                include_voided: false,
                include_transfers: false,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].0.kind, engine::TransactionKind::Income);

    let txs = engine
        .list_transactions_for_wallet(
            &vault_id,
            wallet_id,
            "alice",
            50,
            &TransactionListFilter {
                include_voided: true,
                include_transfers: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(txs.len(), 3);
    assert!(txs.iter().any(|(tx, _)| tx.voided_at.is_some()));
    assert!(
        txs.iter()
            .any(|(tx, _)| tx.kind == engine::TransactionKind::TransferWallet)
    );
}

#[tokio::test]
async fn transactions_pagination_cursor_walks_pages_without_duplicates() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();
    let wallet_id = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };

    for i in 0..5 {
        engine
            .income(
                engine::IncomeCmd::new(&vault_id, "alice", 10, Utc::now())
                    .wallet_id(wallet_id)
                    .category("salary")
                    .note(format!("income {i}")),
            )
            .await
            .unwrap();
    }

    let mut cursor: Option<String> = None;
    let mut seen = std::collections::HashSet::new();
    loop {
        let (items, next) = engine
            .list_transactions_for_wallet_page(
                &vault_id,
                wallet_id,
                "alice",
                2,
                cursor.as_deref(),
                &TransactionListFilter {
                    include_voided: false,
                    include_transfers: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        for (tx, _) in items {
            assert!(seen.insert(tx.id), "duplicate transaction id in paging");
        }

        cursor = next;
        if cursor.is_none() {
            break;
        }
    }

    assert_eq!(seen.len(), 5);
}

#[tokio::test]
async fn restart_engine_reads_same_state() {
    let (engine, db, url, path) = engine_with_file_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let wallet_id = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };
    engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 1000, Utc::now())
                .wallet_id(wallet_id)
                .category("salary"),
        )
        .await
        .unwrap();

    drop(engine);
    drop(db);

    let db2 = Database::connect(&url).await.unwrap();
    let engine2 = Engine::builder()
        .database(db2.clone())
        .build()
        .await
        .unwrap();

    let wallet = engine2.wallet(wallet_id, &vault_id, "alice").await.unwrap();
    assert_eq!(wallet.balance, 1000);

    drop(db2);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn idempotency_key_dedupes_create() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let wallet_id = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };

    let id1 = engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 1000, Utc::now())
                .wallet_id(wallet_id)
                .category("salary")
                .idempotency_key("test-key-1"),
        )
        .await
        .unwrap();

    let id2 = engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 1000, Utc::now())
                .wallet_id(wallet_id)
                .category("salary")
                .idempotency_key("test-key-1"),
        )
        .await
        .unwrap();

    assert_eq!(id1, id2);
    let wallet = engine.wallet(wallet_id, &vault_id, "alice").await.unwrap();
    assert_eq!(wallet.balance, 1000);
}

#[tokio::test]
async fn names_are_trimmed_and_unique_case_insensitive() {
    let (engine, db) = engine_with_db().await;
    let backend = db.get_database_backend();

    let vault_id = engine
        .new_vault("  Main  ", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let vault = engine
        .vault_snapshot(Some(&vault_id), None, "alice")
        .await
        .unwrap();
    assert_eq!(vault.name, "Main");

    let err = engine
        .new_vault("main", "alice", Some(Currency::Eur))
        .await
        .unwrap_err();
    assert_eq!(err, EngineError::ExistingKey("main".to_string()));

    let wallet_id = engine
        .new_wallet(&vault_id, "  Bank  ", 0, "alice")
        .await
        .unwrap();

    let err = engine
        .new_wallet(&vault_id, "bank", 0, "alice")
        .await
        .unwrap_err();
    assert_eq!(err, EngineError::ExistingKey("bank".to_string()));

    let wallet = engine.wallet(wallet_id, &vault_id, "alice").await.unwrap();
    assert_eq!(wallet.name, "Bank");

    let flow_id = engine
        .new_cash_flow(&vault_id, "  Vacanze  ", 0, None, None, "alice")
        .await
        .unwrap();

    let err = engine
        .new_cash_flow(&vault_id, "vacanze", 0, None, None, "alice")
        .await
        .unwrap_err();
    assert_eq!(err, EngineError::ExistingKey("vacanze".to_string()));

    let flow = engine.cash_flow(flow_id, &vault_id, "alice").await.unwrap();
    assert_eq!(flow.name, "Vacanze");

    // Empty names are rejected.
    let err = engine
        .new_wallet(&vault_id, "   ", 0, "alice")
        .await
        .unwrap_err();
    assert_eq!(
        err,
        EngineError::InvalidAmount("wallet name must not be empty".to_string())
    );

    let err = engine
        .new_cash_flow(&vault_id, "   ", 0, None, None, "alice")
        .await
        .unwrap_err();
    assert_eq!(
        err,
        EngineError::InvalidFlow("flow name must not be empty".to_string())
    );

    // Degenerate FlowMode in DB is a hard error.
    let unallocated_id = engine
        .vault_snapshot(Some(&vault_id), None, "alice")
        .await
        .unwrap()
        .unallocated_flow_id()
        .unwrap();

    db.execute(Statement::from_sql_and_values(
        backend,
        "UPDATE cash_flows SET income_balance = 0, max_balance = NULL WHERE id = ?;",
        vec![unallocated_id.to_string().into()],
    ))
    .await
    .unwrap();

    let err = engine
        .vault_snapshot(Some(&vault_id), None, "alice")
        .await
        .unwrap_err();
    assert_eq!(
        err,
        EngineError::InvalidFlow(
            "invalid FlowMode for flow 'unallocated': income_balance requires max_balance"
                .to_string()
        )
    );
}

#[tokio::test]
async fn category_and_note_are_trimmed_and_empty_becomes_none() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let wallet_id = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };

    engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 100, Utc::now())
                .wallet_id(wallet_id)
                .category("   ")
                .note("  hello  "),
        )
        .await
        .unwrap();

    let txs = engine
        .list_transactions_for_wallet(
            &vault_id,
            wallet_id,
            "alice",
            10,
            &TransactionListFilter {
                include_voided: false,
                include_transfers: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].0.category, None);
    assert_eq!(txs[0].0.note.as_deref(), Some("hello"));
}

#[tokio::test]
async fn list_transactions_can_filter_by_date_range_and_kinds() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();
    let wallet_id = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };

    let t0 = Utc.with_ymd_and_hms(2025, 1, 1, 10, 0, 0).unwrap();
    let t1 = Utc.with_ymd_and_hms(2025, 1, 2, 10, 0, 0).unwrap();
    let t2 = Utc.with_ymd_and_hms(2025, 1, 3, 10, 0, 0).unwrap();

    let id0 = engine
        .income(engine::IncomeCmd::new(&vault_id, "alice", 10, t0).wallet_id(wallet_id))
        .await
        .unwrap();
    let id1 = engine
        .expense(engine::ExpenseCmd::new(&vault_id, "alice", 5, t1).wallet_id(wallet_id))
        .await
        .unwrap();
    let id2 = engine
        .refund(engine::RefundCmd::new(&vault_id, "alice", 2, t2).wallet_id(wallet_id))
        .await
        .unwrap();
    let _ = (id0, id1, id2);

    // [t1, t2) includes only the expense at t1.
    let filter = TransactionListFilter {
        from: Some(t1),
        to: Some(t2),
        include_transfers: true,
        ..Default::default()
    };
    let txs = engine
        .list_transactions_for_wallet(&vault_id, wallet_id, "alice", 50, &filter)
        .await
        .unwrap();
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].0.kind, TransactionKind::Expense);

    // Kinds allow-list.
    let filter = TransactionListFilter {
        kinds: Some(vec![TransactionKind::Income]),
        include_transfers: true,
        ..Default::default()
    };
    let txs = engine
        .list_transactions_for_wallet(&vault_id, wallet_id, "alice", 50, &filter)
        .await
        .unwrap();
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].0.kind, TransactionKind::Income);
}

#[tokio::test]
async fn list_transactions_rejects_invalid_filters() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();
    let wallet_id = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };

    let from = Utc.with_ymd_and_hms(2025, 1, 2, 0, 0, 0).unwrap();
    let to = Utc.with_ymd_and_hms(2025, 1, 2, 0, 0, 0).unwrap();

    let err = engine
        .list_transactions_for_wallet(
            &vault_id,
            wallet_id,
            "alice",
            10,
            &TransactionListFilter {
                from: Some(from),
                to: Some(to),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        err,
        EngineError::InvalidAmount("invalid range: from must be < to".to_string())
    );

    let err = engine
        .list_transactions_for_wallet(
            &vault_id,
            wallet_id,
            "alice",
            10,
            &TransactionListFilter {
                kinds: Some(vec![]),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        err,
        EngineError::InvalidAmount("kinds must not be empty".to_string())
    );
}

#[tokio::test]
async fn vault_statistics_treats_refunds_as_expense_reduction() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let wallet_id = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };

    engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 1000, Utc::now())
                .wallet_id(wallet_id)
                .category("salary"),
        )
        .await
        .unwrap();

    engine
        .expense(
            engine::ExpenseCmd::new(&vault_id, "alice", 300, Utc::now())
                .wallet_id(wallet_id)
                .category("food"),
        )
        .await
        .unwrap();

    engine
        .refund(
            engine::RefundCmd::new(&vault_id, "alice", 50, Utc::now())
                .wallet_id(wallet_id)
                .category("food"),
        )
        .await
        .unwrap();

    let (currency, balance_minor, total_income_minor, total_expenses_minor) = engine
        .vault_statistics(&vault_id, "alice", false)
        .await
        .unwrap();
    assert_eq!(currency, engine::Currency::Eur);
    assert_eq!(balance_minor, 750);
    assert_eq!(total_income_minor, 1000);
    assert_eq!(total_expenses_minor, 250);
}

#[tokio::test]
async fn flow_membership_allows_reading_flow_without_vault_access() {
    let (engine, db) = engine_with_db().await;
    let backend = db.get_database_backend();

    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let flow_id = engine
        .new_cash_flow(&vault_id, "Shared", 0, None, None, "alice")
        .await
        .unwrap();

    // "bob" is not a vault member, but is a flow viewer.
    db.execute(Statement::from_sql_and_values(
        backend,
        "INSERT INTO flow_memberships (flow_id, user_id, role) VALUES (?, ?, ?);",
        vec![flow_id.to_string().into(), "bob".into(), "viewer".into()],
    ))
    .await
    .unwrap();

    let shared = engine.cash_flow(flow_id, &vault_id, "bob").await.unwrap();
    assert_eq!(shared.name, "Shared");

    let err = engine
        .vault_snapshot(Some(&vault_id), None, "bob")
        .await
        .unwrap_err();
    assert_eq!(
        err,
        EngineError::KeyNotFound("vault not exists".to_string())
    );
}

#[tokio::test]
async fn flow_member_cannot_access_transaction_detail() {
    let (engine, db) = engine_with_db().await;
    let backend = db.get_database_backend();

    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let flow_id = engine
        .new_cash_flow(&vault_id, "Shared", 0, None, None, "alice")
        .await
        .unwrap();

    let wallet_id = {
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, "alice")
            .await
            .unwrap();
        default_wallet_id(&vault)
    };

    let tx_id = engine
        .income(
            engine::IncomeCmd::new(&vault_id, "alice", 100, Utc::now())
                .flow_id(flow_id)
                .wallet_id(wallet_id),
        )
        .await
        .unwrap();

    // "bob" is not a vault member, but is a flow viewer.
    db.execute(Statement::from_sql_and_values(
        backend,
        "INSERT INTO flow_memberships (flow_id, user_id, role) VALUES (?, ?, ?);",
        vec![flow_id.to_string().into(), "bob".into(), "viewer".into()],
    ))
    .await
    .unwrap();

    let err = engine
        .transaction_with_legs(&vault_id, tx_id, "bob")
        .await
        .unwrap_err();
    assert_eq!(err, EngineError::Forbidden("forbidden".to_string()));
}

#[tokio::test]
async fn flow_membership_editor_can_transfer_between_shared_flows_without_vault_membership() {
    let (engine, db) = engine_with_db().await;
    let backend = db.get_database_backend();

    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let f1 = engine
        .new_cash_flow(&vault_id, "F1", 0, None, None, "alice")
        .await
        .unwrap();
    let f2 = engine
        .new_cash_flow(&vault_id, "F2", 0, None, None, "alice")
        .await
        .unwrap();

    for fid in [f1, f2] {
        db.execute(Statement::from_sql_and_values(
            backend,
            "INSERT INTO flow_memberships (flow_id, user_id, role) VALUES (?, ?, ?);",
            vec![fid.to_string().into(), "bob".into(), "editor".into()],
        ))
        .await
        .unwrap();
    }

    // Allocate funds via owner first (Unallocated -> F1).
    let unallocated_row = db
        .query_one(Statement::from_sql_and_values(
            backend,
            "SELECT id FROM cash_flows WHERE vault_id = ? AND system_kind = 'unallocated';",
            vec![vault_id.clone().into()],
        ))
        .await
        .unwrap()
        .unwrap();
    let unallocated_id: String = unallocated_row.try_get("", "id").unwrap();
    let unallocated_id = Uuid::parse_str(&unallocated_id).unwrap();

    engine
        .transfer_flow(
            engine::TransferFlowCmd::new(&vault_id, "alice", 100, unallocated_id, f1, Utc::now())
                .note("seed"),
        )
        .await
        .unwrap();

    // "bob" can move allocation between F1 and F2.
    engine
        .transfer_flow(engine::TransferFlowCmd::new(&vault_id, "bob", 50, f1, f2, Utc::now()).note("move"))
        .await
        .unwrap();

    let f1_model = engine.cash_flow(f1, &vault_id, "bob").await.unwrap();
    let f2_model = engine.cash_flow(f2, &vault_id, "bob").await.unwrap();
    assert_eq!(f1_model.balance, 50);
    assert_eq!(f2_model.balance, 50);
}

#[tokio::test]
async fn vault_owner_can_manage_vault_members() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    engine
        .upsert_vault_member(&vault_id, "bob", "viewer", "alice")
        .await
        .unwrap();

    // Members can read the vault snapshot (role is enforced only for writes).
    engine
        .vault_snapshot(Some(&vault_id), None, "bob")
        .await
        .unwrap();

    let members = engine.list_vault_members(&vault_id, "alice").await.unwrap();
    assert!(members.iter().any(|(u, r)| u == "alice" && r == "owner"));
    assert!(members.iter().any(|(u, r)| u == "bob" && r == "viewer"));

    // Role update via upsert.
    engine
        .upsert_vault_member(&vault_id, "bob", "editor", "alice")
        .await
        .unwrap();
    let members = engine.list_vault_members(&vault_id, "alice").await.unwrap();
    assert!(members.iter().any(|(u, r)| u == "bob" && r == "editor"));

    engine
        .remove_vault_member(&vault_id, "bob", "alice")
        .await
        .unwrap();
    let members = engine.list_vault_members(&vault_id, "alice").await.unwrap();
    assert!(!members.iter().any(|(u, _)| u == "bob"));
}

#[tokio::test]
async fn non_owner_cannot_manage_memberships() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    // Even if "bob" is a member, only the vault owner can manage memberships.
    engine
        .upsert_vault_member(&vault_id, "bob", "viewer", "alice")
        .await
        .unwrap();

    let err = engine
        .upsert_vault_member(&vault_id, "charlie", "viewer", "bob")
        .await
        .unwrap_err();
    assert_eq!(
        err,
        EngineError::KeyNotFound("vault not exists".to_string())
    );

    let err = engine
        .remove_vault_member(&vault_id, "bob", "bob")
        .await
        .unwrap_err();
    assert_eq!(
        err,
        EngineError::KeyNotFound("vault not exists".to_string())
    );
}

#[tokio::test]
async fn vault_owner_can_manage_flow_members_and_unallocated_is_not_shareable() {
    let (engine, _db) = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let flow_id = engine
        .new_cash_flow(&vault_id, "SharedFlow", 0, None, None, "alice")
        .await
        .unwrap();

    engine
        .upsert_flow_member(&vault_id, flow_id, "bob", "viewer", "alice")
        .await
        .unwrap();

    let members = engine
        .list_flow_members(&vault_id, flow_id, "alice")
        .await
        .unwrap();
    assert!(members.iter().any(|(u, r)| u == "bob" && r == "viewer"));

    engine
        .remove_flow_member(&vault_id, flow_id, "bob", "alice")
        .await
        .unwrap();
    let members = engine
        .list_flow_members(&vault_id, flow_id, "alice")
        .await
        .unwrap();
    assert!(!members.iter().any(|(u, _)| u == "bob"));

    // Unallocated flow cannot be shared.
    let unallocated = engine
        .vault_snapshot(Some(&vault_id), None, "alice")
        .await
        .unwrap()
        .unallocated_flow_id()
        .unwrap();
    let err = engine
        .upsert_flow_member(&vault_id, unallocated, "bob", "viewer", "alice")
        .await
        .unwrap_err();
    assert_eq!(
        err,
        EngineError::InvalidFlow("cannot share Unallocated".to_string())
    );
}
