use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

pub use cash_flows::CashFlow;
pub use currency::Currency;
pub use error::EngineError;
pub use legs::{Leg, LegTarget};
pub use money::Money;
use sea_orm::{
    ActiveValue, DatabaseTransaction, JoinType, QueryFilter, QueryOrder, QuerySelect, Statement,
    TransactionTrait, prelude::*,
};
pub use transactions::{Transaction, TransactionKind};
pub use vault::Vault;
pub use wallets::Wallet;

mod cash_flows;
mod currency;
mod error;
mod legs;
mod money;
mod transactions;
mod vault;
mod wallets;

type ResultEngine<T> = Result<T, EngineError>;

#[derive(Debug)]
pub struct Engine {
    database: DatabaseConnection,
}

impl Engine {
    /// Return a builder for `Engine`. Help to build the struct.
    pub fn builder() -> EngineBuilder {
        EngineBuilder::default()
    }

    async fn require_vault_by_id(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<vault::Model> {
        let model = vault::Entity::find_by_id(vault_id.to_string())
            .one(db)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
        if model.user_id != user_id {
            return Err(EngineError::KeyNotFound("vault not exists".to_string()));
        }
        Ok(model)
    }

    async fn require_vault_by_name(
        &self,
        db: &DatabaseTransaction,
        vault_name: &str,
        user_id: &str,
    ) -> ResultEngine<vault::Model> {
        let model = vault::Entity::find()
            .filter(vault::Column::Name.eq(vault_name.to_string()))
            .filter(vault::Column::UserId.eq(user_id.to_string()))
            .one(db)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
        Ok(model)
    }

    async fn unallocated_flow_id(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
    ) -> ResultEngine<Uuid> {
        let model = cash_flows::Entity::find()
            .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
            .filter(cash_flows::Column::SystemKind.eq(Some(
                cash_flows::SystemFlowKind::Unallocated.as_str().to_string(),
            )))
            .one(db)
            .await?
            .ok_or_else(|| EngineError::InvalidFlow("missing Unallocated flow".to_string()))?;
        Uuid::parse_str(&model.id)
            .map_err(|_| EngineError::InvalidAmount("invalid cash_flow id".to_string()))
    }

    async fn resolve_flow_id(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        flow_id: Option<Uuid>,
    ) -> ResultEngine<Uuid> {
        if let Some(id) = flow_id {
            // Ensure it exists and belongs to the vault.
            let exists = cash_flows::Entity::find_by_id(id.to_string())
                .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
                .one(db)
                .await?
                .is_some();
            if !exists {
                return Err(EngineError::KeyNotFound("cash_flow not exists".to_string()));
            }
            return Ok(id);
        }
        self.unallocated_flow_id(db, vault_id).await
    }

    async fn resolve_wallet_id(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        wallet_id: Option<Uuid>,
    ) -> ResultEngine<Uuid> {
        if let Some(id) = wallet_id {
            let exists = wallets::Entity::find_by_id(id.to_string())
                .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
                .one(db)
                .await?
                .is_some();
            if !exists {
                return Err(EngineError::KeyNotFound("wallet not exists".to_string()));
            }
            return Ok(id);
        }

        let wallet_models: Vec<wallets::Model> = wallets::Entity::find()
            .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
            .filter(wallets::Column::Archived.eq(false))
            .all(db)
            .await?;

        let mut iter = wallet_models.into_iter();
        let first = iter
            .next()
            .ok_or_else(|| EngineError::KeyNotFound("missing wallet".to_string()))?;
        if iter.next().is_some() {
            return Err(EngineError::InvalidAmount(
                "wallet_id is required when more than one wallet exists".to_string(),
            ));
        }
        Uuid::parse_str(&first.id)
            .map_err(|_| EngineError::InvalidAmount("invalid wallet id".to_string()))
    }

    async fn create_transaction_with_legs(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        vault_currency: Currency,
        tx: &Transaction,
        legs: &[Leg],
    ) -> ResultEngine<Uuid> {
        if tx.currency != vault_currency {
            return Err(EngineError::CurrencyMismatch(format!(
                "vault currency is {}, got {}",
                vault_currency.code(),
                tx.currency.code()
            )));
        }

        if let Some(key) = tx.idempotency_key.as_deref() {
            let existing = transactions::Entity::find()
                .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
                .filter(transactions::Column::CreatedBy.eq(tx.created_by.clone()))
                .filter(transactions::Column::IdempotencyKey.eq(key.to_string()))
                .one(db_tx)
                .await?;
            if let Some(existing) = existing {
                return Uuid::parse_str(&existing.id)
                    .map_err(|_| EngineError::InvalidAmount("invalid transaction id".to_string()));
            }
        }

        // Validate currency and domain invariants by simulating balance changes, while also
        // computing the resulting denormalized balances to persist.
        let mut wallet_new_balances: HashMap<Uuid, i64> = HashMap::new();
        let mut flow_previews: HashMap<Uuid, CashFlow> = HashMap::new();

        for leg in legs {
            if leg.currency != vault_currency {
                return Err(EngineError::CurrencyMismatch(format!(
                    "vault currency is {}, got {}",
                    vault_currency.code(),
                    leg.currency.code()
                )));
            }
            match leg.target {
                LegTarget::Wallet { wallet_id } => {
                    let wallet_model = wallets::Entity::find_by_id(wallet_id.to_string())
                        .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
                        .one(db_tx)
                        .await?
                        .ok_or_else(|| EngineError::KeyNotFound("wallet not exists".to_string()))?;
                    let wallet_currency =
                        Currency::try_from(wallet_model.currency.as_str()).unwrap_or(vault_currency);
                    if wallet_currency != vault_currency {
                        return Err(EngineError::CurrencyMismatch(format!(
                            "wallet currency is {}, got {}",
                            wallet_currency.code(),
                            vault_currency.code()
                        )));
                    }
                    let entry = wallet_new_balances
                        .entry(wallet_id)
                        .or_insert(wallet_model.balance);
                    *entry += leg.amount_minor;
                }
                LegTarget::Flow { flow_id } => {
                    let flow_model = cash_flows::Entity::find_by_id(flow_id.to_string())
                        .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
                        .one(db_tx)
                        .await?
                        .ok_or_else(|| {
                            EngineError::KeyNotFound("cash_flow not exists".to_string())
                        })?;
                    let flow_currency =
                        Currency::try_from(flow_model.currency.as_str()).unwrap_or(vault_currency);
                    if flow_currency != vault_currency {
                        return Err(EngineError::CurrencyMismatch(format!(
                            "flow currency is {}, got {}",
                            flow_currency.code(),
                            vault_currency.code()
                        )));
                    }

                    let id = Uuid::parse_str(&flow_model.id).map_err(|_| {
                        EngineError::InvalidAmount("invalid cash_flow id".to_string())
                    })?;
                    let system_kind = flow_model
                        .system_kind
                        .as_deref()
                        .and_then(|k| cash_flows::SystemFlowKind::try_from(k).ok());
                    let entry = flow_previews.entry(id).or_insert_with(|| CashFlow {
                        id,
                        name: flow_model.name.clone(),
                        system_kind,
                        balance: flow_model.balance,
                        max_balance: flow_model.max_balance,
                        income_balance: flow_model.income_balance,
                        currency: flow_currency,
                        archived: flow_model.archived,
                    });
                    entry.apply_leg_change(0, leg.amount_minor)?;
                }
            }
        }

        if let Err(err) = transactions::ActiveModel::from(tx).insert(db_tx).await {
            if tx.idempotency_key.is_some() {
                let key = tx.idempotency_key.as_deref().unwrap_or_default();
                let existing = transactions::Entity::find()
                    .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
                    .filter(transactions::Column::CreatedBy.eq(tx.created_by.clone()))
                    .filter(transactions::Column::IdempotencyKey.eq(key.to_string()))
                    .one(db_tx)
                    .await?;
                if let Some(existing) = existing {
                    return Uuid::parse_str(&existing.id).map_err(|_| {
                        EngineError::InvalidAmount("invalid transaction id".to_string())
                    });
                }
            }
            return Err(err.into());
        }
        for leg in legs {
            legs::ActiveModel::from(leg).insert(db_tx).await?;
        }

        for (wallet_id, new_balance) in wallet_new_balances {
            let wallet_model = wallets::ActiveModel {
                id: ActiveValue::Set(wallet_id.to_string()),
                balance: ActiveValue::Set(new_balance),
                ..Default::default()
            };
            wallet_model.update(db_tx).await?;
        }

        for (flow_id, flow) in flow_previews {
            let flow_model = cash_flows::ActiveModel {
                id: ActiveValue::Set(flow_id.to_string()),
                balance: ActiveValue::Set(flow.balance),
                income_balance: ActiveValue::Set(flow.income_balance),
                ..Default::default()
            };
            flow_model.update(db_tx).await?;
        }

        Ok(tx.id)
    }

    async fn preview_apply_leg_updates(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        vault_currency: Currency,
        updates: &[(LegTarget, i64, i64)],
    ) -> ResultEngine<(HashMap<Uuid, i64>, HashMap<Uuid, CashFlow>)> {
        let mut wallet_new_balances: HashMap<Uuid, i64> = HashMap::new();
        let mut flow_previews: HashMap<Uuid, CashFlow> = HashMap::new();

        for (target, old_amount_minor, new_amount_minor) in updates {
            match *target {
                LegTarget::Wallet { wallet_id } => {
                    let wallet_model = wallets::Entity::find_by_id(wallet_id.to_string())
                        .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
                        .one(db_tx)
                        .await?
                        .ok_or_else(|| EngineError::KeyNotFound("wallet not exists".to_string()))?;
                    let wallet_currency =
                        Currency::try_from(wallet_model.currency.as_str()).unwrap_or(vault_currency);
                    if wallet_currency != vault_currency {
                        return Err(EngineError::CurrencyMismatch(format!(
                            "vault currency is {}, got {}",
                            vault_currency.code(),
                            wallet_currency.code()
                        )));
                    }
                    let entry = wallet_new_balances.entry(wallet_id).or_insert(wallet_model.balance);
                    *entry = *entry - *old_amount_minor + *new_amount_minor;
                }
                LegTarget::Flow { flow_id } => {
                    let flow_model = cash_flows::Entity::find_by_id(flow_id.to_string())
                        .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
                        .one(db_tx)
                        .await?
                        .ok_or_else(|| {
                            EngineError::KeyNotFound("cash_flow not exists".to_string())
                        })?;
                    let flow_currency =
                        Currency::try_from(flow_model.currency.as_str()).unwrap_or(vault_currency);
                    if flow_currency != vault_currency {
                        return Err(EngineError::CurrencyMismatch(format!(
                            "vault currency is {}, got {}",
                            vault_currency.code(),
                            flow_currency.code()
                        )));
                    }
                    let system_kind = flow_model
                        .system_kind
                        .as_deref()
                        .and_then(|k| cash_flows::SystemFlowKind::try_from(k).ok());
                    let entry = flow_previews.entry(flow_id).or_insert_with(|| CashFlow {
                        id: flow_id,
                        name: flow_model.name.clone(),
                        system_kind,
                        balance: flow_model.balance,
                        max_balance: flow_model.max_balance,
                        income_balance: flow_model.income_balance,
                        currency: flow_currency,
                        archived: flow_model.archived,
                    });
                    entry.apply_leg_change(*old_amount_minor, *new_amount_minor)?;
                }
            }
        }

        Ok((wallet_new_balances, flow_previews))
    }

    async fn persist_targets(
        &self,
        db_tx: &DatabaseTransaction,
        wallet_new_balances: HashMap<Uuid, i64>,
        flow_previews: HashMap<Uuid, CashFlow>,
    ) -> ResultEngine<()> {
        for (wallet_id, new_balance) in wallet_new_balances {
            let wallet_model = wallets::ActiveModel {
                id: ActiveValue::Set(wallet_id.to_string()),
                balance: ActiveValue::Set(new_balance),
                ..Default::default()
            };
            wallet_model.update(db_tx).await?;
        }

        for (flow_id, flow) in flow_previews {
            let flow_model = cash_flows::ActiveModel {
                id: ActiveValue::Set(flow_id.to_string()),
                balance: ActiveValue::Set(flow.balance),
                income_balance: ActiveValue::Set(flow.income_balance),
                ..Default::default()
            };
            flow_model.update(db_tx).await?;
        }

        Ok(())
    }

    /// Create an income transaction (increases both wallet and flow).
    pub async fn income(
        &self,
        vault_id: &str,
        amount_minor: i64,
        flow_id: Option<Uuid>,
        wallet_id: Option<Uuid>,
        category: Option<&str>,
        note: Option<&str>,
        idempotency_key: Option<&str>,
        user_id: &str,
        occurred_at: DateTime<Utc>,
    ) -> ResultEngine<Uuid> {
        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();
        let resolved_flow_id = self.resolve_flow_id(&db_tx, vault_id, flow_id).await?;
        let resolved_wallet_id = self.resolve_wallet_id(&db_tx, vault_id, wallet_id).await?;

        let tx = Transaction::new(
            vault_id.to_string(),
            TransactionKind::Income,
            occurred_at,
            amount_minor,
            currency,
            category.map(|s| s.to_string()),
            note.map(|s| s.to_string()),
            user_id.to_string(),
            idempotency_key.map(|s| s.to_string()),
        )?;
        let legs = vec![
            Leg::new(
                tx.id,
                LegTarget::Wallet {
                    wallet_id: resolved_wallet_id,
                },
                amount_minor,
                currency,
            ),
            Leg::new(
                tx.id,
                LegTarget::Flow {
                    flow_id: resolved_flow_id,
                },
                amount_minor,
                currency,
            ),
        ];

        let id = self
            .create_transaction_with_legs(&db_tx, vault_id, currency, &tx, &legs)
            .await?;
        db_tx.commit().await?;
        Ok(id)
    }

    /// Create an expense transaction (decreases both wallet and flow).
    pub async fn expense(
        &self,
        vault_id: &str,
        amount_minor: i64,
        flow_id: Option<Uuid>,
        wallet_id: Option<Uuid>,
        category: Option<&str>,
        note: Option<&str>,
        idempotency_key: Option<&str>,
        user_id: &str,
        occurred_at: DateTime<Utc>,
    ) -> ResultEngine<Uuid> {
        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();
        let resolved_flow_id = self.resolve_flow_id(&db_tx, vault_id, flow_id).await?;
        let resolved_wallet_id = self.resolve_wallet_id(&db_tx, vault_id, wallet_id).await?;

        let tx = Transaction::new(
            vault_id.to_string(),
            TransactionKind::Expense,
            occurred_at,
            amount_minor,
            currency,
            category.map(|s| s.to_string()),
            note.map(|s| s.to_string()),
            user_id.to_string(),
            idempotency_key.map(|s| s.to_string()),
        )?;
        let legs = vec![
            Leg::new(
                tx.id,
                LegTarget::Wallet {
                    wallet_id: resolved_wallet_id,
                },
                -amount_minor,
                currency,
            ),
            Leg::new(
                tx.id,
                LegTarget::Flow {
                    flow_id: resolved_flow_id,
                },
                -amount_minor,
                currency,
            ),
        ];

        let id = self
            .create_transaction_with_legs(&db_tx, vault_id, currency, &tx, &legs)
            .await?;
        db_tx.commit().await?;
        Ok(id)
    }

    /// Create a refund transaction (increases both wallet and flow).
    ///
    /// A refund is modeled as its own `TransactionKind::Refund` instead of a
    /// negative expense, to keep reporting correct and explicit.
    pub async fn refund(
        &self,
        vault_id: &str,
        amount_minor: i64,
        flow_id: Option<Uuid>,
        wallet_id: Option<Uuid>,
        category: Option<&str>,
        note: Option<&str>,
        idempotency_key: Option<&str>,
        user_id: &str,
        occurred_at: DateTime<Utc>,
    ) -> ResultEngine<Uuid> {
        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();
        let resolved_flow_id = self.resolve_flow_id(&db_tx, vault_id, flow_id).await?;
        let resolved_wallet_id = self.resolve_wallet_id(&db_tx, vault_id, wallet_id).await?;

        let tx = Transaction::new(
            vault_id.to_string(),
            TransactionKind::Refund,
            occurred_at,
            amount_minor,
            currency,
            category.map(|s| s.to_string()),
            note.map(|s| s.to_string()),
            user_id.to_string(),
            idempotency_key.map(|s| s.to_string()),
        )?;
        let legs = vec![
            Leg::new(
                tx.id,
                LegTarget::Wallet {
                    wallet_id: resolved_wallet_id,
                },
                amount_minor,
                currency,
            ),
            Leg::new(
                tx.id,
                LegTarget::Flow {
                    flow_id: resolved_flow_id,
                },
                amount_minor,
                currency,
            ),
        ];

        let id = self
            .create_transaction_with_legs(&db_tx, vault_id, currency, &tx, &legs)
            .await?;
        db_tx.commit().await?;
        Ok(id)
    }

    pub async fn transfer_wallet(
        &self,
        vault_id: &str,
        amount_minor: i64,
        from_wallet_id: Uuid,
        to_wallet_id: Uuid,
        note: Option<&str>,
        idempotency_key: Option<&str>,
        user_id: &str,
        occurred_at: DateTime<Utc>,
    ) -> ResultEngine<Uuid> {
        if from_wallet_id == to_wallet_id {
            return Err(EngineError::InvalidAmount(
                "from_wallet_id and to_wallet_id must differ".to_string(),
            ));
        }
        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();
        // Ensure wallets belong to the vault.
        self.resolve_wallet_id(&db_tx, vault_id, Some(from_wallet_id))
            .await?;
        self.resolve_wallet_id(&db_tx, vault_id, Some(to_wallet_id)).await?;

        let tx = Transaction::new(
            vault_id.to_string(),
            TransactionKind::TransferWallet,
            occurred_at,
            amount_minor,
            currency,
            None,
            note.map(|s| s.to_string()),
            user_id.to_string(),
            idempotency_key.map(|s| s.to_string()),
        )?;
        let legs = vec![
            Leg::new(
                tx.id,
                LegTarget::Wallet {
                    wallet_id: from_wallet_id,
                },
                -amount_minor,
                currency,
            ),
            Leg::new(
                tx.id,
                LegTarget::Wallet {
                    wallet_id: to_wallet_id,
                },
                amount_minor,
                currency,
            ),
        ];

        let id = self
            .create_transaction_with_legs(&db_tx, vault_id, currency, &tx, &legs)
            .await?;
        db_tx.commit().await?;
        Ok(id)
    }

    pub async fn transfer_flow(
        &self,
        vault_id: &str,
        amount_minor: i64,
        from_flow_id: Uuid,
        to_flow_id: Uuid,
        note: Option<&str>,
        idempotency_key: Option<&str>,
        user_id: &str,
        occurred_at: DateTime<Utc>,
    ) -> ResultEngine<Uuid> {
        if from_flow_id == to_flow_id {
            return Err(EngineError::InvalidAmount(
                "from_flow_id and to_flow_id must differ".to_string(),
            ));
        }
        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();
        // Ensure flows belong to the vault.
        self.resolve_flow_id(&db_tx, vault_id, Some(from_flow_id))
            .await?;
        self.resolve_flow_id(&db_tx, vault_id, Some(to_flow_id)).await?;

        let tx = Transaction::new(
            vault_id.to_string(),
            TransactionKind::TransferFlow,
            occurred_at,
            amount_minor,
            currency,
            None,
            note.map(|s| s.to_string()),
            user_id.to_string(),
            idempotency_key.map(|s| s.to_string()),
        )?;
        let legs = vec![
            Leg::new(
                tx.id,
                LegTarget::Flow {
                    flow_id: from_flow_id,
                },
                -amount_minor,
                currency,
            ),
            Leg::new(
                tx.id,
                LegTarget::Flow {
                    flow_id: to_flow_id,
                },
                amount_minor,
                currency,
            ),
        ];

        let id = self
            .create_transaction_with_legs(&db_tx, vault_id, currency, &tx, &legs)
            .await?;
        db_tx.commit().await?;
        Ok(id)
    }

    /// Voids a transaction (soft delete).
    ///
    /// This:
    /// - sets `voided_at`/`voided_by` on the transaction row
    /// - reverts all legs effects on wallet/flow balances
    ///
    /// Voided transactions are hidden by default in lists/reports.
    pub async fn void_transaction(
        &self,
        vault_id: &str,
        transaction_id: Uuid,
        user_id: &str,
        voided_at: DateTime<Utc>,
    ) -> ResultEngine<()> {
        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let vault_currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

        let tx_model = transactions::Entity::find_by_id(transaction_id.to_string())
            .one(&db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("transaction not exists".to_string()))?;
        if tx_model.vault_id != vault_id {
            return Err(EngineError::KeyNotFound(
                "transaction not exists".to_string(),
            ));
        }
        if tx_model.voided_at.is_some() {
            return Err(EngineError::InvalidAmount(
                "transaction already voided".to_string(),
            ));
        }

        let leg_models = legs::Entity::find()
            .filter(legs::Column::TransactionId.eq(transaction_id.to_string()))
            .all(&db_tx)
            .await?;

        let mut updates: Vec<(LegTarget, i64, i64)> = Vec::with_capacity(leg_models.len());
        for leg_model in leg_models {
            let leg = Leg::try_from(leg_model)?;
            updates.push((leg.target, leg.amount_minor, 0));
        }

        let (wallet_new_balances, flow_previews) =
            self.preview_apply_leg_updates(&db_tx, vault_id, vault_currency, &updates)
                .await?;

        let tx_active = transactions::ActiveModel {
            id: ActiveValue::Set(transaction_id.to_string()),
            voided_at: ActiveValue::Set(Some(voided_at)),
            voided_by: ActiveValue::Set(Some(user_id.to_string())),
            ..Default::default()
        };
        tx_active.update(&db_tx).await?;

        self.persist_targets(&db_tx, wallet_new_balances, flow_previews)
            .await?;

        db_tx.commit().await?;
        Ok(())
    }

    /// Updates the amount/metadata of an existing transaction.
    ///
    /// Targets (wallet/flow ids) are kept unchanged. For transfers, the sign of
    /// the two legs is preserved (one negative, one positive).
    pub async fn update_transaction(
        &self,
        vault_id: &str,
        transaction_id: Uuid,
        user_id: &str,
        amount_minor: i64,
        category: Option<&str>,
        note: Option<&str>,
        occurred_at: Option<DateTime<Utc>>,
    ) -> ResultEngine<()> {
        if amount_minor <= 0 {
            return Err(EngineError::InvalidAmount(
                "amount_minor must be > 0".to_string(),
            ));
        }

        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let vault_currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

        let tx_model = transactions::Entity::find_by_id(transaction_id.to_string())
            .one(&db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("transaction not exists".to_string()))?;
        if tx_model.vault_id != vault_id {
            return Err(EngineError::KeyNotFound(
                "transaction not exists".to_string(),
            ));
        }
        if tx_model.voided_at.is_some() {
            return Err(EngineError::InvalidAmount(
                "cannot update a voided transaction".to_string(),
            ));
        }

        let kind = TransactionKind::try_from(tx_model.kind.as_str())?;
        let leg_models = legs::Entity::find()
            .filter(legs::Column::TransactionId.eq(transaction_id.to_string()))
            .all(&db_tx)
            .await?;

        let mut leg_pairs: Vec<(legs::Model, Leg)> = Vec::with_capacity(leg_models.len());
        for leg_model in leg_models {
            let leg = Leg::try_from(leg_model.clone())?;
            leg_pairs.push((leg_model, leg));
        }

        let (mut negative_leg, mut positive_leg): (Option<Leg>, Option<Leg>) = (None, None);
        for (_, leg) in &leg_pairs {
            if leg.amount_minor < 0 {
                negative_leg = Some(leg.clone());
            } else if leg.amount_minor > 0 {
                positive_leg = Some(leg.clone());
            }
        }

        let sign = match kind {
            TransactionKind::Income | TransactionKind::Refund => 1,
            TransactionKind::Expense => -1,
            TransactionKind::TransferWallet | TransactionKind::TransferFlow => 1,
        };

        let mut updates: Vec<(LegTarget, i64, i64)> = Vec::with_capacity(leg_pairs.len());
        let mut leg_amount_updates: HashMap<Uuid, i64> = HashMap::new();

        match kind {
            TransactionKind::Income | TransactionKind::Expense | TransactionKind::Refund => {
                for (_, leg) in &leg_pairs {
                    let new_leg_amount = sign * amount_minor;
                    updates.push((leg.target, leg.amount_minor, new_leg_amount));
                    leg_amount_updates.insert(leg.id, new_leg_amount);
                }
            }
            TransactionKind::TransferWallet | TransactionKind::TransferFlow => {
                let from = negative_leg.ok_or_else(|| {
                    EngineError::InvalidAmount("invalid transfer: missing negative leg".to_string())
                })?;
                let to = positive_leg.ok_or_else(|| {
                    EngineError::InvalidAmount("invalid transfer: missing positive leg".to_string())
                })?;
                for (_, leg) in &leg_pairs {
                    let new_leg_amount = if leg.id == from.id {
                        -amount_minor
                    } else if leg.id == to.id {
                        amount_minor
                    } else {
                        return Err(EngineError::InvalidAmount(
                            "invalid transfer: unexpected legs".to_string(),
                        ));
                    };
                    updates.push((leg.target, leg.amount_minor, new_leg_amount));
                    leg_amount_updates.insert(leg.id, new_leg_amount);
                }
            }
        }

        let (wallet_new_balances, flow_previews) =
            self.preview_apply_leg_updates(&db_tx, vault_id, vault_currency, &updates)
                .await?;

        let tx_active = transactions::ActiveModel {
            id: ActiveValue::Set(transaction_id.to_string()),
            amount_minor: ActiveValue::Set(amount_minor),
            category: ActiveValue::Set(category.map(|s| s.to_string())),
            note: ActiveValue::Set(note.map(|s| s.to_string())),
            occurred_at: ActiveValue::Set(occurred_at.unwrap_or(tx_model.occurred_at)),
            ..Default::default()
        };
        tx_active.update(&db_tx).await?;

        for (model, _) in &leg_pairs {
            let id = Uuid::parse_str(&model.id)
                .map_err(|_| EngineError::InvalidAmount("invalid leg id".to_string()))?;
            let new_amount = leg_amount_updates
                .get(&id)
                .copied()
                .ok_or_else(|| EngineError::InvalidAmount("invalid leg updates".to_string()))?;
            let leg_active = legs::ActiveModel {
                id: ActiveValue::Set(model.id.clone()),
                amount_minor: ActiveValue::Set(new_amount),
                ..Default::default()
            };
            leg_active.update(&db_tx).await?;
        }

        self.persist_targets(&db_tx, wallet_new_balances, flow_previews)
            .await?;

        db_tx.commit().await?;
        Ok(())
    }

    /// Return a [`CashFlow`] (snapshot from DB).
    pub async fn cash_flow(
        &self,
        cash_flow_id: Uuid,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<CashFlow> {
        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let vault_currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

        let model = cash_flows::Entity::find_by_id(cash_flow_id.to_string())
            .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
            .one(&db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;

        let system_kind = model
            .system_kind
            .as_deref()
            .and_then(|k| cash_flows::SystemFlowKind::try_from(k).ok());
        let currency = Currency::try_from(model.currency.as_str()).unwrap_or(vault_currency);
        if currency != vault_currency {
            return Err(EngineError::CurrencyMismatch(format!(
                "vault currency is {}, got {}",
                vault_currency.code(),
                currency.code()
            )));
        }

        db_tx.commit().await?;
        Ok(CashFlow {
            id: cash_flow_id,
            name: model.name,
            system_kind,
            balance: model.balance,
            max_balance: model.max_balance,
            income_balance: model.income_balance,
            currency,
            archived: model.archived,
        })
    }

    pub async fn cash_flow_by_name(
        &self,
        name: &str,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<CashFlow> {
        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let vault_currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

        let model = cash_flows::Entity::find()
            .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
            .filter(cash_flows::Column::Name.eq(name.to_string()))
            .one(&db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;

        let flow_id = Uuid::parse_str(&model.id)
            .map_err(|_| EngineError::InvalidAmount("invalid cash_flow id".to_string()))?;
        let system_kind = model
            .system_kind
            .as_deref()
            .and_then(|k| cash_flows::SystemFlowKind::try_from(k).ok());
        let currency = Currency::try_from(model.currency.as_str()).unwrap_or(vault_currency);
        if currency != vault_currency {
            return Err(EngineError::CurrencyMismatch(format!(
                "vault currency is {}, got {}",
                vault_currency.code(),
                currency.code()
            )));
        }

        db_tx.commit().await?;
        Ok(CashFlow {
            id: flow_id,
            name: model.name,
            system_kind,
            balance: model.balance,
            max_balance: model.max_balance,
            income_balance: model.income_balance,
            currency,
            archived: model.archived,
        })
    }

    /// Delete a cash flow contained by a vault.
    pub async fn delete_cash_flow(
        &self,
        vault_id: &str,
        cash_flow_id: Uuid,
        archive: bool,
        user_id: &str,
    ) -> ResultEngine<()> {
        let db_tx = self.database.begin().await?;
        self.require_vault_by_id(&db_tx, vault_id, user_id).await?;

        let flow_model = cash_flows::Entity::find_by_id(cash_flow_id.to_string())
            .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
            .one(&db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;

        if flow_model
            .system_kind
            .as_deref()
            .is_some_and(|k| k == cash_flows::SystemFlowKind::Unallocated.as_str())
            || flow_model
                .name
                .eq_ignore_ascii_case(cash_flows::UNALLOCATED_INTERNAL_NAME)
        {
            return Err(EngineError::InvalidFlow(if archive {
                "cannot archive Unallocated".to_string()
            } else {
                "cannot delete Unallocated".to_string()
            }));
        }

        if archive {
            let flow_model = cash_flows::ActiveModel {
                id: ActiveValue::Set(cash_flow_id.to_string()),
                archived: ActiveValue::Set(true),
                ..Default::default()
            };
            flow_model.update(&db_tx).await?;
        } else {
            cash_flows::Entity::delete_by_id(cash_flow_id.to_string())
                .exec(&db_tx)
                .await?;
        }
        db_tx.commit().await?;

        Ok(())
    }

    /// Delete or archive a vault
    /// TODO: Add `archive`
    pub async fn delete_vault(&self, vault_id: &str, user_id: &str) -> ResultEngine<()> {
        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let vault_db_id = vault_model.id;

        // Best-effort cascade delete within one DB transaction.
        // (FKs currently don't declare ON DELETE CASCADE everywhere, and some
        // relationships are not FK-backed, so we do it explicitly.)
        let backend = self.database.get_database_backend();

        // 1) legs for transactions in this vault
        db_tx
            .execute(Statement::from_sql_and_values(
                backend,
                "DELETE FROM legs WHERE transaction_id IN (SELECT id FROM transactions WHERE vault_id = ?);",
                vec![vault_db_id.clone().into()],
            ))
            .await?;

        // 2) transactions
        db_tx
            .execute(Statement::from_sql_and_values(
                backend,
                "DELETE FROM transactions WHERE vault_id = ?;",
                vec![vault_db_id.clone().into()],
            ))
            .await?;

        // 3) flows and wallets
        // (Legacy) entries table (kept for now) references wallets/flows via FK.
        db_tx
            .execute(Statement::from_sql_and_values(
                backend,
                "DELETE FROM entries WHERE vault_id = ?;",
                vec![vault_db_id.clone().into()],
            ))
            .await?;

        db_tx
            .execute(Statement::from_sql_and_values(
                backend,
                "DELETE FROM cash_flows WHERE vault_id = ?;",
                vec![vault_db_id.clone().into()],
            ))
            .await?;
        db_tx
            .execute(Statement::from_sql_and_values(
                backend,
                "DELETE FROM wallets WHERE vault_id = ?;",
                vec![vault_db_id.clone().into()],
            ))
            .await?;

        // 4) vault
        db_tx
            .execute(Statement::from_sql_and_values(
                backend,
                "DELETE FROM vaults WHERE id = ?;",
                vec![vault_db_id.clone().into()],
            ))
            .await?;

        db_tx.commit().await?;

        Ok(())
    }

    /// Add a new vault
    pub async fn new_vault(
        &self,
        name: &str,
        user_id: &str,
        currency: Option<Currency>,
    ) -> ResultEngine<String> {
        let mut new_vault = Vault::new(name.to_string(), user_id);
        new_vault.currency = currency.unwrap_or_default();
        let new_vault_id = new_vault.id.clone();
        let vault_entry: vault::ActiveModel = (&new_vault).into();

        let db_tx = self.database.begin().await?;
        vault_entry.insert(&db_tx).await?;

        // Create the system flow "Unallocated".
        let mut unallocated = CashFlow::new(
            cash_flows::UNALLOCATED_INTERNAL_NAME.to_string(),
            0,
            None,
            None,
            new_vault.currency,
        )?;
        unallocated.system_kind = Some(cash_flows::SystemFlowKind::Unallocated);
        let mut unallocated_model: cash_flows::ActiveModel = (&unallocated).into();
        unallocated_model.vault_id = ActiveValue::Set(new_vault_id.clone());
        unallocated_model.insert(&db_tx).await?;

        // Create a default wallet ("Cash") so clients can start immediately.
        let default_wallet = Wallet::new("Cash".to_string(), 0, new_vault.currency);
        let mut default_wallet_model: wallets::ActiveModel = (&default_wallet).into();
        default_wallet_model.vault_id = ActiveValue::Set(new_vault_id.clone());
        default_wallet_model.insert(&db_tx).await?;

        db_tx.commit().await?;
        Ok(new_vault_id)
    }

    /// Add a new cash flow inside a vault.
    ///
    /// `balance` represents the initial allocation for the flow and is modeled
    /// as an opening `TransferFlow` from `Unallocated → this flow` (so
    /// transfers do not inflate income/expense stats).
    ///
    /// The opening transfer uses `Utc::now()` as `occurred_at`.
    pub async fn new_cash_flow(
        &self,
        vault_id: &str,
        name: &str,
        balance: i64,
        max_balance: Option<i64>,
        income_bounded: Option<bool>,
        user_id: &str,
    ) -> ResultEngine<Uuid> {
        let occurred_at = Utc::now();
        if balance < 0 {
            return Err(EngineError::InvalidAmount(
                "flow balance must be >= 0".to_string(),
            ));
        }

        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let vault_currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

        if name.eq_ignore_ascii_case(cash_flows::UNALLOCATED_INTERNAL_NAME) {
            return Err(EngineError::InvalidFlow(
                "flow name is reserved".to_string(),
            ));
        }
        let exists = cash_flows::Entity::find()
            .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
            .filter(cash_flows::Column::Name.eq(name.to_string()))
            .one(&db_tx)
            .await?
            .is_some();
        if exists {
            return Err(EngineError::ExistingKey(name.to_string()));
        }

        // Create the flow with a 0 balance. If `balance > 0`, we represent it as an
        // opening allocation transfer from Unallocated → new flow.
        let flow = CashFlow::new(
            name.to_string(),
            0,
            max_balance,
            income_bounded,
            vault_currency,
        )?;
        let flow_id = flow.id;
        let mut flow_model: cash_flows::ActiveModel = (&flow).into();
        flow_model.vault_id = ActiveValue::Set(vault_model.id);
        flow_model.insert(&db_tx).await?;

        if balance > 0 {
            let unallocated_flow_id = self.unallocated_flow_id(&db_tx, vault_id).await?;
            let tx = Transaction::new(
                vault_id.to_string(),
                TransactionKind::TransferFlow,
                occurred_at,
                balance,
                vault_currency,
                None,
                Some(format!("opening allocation for flow '{name}'")),
                user_id.to_string(),
                None,
            )?;

            let legs = vec![
                Leg::new(
                    tx.id,
                    LegTarget::Flow {
                        flow_id: unallocated_flow_id,
                    },
                    -balance,
                    vault_currency,
                ),
                Leg::new(tx.id, LegTarget::Flow { flow_id }, balance, vault_currency),
            ];
            self.create_transaction_with_legs(&db_tx, vault_id, vault_currency, &tx, &legs)
                .await?;
        }

        db_tx.commit().await?;

        Ok(flow_id)
    }

    /// Add a new wallet inside a vault.
    ///
    /// `balance_minor` is modeled as an opening transaction against the system
    /// flow `Unallocated`:
    /// - if `balance_minor > 0`: an opening `Income`
    /// - if `balance_minor < 0`: an opening `Expense`
    ///
    /// The opening transaction uses `Utc::now()` as `occurred_at`.
    pub async fn new_wallet(
        &self,
        vault_id: &str,
        name: &str,
        balance_minor: i64,
        user_id: &str,
    ) -> ResultEngine<Uuid> {
        let occurred_at = Utc::now();
        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

        let exists = wallets::Entity::find()
            .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
            .filter(wallets::Column::Name.eq(name.to_string()))
            .one(&db_tx)
            .await?
            .is_some();
        if exists {
            return Err(EngineError::ExistingKey(name.to_string()));
        }

        // Create the wallet with a 0 balance. If `balance_minor != 0`, we represent it
        // as an opening transaction that affects both the wallet and
        // Unallocated.
        let wallet = Wallet::new(name.to_string(), 0, currency);
        let wallet_id = wallet.id;
        let mut wallet_model: wallets::ActiveModel = (&wallet).into();
        wallet_model.vault_id = ActiveValue::Set(vault_model.id);
        wallet_model.insert(&db_tx).await?;

        if balance_minor != 0 {
            let (kind, signed_amount) = if balance_minor > 0 {
                (TransactionKind::Income, balance_minor)
            } else {
                (TransactionKind::Expense, balance_minor)
            };
            let amount_minor = balance_minor.abs();

            let tx = Transaction::new(
                vault_id.to_string(),
                kind,
                occurred_at,
                amount_minor,
                currency,
                Some("opening".to_string()),
                Some(format!("opening balance for wallet '{name}'")),
                user_id.to_string(),
                None,
            )?;

            let unallocated_flow_id = self.unallocated_flow_id(&db_tx, vault_id).await?;
            let legs = vec![
                Leg::new(
                    tx.id,
                    LegTarget::Wallet { wallet_id },
                    signed_amount,
                    currency,
                ),
                Leg::new(
                    tx.id,
                    LegTarget::Flow {
                        flow_id: unallocated_flow_id,
                    },
                    signed_amount,
                    currency,
                ),
            ];
            self.create_transaction_with_legs(&db_tx, vault_id, currency, &tx, &legs)
                .await?;
        }

        db_tx.commit().await?;
        Ok(wallet_id)
    }

    /// Recomputes denormalized balances for wallets and flows from the ledger
    /// (`transactions` + `legs`).
    ///
    /// - Ignores voided transactions.
    /// - Ignores legacy `entries`.
    /// - Validates flow invariants while replaying legs in chronological order.
    /// - Refreshes the in-memory vault state from DB models post-commit.
    pub async fn recompute_balances(&self, vault_id: &str, user_id: &str) -> ResultEngine<()> {
        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

        // Load all wallets/flows from DB (including archived) to avoid stale RAM
        // issues.
        let wallet_models: Vec<wallets::Model> = wallets::Entity::find()
            .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
            .all(&db_tx)
            .await?;
        let flow_models: Vec<cash_flows::Model> = cash_flows::Entity::find()
            .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
            .all(&db_tx)
            .await?;

        let mut wallets_by_id: HashMap<Uuid, Wallet> = HashMap::new();
        for model in wallet_models {
            let id = Uuid::parse_str(&model.id)
                .map_err(|_| EngineError::InvalidAmount("invalid wallet id".to_string()))?;
            let wallet_currency = Currency::try_from(model.currency.as_str()).unwrap_or(currency);
            if wallet_currency != currency {
                return Err(EngineError::CurrencyMismatch(format!(
                    "vault currency is {}, got {}",
                    currency.code(),
                    wallet_currency.code()
                )));
            }
            wallets_by_id.insert(
                id,
                Wallet {
                    id,
                    name: model.name,
                    balance: 0,
                    currency: wallet_currency,
                    archived: model.archived,
                },
            );
        }

        let mut flows: HashMap<Uuid, CashFlow> = HashMap::new();
        for model in flow_models {
            let id = Uuid::parse_str(&model.id)
                .map_err(|_| EngineError::InvalidAmount("invalid cash_flow id".to_string()))?;
            let flow_currency = Currency::try_from(model.currency.as_str()).unwrap_or(currency);
            if flow_currency != currency {
                return Err(EngineError::CurrencyMismatch(format!(
                    "vault currency is {}, got {}",
                    currency.code(),
                    flow_currency.code()
                )));
            }
            let system_kind = model
                .system_kind
                .as_deref()
                .and_then(|k| cash_flows::SystemFlowKind::try_from(k).ok());
            flows.insert(
                id,
                CashFlow {
                    id,
                    name: model.name,
                    system_kind,
                    balance: 0,
                    max_balance: model.max_balance,
                    income_balance: if model.max_balance.is_some() {
                        model.income_balance.map(|_| 0)
                    } else {
                        None
                    },
                    currency: flow_currency,
                    archived: model.archived,
                },
            );
        }

        // Replay all non-voided legs in chronological order to validate invariants.
        let leg_models: Vec<legs::Model> = legs::Entity::find()
            .join(JoinType::InnerJoin, legs::Relation::Transactions.def())
            .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
            .filter(transactions::Column::VoidedAt.is_null())
            .order_by_asc(transactions::Column::OccurredAt)
            .order_by_asc(legs::Column::Id)
            .all(&db_tx)
            .await?;

        for leg_model in leg_models {
            let leg = Leg::try_from(leg_model)?;
            if leg.currency != currency {
                return Err(EngineError::CurrencyMismatch(format!(
                    "vault currency is {}, got {}",
                    currency.code(),
                    leg.currency.code()
                )));
            }

            match leg.target {
                LegTarget::Wallet { wallet_id } => {
                    let wallet = wallets_by_id
                        .get_mut(&wallet_id)
                        .ok_or_else(|| EngineError::KeyNotFound("wallet not exists".to_string()))?;
                    wallet.balance += leg.amount_minor;
                }
                LegTarget::Flow { flow_id } => {
                    let flow = flows.get_mut(&flow_id).ok_or_else(|| {
                        EngineError::KeyNotFound("cash_flow not exists".to_string())
                    })?;
                    flow.apply_leg_change(0, leg.amount_minor)?;
                }
            }
        }

        // Persist denormalized balances.
        for (wallet_id, wallet) in &wallets_by_id {
            let wallet_model = wallets::ActiveModel {
                id: ActiveValue::Set(wallet_id.to_string()),
                balance: ActiveValue::Set(wallet.balance),
                ..Default::default()
            };
            wallet_model.update(&db_tx).await?;
        }

        for (flow_id, flow) in &flows {
            let flow_model = cash_flows::ActiveModel {
                id: ActiveValue::Set(flow_id.to_string()),
                balance: ActiveValue::Set(flow.balance),
                income_balance: ActiveValue::Set(flow.income_balance),
                ..Default::default()
            };
            flow_model.update(&db_tx).await?;
        }

        db_tx.commit().await?;
        Ok(())
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

        let db_tx = self.database.begin().await?;
        let vault_model = if let Some(id) = vault_id {
            self.require_vault_by_id(&db_tx, id, user_id).await?
        } else {
            let name = vault_name.ok_or_else(|| {
                EngineError::KeyNotFound("missing vault id or name".to_string())
            })?;
            self.require_vault_by_name(&db_tx, &name, user_id).await?
        };
        let vault_currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

        let flow_models: Vec<cash_flows::Model> = cash_flows::Entity::find()
            .filter(cash_flows::Column::VaultId.eq(vault_model.id.clone()))
            .all(&db_tx)
            .await?;
        let wallet_models: Vec<wallets::Model> = wallets::Entity::find()
            .filter(wallets::Column::VaultId.eq(vault_model.id.clone()))
            .all(&db_tx)
            .await?;

        let mut flows = HashMap::new();
        for flow_model in flow_models {
            let id = Uuid::parse_str(&flow_model.id)
                .map_err(|_| EngineError::InvalidAmount("invalid cash_flow id".to_string()))?;
            let system_kind = flow_model
                .system_kind
                .as_deref()
                .and_then(|k| cash_flows::SystemFlowKind::try_from(k).ok());
            let currency = Currency::try_from(flow_model.currency.as_str()).unwrap_or(vault_currency);
            if currency != vault_currency {
                return Err(EngineError::CurrencyMismatch(format!(
                    "vault currency is {}, got {}",
                    vault_currency.code(),
                    currency.code()
                )));
            }
            flows.insert(
                id,
                CashFlow {
                    id,
                    name: flow_model.name,
                    system_kind,
                    balance: flow_model.balance,
                    max_balance: flow_model.max_balance,
                    income_balance: flow_model.income_balance,
                    currency,
                    archived: flow_model.archived,
                },
            );
        }

        let mut wallets_map = HashMap::new();
        for wallet_model in wallet_models {
            let id = Uuid::parse_str(&wallet_model.id)
                .map_err(|_| EngineError::InvalidAmount("invalid wallet id".to_string()))?;
            let currency =
                Currency::try_from(wallet_model.currency.as_str()).unwrap_or(vault_currency);
            if currency != vault_currency {
                return Err(EngineError::CurrencyMismatch(format!(
                    "vault currency is {}, got {}",
                    vault_currency.code(),
                    currency.code()
                )));
            }
            wallets_map.insert(
                id,
                Wallet {
                    id,
                    name: wallet_model.name,
                    balance: wallet_model.balance,
                    currency,
                    archived: wallet_model.archived,
                },
            );
        }

        db_tx.commit().await?;
        Ok(Vault {
            id: vault_model.id,
            name: vault_model.name,
            cash_flow: flows,
            wallet: wallets_map,
            user_id: vault_model.user_id,
            currency: vault_currency,
        })
    }

    /// Return a wallet snapshot from DB.
    pub async fn wallet(
        &self,
        wallet_id: Uuid,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<Wallet> {
        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let vault_currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

        let model = wallets::Entity::find_by_id(wallet_id.to_string())
            .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
            .one(&db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("wallet not exists".to_string()))?;

        let currency = Currency::try_from(model.currency.as_str()).unwrap_or(vault_currency);
        if currency != vault_currency {
            return Err(EngineError::CurrencyMismatch(format!(
                "vault currency is {}, got {}",
                vault_currency.code(),
                currency.code()
            )));
        }
        db_tx.commit().await?;
        Ok(Wallet {
            id: wallet_id,
            name: model.name,
            balance: model.balance,
            currency,
            archived: model.archived,
        })
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
        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

        let backend = self.database.get_database_backend();
        let void_cond = if include_voided { "" } else { " AND voided_at IS NULL" };

        let balance_minor: i64 = {
            let stmt = Statement::from_sql_and_values(
                backend,
                "SELECT COALESCE(SUM(balance), 0) AS sum FROM wallets WHERE vault_id = ? AND archived = 0;"
                    .to_string(),
                vec![vault_id.into()],
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
                vec![vault_id.into(), TransactionKind::Income.as_str().into()],
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
                vec![vault_id.into(), TransactionKind::Expense.as_str().into()],
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
                vec![vault_id.into(), TransactionKind::Refund.as_str().into()],
            );
            let row = db_tx.query_one(stmt).await?;
            row.and_then(|r| r.try_get("", "sum").ok()).unwrap_or(0)
        };

        db_tx.commit().await?;
        Ok((
            currency,
            balance_minor,
            total_income_minor,
            total_expenses_minor - total_refunds_minor,
        ))
    }

    /// Lists recent transactions that affect a given flow.
    ///
    /// Returns `(transaction, signed_amount_minor)` where `signed_amount_minor`
    /// is the leg amount for that flow.
    pub async fn list_transactions_for_flow(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        user_id: &str,
        limit: u64,
        include_voided: bool,
        include_transfers: bool,
    ) -> ResultEngine<Vec<(Transaction, i64)>> {
        let db_tx = self.database.begin().await?;
        self.require_vault_by_id(&db_tx, vault_id, user_id).await?;

        let mut query = legs::Entity::find()
            .filter(legs::Column::TargetKind.eq(crate::legs::LegTargetKind::Flow.as_str()))
            .filter(legs::Column::TargetId.eq(flow_id.to_string()))
            .find_also_related(transactions::Entity)
            .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
            .order_by_desc(transactions::Column::OccurredAt)
            .limit(limit);

        if !include_voided {
            query = query.filter(transactions::Column::VoidedAt.is_null());
        }
        if !include_transfers {
            query = query.filter(transactions::Column::Kind.is_not_in([
                TransactionKind::TransferWallet.as_str(),
                TransactionKind::TransferFlow.as_str(),
            ]));
        }

        let rows: Vec<(legs::Model, Option<transactions::Model>)> = query.all(&db_tx).await?;

        let mut out = Vec::with_capacity(rows.len());
        for (leg_model, tx_model) in rows {
            let Some(tx_model) = tx_model else { continue };
            let tx = Transaction::try_from(tx_model)?;
            out.push((tx, leg_model.amount_minor));
        }
        db_tx.commit().await?;
        Ok(out)
    }

    /// Lists recent transactions that affect a given wallet.
    ///
    /// Returns `(transaction, signed_amount_minor)` where `signed_amount_minor`
    /// is the leg amount for that wallet.
    pub async fn list_transactions_for_wallet(
        &self,
        vault_id: &str,
        wallet_id: Uuid,
        user_id: &str,
        limit: u64,
        include_voided: bool,
        include_transfers: bool,
    ) -> ResultEngine<Vec<(Transaction, i64)>> {
        let db_tx = self.database.begin().await?;
        self.require_vault_by_id(&db_tx, vault_id, user_id).await?;

        let mut query = legs::Entity::find()
            .filter(legs::Column::TargetKind.eq(crate::legs::LegTargetKind::Wallet.as_str()))
            .filter(legs::Column::TargetId.eq(wallet_id.to_string()))
            .find_also_related(transactions::Entity)
            .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
            .order_by_desc(transactions::Column::OccurredAt)
            .limit(limit);

        if !include_voided {
            query = query.filter(transactions::Column::VoidedAt.is_null());
        }
        if !include_transfers {
            query = query.filter(transactions::Column::Kind.is_not_in([
                TransactionKind::TransferWallet.as_str(),
                TransactionKind::TransferFlow.as_str(),
            ]));
        }

        let rows: Vec<(legs::Model, Option<transactions::Model>)> = query.all(&db_tx).await?;

        let mut out = Vec::with_capacity(rows.len());
        for (leg_model, tx_model) in rows {
            let Some(tx_model) = tx_model else { continue };
            let tx = Transaction::try_from(tx_model)?;
            out.push((tx, leg_model.amount_minor));
        }
        db_tx.commit().await?;
        Ok(out)
    }
}

/// The builder for `Engine`
#[derive(Default)]
pub struct EngineBuilder {
    database: DatabaseConnection,
}

impl EngineBuilder {
    /// Pass the required database
    pub fn database(mut self, db: DatabaseConnection) -> EngineBuilder {
        self.database = db;
        self
    }

    /// Construct `Engine`
    pub async fn build(self) -> ResultEngine<Engine> {
        Ok(Engine {
            database: self.database,
        })
    }
}
