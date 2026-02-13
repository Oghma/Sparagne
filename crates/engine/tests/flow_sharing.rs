//! Tests for cross-vault flow sharing via flow_references.

use engine::{Currency, Engine, EngineBuilder, NewCashFlowParams};
use migration::MigratorTrait;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, Statement};
use uuid::Uuid;

async fn setup_test_db() -> Result<DatabaseConnection, Box<dyn std::error::Error>> {
    let db = Database::connect("sqlite::memory:").await?;
    Ok(db)
}

async fn setup_engine(db: DatabaseConnection) -> Result<Engine, Box<dyn std::error::Error>> {
    migration::Migrator::up(&db, None).await?;

    // Create test users
    let backend = db.get_database_backend();
    for username in ["alice", "bob", "charlie"] {
        db.execute(Statement::from_sql_and_values(
            backend,
            "INSERT INTO users (username, password) VALUES (?, ?)",
            vec![username.into(), "password".into()],
        ))
        .await?;
    }

    let engine = EngineBuilder::default().database(db).build().await?;
    Ok(engine)
}

async fn create_test_vault(
    engine: &Engine,
    name: &str,
    user: &str,
) -> Result<String, engine::EngineError> {
    engine.new_vault(name, user, Some(Currency::Eur)).await
}

async fn create_test_flow(
    engine: &Engine,
    vault_id: &str,
    name: &str,
    user: &str,
) -> Result<Uuid, engine::EngineError> {
    engine
        .new_cash_flow(NewCashFlowParams {
            vault_id,
            name,
            balance: 0,
            max_balance: None,
            income_bounded: None,
            allow_negative: false,
            user_id: user,
        })
        .await
}

#[tokio::test]
async fn test_share_flow_creates_reference() -> Result<(), Box<dyn std::error::Error>> {
    let db = setup_test_db().await?;
    let engine = setup_engine(db).await?;

    // Create user A with vault and flow
    let vault_a = create_test_vault(&engine, "VaultA", "alice").await?;
    let flow_id = create_test_flow(&engine, &vault_a, "SharedFlow", "alice").await?;

    // Create user B with vault
    let vault_b = create_test_vault(&engine, "VaultB", "bob").await?;

    // Share flow from A to B
    engine
        .share_flow_with_user(&vault_a, flow_id, "bob", Some("VaultB"), "editor", "alice")
        .await?;

    // Verify B can see the flow via vault_snapshot
    let snapshot_b = engine.vault_snapshot(Some(&vault_b), None, "bob").await?;

    let shared_flow = snapshot_b
        .cash_flow
        .get(&flow_id)
        .ok_or("flow should be visible in B's vault")?;

    assert_eq!(shared_flow.name, "SharedFlow");
    assert!(shared_flow.is_shared, "flow should be marked as shared");
    assert!(
        shared_flow.is_reference,
        "flow should be marked as reference"
    );
    Ok(())
}

#[tokio::test]
async fn test_cross_vault_transaction() -> Result<(), Box<dyn std::error::Error>> {
    let db = setup_test_db().await?;
    let engine = setup_engine(db).await?;

    // Create user A with vault, wallet, and flow
    let vault_a = create_test_vault(&engine, "VaultA", "alice").await?;
    let flow_id = create_test_flow(&engine, &vault_a, "Casa", "alice").await?;

    // Create user B with vault and wallet
    let vault_b = create_test_vault(&engine, "VaultB", "bob").await?;

    // Get B's default wallet
    let snapshot_b = engine.vault_snapshot(Some(&vault_b), None, "bob").await?;
    let wallet_b_id = snapshot_b
        .wallet
        .values()
        .next()
        .ok_or("B should have a wallet")?
        .id;

    // Share flow from A to B
    engine
        .share_flow_with_user(&vault_a, flow_id, "bob", Some("VaultB"), "editor", "alice")
        .await?;

    // B records income to the shared flow (standard income semantics: both wallet and flow increase)
    let _tx_id = engine
        .income(engine::IncomeCmd {
            vault_id: vault_b.clone(),
            amount_minor: 10000, // €100
            wallet_id: Some(wallet_b_id),
            flow_id: Some(flow_id),
            meta: engine::TxMeta {
                occurred_at: chrono::Utc::now(),
                note: Some("Income to shared flow".to_string()),
                category_id: None,
                category: None,
                idempotency_key: None,
            },
            user_id: "bob".to_string(),
        })
        .await?;

    // Verify flow balance increased in vault A (where flow lives)
    let snapshot_a = engine.vault_snapshot(Some(&vault_a), None, "alice").await?;
    let flow_a = snapshot_a
        .cash_flow
        .get(&flow_id)
        .ok_or("flow should exist in A")?;
    assert_eq!(
        flow_a.balance, 10000,
        "flow balance should reflect B's income"
    );

    // Verify wallet balance also increased in vault B (income increases both wallet and flow)
    let snapshot_b_after = engine.vault_snapshot(Some(&vault_b), None, "bob").await?;
    let wallet_b = snapshot_b_after
        .wallet
        .get(&wallet_b_id)
        .ok_or("wallet should exist")?;
    assert_eq!(
        wallet_b.balance, 10000,
        "wallet balance should increase (income semantics)"
    );
    Ok(())
}

#[tokio::test]
async fn test_remove_flow_reference() -> Result<(), Box<dyn std::error::Error>> {
    let db = setup_test_db().await?;
    let engine = setup_engine(db).await?;

    // Create user A with vault and flow
    let vault_a = create_test_vault(&engine, "VaultA", "alice").await?;
    let flow_id = create_test_flow(&engine, &vault_a, "SharedFlow", "alice").await?;

    // Create user B with vault
    let vault_b = create_test_vault(&engine, "VaultB", "bob").await?;

    // Share flow
    engine
        .share_flow_with_user(&vault_a, flow_id, "bob", Some("VaultB"), "editor", "alice")
        .await?;

    // B removes the reference
    engine
        .remove_flow_reference(&vault_b, flow_id, "bob")
        .await?;

    // Verify flow no longer visible in B's vault
    let snapshot_b = engine.vault_snapshot(Some(&vault_b), None, "bob").await?;

    assert!(
        !snapshot_b.cash_flow.contains_key(&flow_id),
        "flow should no longer be visible in B's vault"
    );

    // Verify flow still exists in A's vault
    let snapshot_a = engine.vault_snapshot(Some(&vault_a), None, "alice").await?;

    assert!(
        snapshot_a.cash_flow.contains_key(&flow_id),
        "flow should still exist in A's vault"
    );
    Ok(())
}

#[tokio::test]
async fn test_shared_flow_archival() -> Result<(), Box<dyn std::error::Error>> {
    let db = setup_test_db().await?;
    let engine = setup_engine(db).await?;

    // Create user A with vault and flow
    let vault_a = create_test_vault(&engine, "VaultA", "alice").await?;
    let flow_id = create_test_flow(&engine, &vault_a, "SharedFlow", "alice").await?;

    // Create user B with vault
    let vault_b = create_test_vault(&engine, "VaultB", "bob").await?;

    // Share flow
    engine
        .share_flow_with_user(&vault_a, flow_id, "bob", Some("VaultB"), "editor", "alice")
        .await?;

    // A archives the flow
    engine
        .set_cash_flow_archived(&vault_a, flow_id, true, "alice")
        .await?;

    // Verify B sees flow as archived (via vault_snapshot with include_archived)
    let snapshot_b = engine.vault_snapshot(Some(&vault_b), None, "bob").await?;

    // Flow should not appear in default snapshot (archived excluded)
    assert!(
        !snapshot_b.cash_flow.contains_key(&flow_id),
        "archived flow should not appear in default snapshot"
    );

    // Verify flow still accessible via list_accessible_flows with include_archived
    let flows = engine
        .list_accessible_flows(&vault_b, "bob", true)
        .await?;

    let archived_flow = flows
        .iter()
        .find(|f| f.id == flow_id)
        .ok_or("archived flow should be accessible")?;

    assert!(archived_flow.archived, "flow should be marked as archived");
    Ok(())
}

#[tokio::test]
async fn test_name_conflict_handling() -> Result<(), Box<dyn std::error::Error>> {
    let db = setup_test_db().await?;
    let engine = setup_engine(db).await?;

    // Create user A with vault and flow "Casa"
    let vault_a = create_test_vault(&engine, "VaultA", "alice").await?;
    let flow_a_id = create_test_flow(&engine, &vault_a, "Casa", "alice").await?;

    // Create user B with vault and flow also named "Casa"
    let vault_b = create_test_vault(&engine, "VaultB", "bob").await?;
    let _flow_b_id = create_test_flow(&engine, &vault_b, "Casa", "bob").await?;

    // Share A's "Casa" with B (should auto-rename to avoid conflict)
    engine
        .share_flow_with_user(
            &vault_a,
            flow_a_id,
            "bob",
            Some("VaultB"),
            "editor",
            "alice",
        )
        .await?;

    // Verify B sees the shared flow with disambiguated name
    let snapshot_b = engine.vault_snapshot(Some(&vault_b), None, "bob").await?;

    let shared_flow = snapshot_b
        .cash_flow
        .get(&flow_a_id)
        .ok_or("shared flow should be visible")?;

    // Should have display_name override like "Casa (alice)"
    assert!(
        shared_flow.name.contains("alice"),
        "shared flow name should include owner for disambiguation: {}",
        shared_flow.name
    );
    Ok(())
}
