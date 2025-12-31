use std::collections::HashMap;

use sea_orm::{ActiveValue, QueryFilter, Statement, prelude::*, sea_query::Expr};
use uuid::Uuid;

use crate::{
    CashFlow, Currency, EngineError, ResultEngine, TransactionKind, Vault, Wallet, cash_flows,
    categories,
    util::{normalize_category_key, normalize_required_name},
    vault, vault_memberships, wallets,
};

use super::{Engine, parse_vault_uuid};

impl Engine {
    /// Delete or archive a vault
    pub async fn delete_vault(&self, vault_id: &str, user_id: &str) -> ResultEngine<()> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| Box::pin(async move {
            let vault_model = engine
                .require_vault_by_id_write(db_tx, vault_id.as_str(), user_id.as_str())
                .await?;
            let vault_db_id = vault_model.id;

            // Best-effort cascade delete within one DB transaction.
            // (FKs currently don't declare ON DELETE CASCADE everywhere, and some
            // relationships are not FK-backed, so we do it explicitly.)
            let backend = engine.database.get_database_backend();

            let vault_db_id_bytes: Vec<u8> = vault_db_id.as_bytes().to_vec();

            // 1) legs for transactions in this vault
            db_tx
                .execute(Statement::from_sql_and_values(
                    backend,
                    "DELETE FROM legs WHERE transaction_id IN (SELECT id FROM transactions WHERE vault_id = ?);",
                    vec![vault_db_id_bytes.clone().into()],
                ))
                .await?;

            // 2) transactions
            db_tx
                .execute(Statement::from_sql_and_values(
                    backend,
                    "DELETE FROM transactions WHERE vault_id = ?;",
                    vec![vault_db_id_bytes.clone().into()],
                ))
                .await?;

            // 3) category aliases and categories
            db_tx
                .execute(Statement::from_sql_and_values(
                    backend,
                    "DELETE FROM category_aliases WHERE vault_id = ?;",
                    vec![vault_db_id_bytes.clone().into()],
                ))
                .await?;
            db_tx
                .execute(Statement::from_sql_and_values(
                    backend,
                    "DELETE FROM categories WHERE vault_id = ?;",
                    vec![vault_db_id_bytes.clone().into()],
                ))
                .await?;

            // 4) flows and wallets (no more entries table)
            db_tx
                .execute(Statement::from_sql_and_values(
                    backend,
                    "DELETE FROM cash_flows WHERE vault_id = ?;",
                    vec![vault_db_id_bytes.clone().into()],
                ))
                .await?;
            db_tx
                .execute(Statement::from_sql_and_values(
                    backend,
                    "DELETE FROM wallets WHERE vault_id = ?;",
                    vec![vault_db_id_bytes.clone().into()],
                ))
                .await?;

            // 5) vault
            db_tx
                .execute(Statement::from_sql_and_values(
                    backend,
                    "DELETE FROM vaults WHERE id = ?;",
                    vec![vault_db_id_bytes.clone().into()],
                ))
                .await?;

            Ok(())
        }))
        .await
    }

    /// Add a new vault
    pub async fn new_vault(
        &self,
        name: &str,
        user_id: &str,
        currency: Option<Currency>,
    ) -> ResultEngine<String> {
        let name = normalize_required_name(name, "vault")?;
        let user_id = user_id.to_string();

        let mut new_vault = Vault::new(name.clone(), user_id.as_str());
        new_vault.currency = currency.unwrap_or_default();
        let new_vault_id = new_vault.id.clone();
        let vault_entry: vault::ActiveModel = (&new_vault).into();
        self.with_tx(|_engine, db_tx| {
            Box::pin(async move {
                // Enforce unique vault names per owner (case-insensitive) to avoid
                // ambiguous name lookups.
                let exists = vault::Entity::find()
                    .filter(vault::Column::UserId.eq(user_id.clone()))
                    .filter(Expr::cust("LOWER(name)").eq(name.to_lowercase()))
                    .one(db_tx)
                    .await?
                    .is_some();
                if exists {
                    return Err(EngineError::ExistingKey(name));
                }

                vault_entry.insert(db_tx).await?;

                // Create the system flow "Unallocated".
                let mut unallocated = CashFlow::new(
                    cash_flows::UNALLOCATED_INTERNAL_NAME.to_string(),
                    0,
                    None,
                    None,
                    new_vault.currency,
                )?;
                unallocated.system_kind = Some(cash_flows::SystemFlowKind::Unallocated);
                let new_vault_uuid = Uuid::parse_str(&new_vault_id)
                    .map_err(|_| EngineError::InvalidId("invalid vault id".to_string()))?;
                let mut unallocated_model: cash_flows::ActiveModel = (&unallocated).into();
                unallocated_model.vault_id = ActiveValue::Set(new_vault_uuid);
                unallocated_model.insert(db_tx).await?;

                // Create a default wallet ("Cash") so clients can start immediately.
                let default_wallet = Wallet::new("Cash".to_string(), 0, new_vault.currency);
                let mut default_wallet_model: wallets::ActiveModel = (&default_wallet).into();
                default_wallet_model.vault_id = ActiveValue::Set(new_vault_uuid);
                default_wallet_model.insert(db_tx).await?;

                let category_name = "Uncategorized";
                let category_norm = normalize_category_key(category_name)?;
                let category = categories::ActiveModel {
                    id: ActiveValue::Set(Uuid::new_v4()),
                    vault_id: ActiveValue::Set(new_vault_uuid),
                    name: ActiveValue::Set(category_name.to_string()),
                    name_norm: ActiveValue::Set(category_norm),
                    archived: ActiveValue::Set(false),
                    is_system: ActiveValue::Set(true),
                };
                category.insert(db_tx).await?;

                // Scaffolding for future sharing: create the owner membership row.
                let membership = vault_memberships::ActiveModel {
                    vault_id: ActiveValue::Set(new_vault_uuid),
                    user_id: ActiveValue::Set(user_id.clone()),
                    role: ActiveValue::Set("owner".to_string()),
                };
                membership.insert(db_tx).await?;

                Ok(new_vault_id)
            })
        })
        .await
    }

    /// Return a user `Vault`.
    /// Return a vault snapshot from DB, including all wallets and flows.
    pub async fn vault_snapshot(
        &self,
        vault_id: Option<&str>,
        vault_name: Option<String>,
        user_id: &str,
    ) -> ResultEngine<Vault> {
        if vault_id.is_none() && vault_name.is_none() {
            return Err(EngineError::KeyNotFound(
                "missing vault id or name".to_string(),
            ));
        }
        let vault_id = vault_id.map(str::to_string);
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                let vault_model = if let Some(id) = vault_id.as_deref() {
                    engine
                        .require_vault_by_id(db_tx, id, user_id.as_str())
                        .await?
                } else {
                    let name = vault_name.ok_or_else(|| {
                        EngineError::KeyNotFound("missing vault id or name".to_string())
                    })?;
                    engine
                        .require_vault_by_name(db_tx, &name, user_id.as_str())
                        .await?
                };
                let vault_currency = vault_model.currency;

                let flow_models: Vec<cash_flows::Model> = cash_flows::Entity::find()
                    .filter(cash_flows::Column::VaultId.eq(vault_model.id))
                    .all(db_tx)
                    .await?;
                let wallet_models: Vec<wallets::Model> = wallets::Entity::find()
                    .filter(wallets::Column::VaultId.eq(vault_model.id))
                    .all(db_tx)
                    .await?;

                let mut flows = HashMap::new();
                for flow_model in flow_models {
                    let flow = CashFlow::try_from((flow_model, vault_currency))?;
                    flows.insert(flow.id, flow);
                }

                let mut wallets_map = HashMap::new();
                for wallet_model in wallet_models {
                    let wallet = Wallet::try_from((wallet_model, vault_currency))?;
                    wallets_map.insert(wallet.id, wallet);
                }

                let snapshot = Vault {
                    id: vault_model.id.to_string(),
                    name: vault_model.name,
                    cash_flow: flows,
                    wallet: wallets_map,
                    user_id: vault_model.user_id,
                    currency: vault_currency,
                };
                Ok(snapshot)
            })
        })
        .await
    }

    /// Returns vault totals: `(currency, balance_minor, total_income_minor,
    /// total_expenses_minor)`.
    ///
    /// Transfers are excluded from income/expense totals.
    pub async fn vault_statistics(
        &self,
        vault_id: &str,
        user_id: &str,
        include_voided: bool,
    ) -> ResultEngine<(Currency, i64, i64, i64)> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| Box::pin(async move {
            let vault_model = engine
                .require_vault_by_id(db_tx, vault_id.as_str(), user_id.as_str())
                .await?;
            let currency = vault_model.currency;
            let vault_uuid = parse_vault_uuid(vault_id.as_str())?;
            let vault_bytes: Vec<u8> = vault_uuid.as_bytes().to_vec();

            let backend = engine.database.get_database_backend();
            let void_cond = if include_voided {
                ""
            } else {
                " AND voided_at IS NULL"
            };

            let balance_minor: i64 = {
                let stmt = Statement::from_sql_and_values(
                    backend,
                    "SELECT COALESCE(SUM(balance), 0) AS sum FROM wallets WHERE vault_id = ? AND archived = 0;"
                        .to_string(),
                    vec![vault_bytes.clone().into()],
                );
                let row = db_tx.query_one(stmt).await?;
                row.and_then(|r| r.try_get("", "sum").ok()).unwrap_or(0)
            };

            let total_income_minor: i64 = {
                let stmt = Statement::from_sql_and_values(
                    backend,
                    format!(
                        "SELECT COALESCE(SUM(amount_minor), 0) AS sum \
                         FROM transactions \
                         WHERE vault_id = ? AND kind = ?{void_cond}"
                    ),
                    vec![vault_bytes.clone().into(), TransactionKind::Income.as_str().into()],
                );
                let row = db_tx.query_one(stmt).await?;
                row.and_then(|r| r.try_get("", "sum").ok()).unwrap_or(0)
            };

            let total_expenses_minor: i64 = {
                let stmt = Statement::from_sql_and_values(
                    backend,
                    format!(
                        "SELECT COALESCE(SUM(amount_minor), 0) AS sum \
                         FROM transactions \
                         WHERE vault_id = ? AND kind = ?{void_cond}"
                    ),
                    vec![vault_bytes.clone().into(), TransactionKind::Expense.as_str().into()],
                );
                let row = db_tx.query_one(stmt).await?;
                row.and_then(|r| r.try_get("", "sum").ok()).unwrap_or(0)
            };

            let total_refunds_minor: i64 = {
                let stmt = Statement::from_sql_and_values(
                    backend,
                    format!(
                        "SELECT COALESCE(SUM(amount_minor), 0) AS sum \
                         FROM transactions \
                         WHERE vault_id = ? AND kind = ?{void_cond}"
                    ),
                    vec![vault_bytes.clone().into(), TransactionKind::Refund.as_str().into()],
                );
                let row = db_tx.query_one(stmt).await?;
                row.and_then(|r| r.try_get("", "sum").ok()).unwrap_or(0)
            };

            Ok((
                currency,
                balance_minor,
                total_income_minor,
                total_expenses_minor - total_refunds_minor,
            ))
        }))
        .await
    }
}
