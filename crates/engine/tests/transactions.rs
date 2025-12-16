use chrono::Utc;
use sea_orm::{ConnectionTrait, Database, Statement};

use engine::{Currency, Engine, EngineError};
use migration::MigratorTrait;

async fn engine_with_db() -> Engine {
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
    Engine::builder().database(db).build().await.unwrap()
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
    let mut engine = engine_with_db().await;

    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let vault = engine.vault(Some(&vault_id), None, "alice").unwrap();
    assert!(vault.cash_flow.values().any(|f| f.is_unallocated()));
    assert!(vault.wallet.values().any(|w| w.name == "Cash"));
}

#[tokio::test]
async fn income_expense_void_reverts_balances() {
    let mut engine = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let flow_id = engine
        .new_cash_flow(&vault_id, "Vacanze", 0, None, None)
        .await
        .unwrap();

    let wallet_id = {
        let vault = engine.vault(Some(&vault_id), None, "alice").unwrap();
        default_wallet_id(vault)
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

    let flow = engine.cash_flow(flow_id, &vault_id, "alice").unwrap();
    assert_eq!(flow.balance, 1000);
    let wallet = engine.wallet(wallet_id, &vault_id, "alice").unwrap();
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

    let flow = engine.cash_flow(flow_id, &vault_id, "alice").unwrap();
    assert_eq!(flow.balance, 800);
    let wallet = engine.wallet(wallet_id, &vault_id, "alice").unwrap();
    assert_eq!(wallet.balance, 800);

    engine
        .void_transaction(&vault_id, expense_id, "alice", Utc::now())
        .await
        .unwrap();

    let flow = engine.cash_flow(flow_id, &vault_id, "alice").unwrap();
    assert_eq!(flow.balance, 1000);
    let wallet = engine.wallet(wallet_id, &vault_id, "alice").unwrap();
    assert_eq!(wallet.balance, 1000);
}

#[tokio::test]
async fn transfer_wallet_does_not_touch_flows() {
    let mut engine = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let wallet_cash = {
        let vault = engine.vault(Some(&vault_id), None, "alice").unwrap();
        default_wallet_id(vault)
    };
    let wallet_bank = engine
        .new_wallet(&vault_id, "Bank", 0, "alice")
        .await
        .unwrap();

    let unallocated_flow_id = engine
        .vault(Some(&vault_id), None, "alice")
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

    let cash = engine.wallet(wallet_cash, &vault_id, "alice").unwrap();
    let bank = engine.wallet(wallet_bank, &vault_id, "alice").unwrap();
    assert_eq!(cash.balance, 750);
    assert_eq!(bank.balance, 250);

    let unallocated = engine
        .cash_flow(unallocated_flow_id, &vault_id, "alice")
        .unwrap();
    assert_eq!(unallocated.balance, 1000);
}

#[tokio::test]
async fn income_capped_counts_transfers_in() {
    let mut engine = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let wallet_id = {
        let vault = engine.vault(Some(&vault_id), None, "alice").unwrap();
        default_wallet_id(vault)
    };

    let from_flow = engine
        .vault(Some(&vault_id), None, "alice")
        .unwrap()
        .unallocated_flow_id()
        .unwrap();
    let capped_flow = engine
        .new_cash_flow(&vault_id, "Capped", 0, Some(500), Some(true))
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
    let mut engine = engine_with_db().await;
    let vault_id = engine
        .new_vault("Main", "alice", Some(Currency::Eur))
        .await
        .unwrap();

    let flow_id = engine
        .new_cash_flow(&vault_id, "Vacanze", 0, None, None)
        .await
        .unwrap();
    let wallet_id = {
        let vault = engine.vault(Some(&vault_id), None, "alice").unwrap();
        default_wallet_id(vault)
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

    let flow = engine.cash_flow(flow_id, &vault_id, "alice").unwrap();
    assert_eq!(flow.balance, 850);
    let wallet = engine.wallet(wallet_id, &vault_id, "alice").unwrap();
    assert_eq!(wallet.balance, 850);
}
