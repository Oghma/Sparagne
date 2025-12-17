use chrono::Utc;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, Statement};

use engine::{Currency, Engine, EngineError};
use migration::MigratorTrait;
use uuid::Uuid;

async fn engine_with_db() -> (Engine, DatabaseConnection) {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    migration::Migrator::up(&db, None).await.unwrap();
    let backend = db.get_database_backend();
    db.execute(Statement::from_sql_and_values(
        backend,
        "INSERT INTO users (username, password) VALUES (?, ?)",
        vec!["alice".into(), "password".into()],
    ))
    .await
    .unwrap();
    let engine = Engine::builder()
        .database(db.clone())
        .build()
        .await
        .unwrap();
    (engine, db)
}

async fn engine_with_file_db() -> (Engine, DatabaseConnection, String, std::path::PathBuf) {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test_dbs");
    std::fs::create_dir_all(&root).unwrap();

    let path = root.join(format!("engine_{}.db", Uuid::new_v4()));
    let url = format!("sqlite:{}?mode=rwc", path.display());

    let db = Database::connect(&url).await.unwrap();
    migration::Migrator::up(&db, None).await.unwrap();
    let backend = db.get_database_backend();
    db.execute(Statement::from_sql_and_values(
        backend,
        "INSERT INTO users (username, password) VALUES (?, ?)",
        vec!["alice".into(), "password".into()],
    ))
    .await
    .unwrap();
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
            &vault_id,
            1000,
            Some(flow_id),
            Some(wallet_id),
            Some("salary"),
            Some("January"),
            "alice",
            Utc::now(),
        )
        .await
        .unwrap();

    let flow = engine.cash_flow(flow_id, &vault_id, "alice").await.unwrap();
    assert_eq!(flow.balance, 1000);
    let wallet = engine.wallet(wallet_id, &vault_id, "alice").await.unwrap();
    assert_eq!(wallet.balance, 1000);

    let expense_id = engine
        .expense(
            &vault_id,
            200,
            Some(flow_id),
            Some(wallet_id),
            Some("food"),
            Some("Lunch"),
            "alice",
            Utc::now(),
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
            &vault_id,
            1000,
            Some(flow_id),
            Some(wallet_id),
            Some("salary"),
            None,
            "alice",
            Utc::now(),
        )
        .await
        .unwrap();
    engine
        .expense(
            &vault_id,
            200,
            Some(flow_id),
            Some(wallet_id),
            Some("food"),
            None,
            "alice",
            Utc::now(),
        )
        .await
        .unwrap();
    engine
        .refund(
            &vault_id,
            50,
            Some(flow_id),
            Some(wallet_id),
            Some("food"),
            Some("refund"),
            "alice",
            Utc::now(),
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
            &vault_id,
            1000,
            None,
            Some(wallet_cash),
            Some("salary"),
            None,
            "alice",
            Utc::now(),
        )
        .await
        .unwrap();

    engine
        .transfer_wallet(
            &vault_id,
            250,
            wallet_cash,
            wallet_bank,
            Some("move"),
            "alice",
            Utc::now(),
        )
        .await
        .unwrap();

    let cash = engine.wallet(wallet_cash, &vault_id, "alice").await.unwrap();
    let bank = engine.wallet(wallet_bank, &vault_id, "alice").await.unwrap();
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
            &vault_id,
            600,
            None,
            Some(wallet_id),
            Some("salary"),
            None,
            "alice",
            Utc::now(),
        )
        .await
        .unwrap();

    let err = engine
        .transfer_flow(
            &vault_id,
            600,
            from_flow,
            capped_flow,
            Some("allocate"),
            "alice",
            Utc::now(),
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
            &vault_id,
            1000,
            Some(flow_id),
            Some(wallet_id),
            None,
            None,
            "alice",
            Utc::now(),
        )
        .await
        .unwrap();

    let expense_id = engine
        .expense(
            &vault_id,
            100,
            Some(flow_id),
            Some(wallet_id),
            Some("food"),
            None,
            "alice",
            Utc::now(),
        )
        .await
        .unwrap();

    engine
        .update_transaction(
            &vault_id,
            expense_id,
            "alice",
            150,
            Some("food"),
            Some("bigger lunch"),
            None,
        )
        .await
        .unwrap();

    let flow = engine.cash_flow(flow_id, &vault_id, "alice").await.unwrap();
    assert_eq!(flow.balance, 850);
    let wallet = engine.wallet(wallet_id, &vault_id, "alice").await.unwrap();
    assert_eq!(wallet.balance, 850);
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
            &vault_id,
            300,
            unallocated_flow,
            capped_flow,
            Some("allocate"),
            "alice",
            Utc::now(),
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
            &vault_id,
            1000,
            Some(vacanze_flow),
            Some(wallet_cash),
            Some("salary"),
            None,
            "alice",
            Utc::now(),
        )
        .await
        .unwrap();
    let expense_id = engine
        .expense(
            &vault_id,
            200,
            Some(vacanze_flow),
            Some(wallet_cash),
            Some("food"),
            None,
            "alice",
            Utc::now(),
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
        db.execute(Statement::from_sql_and_values(
            backend,
            "UPDATE cash_flows SET balance = ?, income_balance = ? WHERE id = ?;",
            vec![999i64.into(), 0i64.into(), flow_id.to_string().into()],
        ))
        .await
        .unwrap();
    }

    engine.recompute_balances(&vault_id, "alice").await.unwrap();

    // Expected balances:
    // - wallet_cash: +1000 (income), voided expense ignored
    // - vacanze_flow: +1000 (income), voided expense ignored
    // - capped_flow: +300 (transfer in)
    // - unallocated: -300 (transfer out); untouched by wallet+vacanze income
    let wallet = engine.wallet(wallet_cash, &vault_id, "alice").await.unwrap();
    assert_eq!(wallet.balance, 1000);

    let vacanze = engine
        .cash_flow(vacanze_flow, &vault_id, "alice")
        .await
        .unwrap();
    assert_eq!(vacanze.balance, 1000);

    let capped = engine.cash_flow(capped_flow, &vault_id, "alice").await.unwrap();
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
            &vault_id,
            1,
            Some(flow_id),
            Some(wallet_id),
            Some("food"),
            None,
            "alice",
            Utc::now(),
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
            &vault_id,
            1000,
            None,
            Some(wallet_id),
            Some("salary"),
            None,
            "alice",
            Utc::now(),
        )
        .await
        .unwrap();

    let spend_id = engine
        .expense(
            &vault_id,
            100,
            None,
            Some(wallet_id),
            Some("food"),
            None,
            "alice",
            Utc::now(),
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
            &vault_id,
            50,
            wallet_id,
            other_wallet,
            Some("move"),
            "alice",
            Utc::now(),
        )
        .await
        .unwrap();

    let txs = engine
        .list_transactions_for_wallet(&vault_id, wallet_id, "alice", 50, false, false)
        .await
        .unwrap();
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].0.kind, engine::TransactionKind::Income);

    let txs = engine
        .list_transactions_for_wallet(&vault_id, wallet_id, "alice", 50, true, true)
        .await
        .unwrap();
    assert_eq!(txs.len(), 3);
    assert!(txs.iter().any(|(tx, _)| tx.voided_at.is_some()));
    assert!(txs
        .iter()
        .any(|(tx, _)| tx.kind == engine::TransactionKind::TransferWallet));
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
            &vault_id,
            1000,
            None,
            Some(wallet_id),
            Some("salary"),
            None,
            "alice",
            Utc::now(),
        )
        .await
        .unwrap();

    drop(engine);
    drop(db);

    let db2 = Database::connect(&url).await.unwrap();
    let engine2 = Engine::builder().database(db2.clone()).build().await.unwrap();

    let wallet = engine2.wallet(wallet_id, &vault_id, "alice").await.unwrap();
    assert_eq!(wallet.balance, 1000);

    drop(db2);
    let _ = std::fs::remove_file(path);
}
