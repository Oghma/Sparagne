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
    vaults: HashMap<String, Vault>,
    database: DatabaseConnection,
}

impl Engine {
    /// Return a builder for `Engine`. Help to build the struct.
    pub fn builder() -> EngineBuilder {
        EngineBuilder::default()
    }

    fn vault_mut(&mut self, vault_id: &str, user_id: &str) -> ResultEngine<&mut Vault> {
        match self.vaults.get_mut(vault_id) {
            Some(vault) => {
                if vault.user_id != user_id {
                    return Err(EngineError::KeyNotFound("vault not exists".to_string()));
                }
                Ok(vault)
            }
            None => Err(EngineError::KeyNotFound(vault_id.to_string())),
        }
    }

    fn resolve_flow_id(&self, vault: &Vault, flow_id: Option<Uuid>) -> ResultEngine<Uuid> {
        if let Some(id) = flow_id {
            return Ok(id);
        }
        vault.unallocated_flow_id()
    }

    fn resolve_wallet_id(&self, vault: &Vault, wallet_id: Option<Uuid>) -> ResultEngine<Uuid> {
        if let Some(id) = wallet_id {
            return Ok(id);
        }

        let mut active = vault.wallet.iter().filter(|(_, w)| !w.archived);
        let (id, _) = active
            .next()
            .ok_or_else(|| EngineError::KeyNotFound("missing wallet".to_string()))?;
        if active.next().is_some() {
            return Err(EngineError::InvalidAmount(
                "wallet_id is required when more than one wallet exists".to_string(),
            ));
        }
        Ok(*id)
    }

    async fn create_transaction_with_legs(
        &mut self,
        vault_id: &str,
        user_id: &str,
        tx: Transaction,
        legs: Vec<Leg>,
    ) -> ResultEngine<Uuid> {
        let vault_currency = {
            let vault = self.vault(Some(vault_id), None, user_id)?;
            vault.currency
        };
        if tx.currency != vault_currency {
            return Err(EngineError::CurrencyMismatch(format!(
                "vault currency is {}, got {}",
                vault_currency.code(),
                tx.currency.code()
            )));
        }

        // Validate currency and domain invariants by simulating balance changes.
        {
            let vault = self.vault(Some(vault_id), None, user_id)?;
            for leg in &legs {
                if leg.currency != vault_currency {
                    return Err(EngineError::CurrencyMismatch(format!(
                        "vault currency is {}, got {}",
                        vault_currency.code(),
                        leg.currency.code()
                    )));
                }
                match leg.target {
                    // Wallets currently have no constraints (they can go negative), so we only
                    // compute the resulting balances. Flows need a preview because they can reject
                    // changes (caps / non-negativity / recovery mode).
                    LegTarget::Wallet { wallet_id } => {
                        let wallet = vault.wallet.get(&wallet_id).ok_or_else(|| {
                            EngineError::KeyNotFound("wallet not exists".to_string())
                        })?;
                        if wallet.currency != vault_currency {
                            return Err(EngineError::CurrencyMismatch(format!(
                                "wallet currency is {}, got {}",
                                wallet.currency.code(),
                                vault_currency.code()
                            )));
                        }
                    }
                    LegTarget::Flow { flow_id } => {
                        let flow = vault.cash_flow.get(&flow_id).ok_or_else(|| {
                            EngineError::KeyNotFound("cash_flow not exists".to_string())
                        })?;
                        if flow.currency != vault_currency {
                            return Err(EngineError::CurrencyMismatch(format!(
                                "flow currency is {}, got {}",
                                flow.currency.code(),
                                vault_currency.code()
                            )));
                        }
                        let mut preview = flow.clone();
                        preview.apply_leg_change(0, leg.amount_minor)?;
                    }
                }
            }
        }

        let db_tx = self.database.begin().await?;
        transactions::ActiveModel::from(&tx).insert(&db_tx).await?;
        for leg in &legs {
            legs::ActiveModel::from(leg).insert(&db_tx).await?;
        }

        // Persist updated balances (we rely on the in-memory state as the
        // source of truth for balances, but we only mutate it after the DB
        // transaction commits).
        {
            let vault = self.vault(Some(vault_id), None, user_id)?;

            for leg in &legs {
                match leg.target {
                    LegTarget::Wallet { wallet_id } => {
                        let wallet = vault.wallet.get(&wallet_id).ok_or_else(|| {
                            EngineError::KeyNotFound("wallet not exists".to_string())
                        })?;
                        let new_balance = wallet.balance + leg.amount_minor;
                        let wallet_model = wallets::ActiveModel {
                            id: ActiveValue::Set(wallet_id.to_string()),
                            balance: ActiveValue::Set(new_balance),
                            ..Default::default()
                        };
                        wallet_model.update(&db_tx).await?;
                    }
                    LegTarget::Flow { flow_id } => {
                        let flow = vault.cash_flow.get(&flow_id).ok_or_else(|| {
                            EngineError::KeyNotFound("cash_flow not exists".to_string())
                        })?;
                        let mut preview = flow.clone();
                        preview.apply_leg_change(0, leg.amount_minor)?;
                        let flow_model = cash_flows::ActiveModel {
                            id: ActiveValue::Set(flow_id.to_string()),
                            balance: ActiveValue::Set(preview.balance),
                            income_balance: ActiveValue::Set(preview.income_balance),
                            ..Default::default()
                        };
                        flow_model.update(&db_tx).await?;
                    }
                }
            }
        }

        db_tx.commit().await?;

        // Apply changes to in-memory state.
        {
            let vault = self.vault_mut(vault_id, user_id)?;
            for leg in legs {
                match leg.target {
                    LegTarget::Wallet { wallet_id } => {
                        let wallet = vault.wallet.get_mut(&wallet_id).ok_or_else(|| {
                            EngineError::KeyNotFound("wallet not exists".to_string())
                        })?;
                        wallet.apply_leg_change(0, leg.amount_minor);
                    }
                    LegTarget::Flow { flow_id } => {
                        let flow = vault.cash_flow.get_mut(&flow_id).ok_or_else(|| {
                            EngineError::KeyNotFound("cash_flow not exists".to_string())
                        })?;
                        flow.apply_leg_change(0, leg.amount_minor)?;
                    }
                }
            }
        }

        Ok(tx.id)
    }

    fn preview_apply_leg_updates(
        &self,
        vault_id: &str,
        user_id: &str,
        updates: &[(LegTarget, i64, i64)],
    ) -> ResultEngine<(HashMap<Uuid, i64>, HashMap<Uuid, CashFlow>)> {
        let vault = self.vault(Some(vault_id), None, user_id)?;

        let mut wallet_new_balances: HashMap<Uuid, i64> = HashMap::new();
        let mut flow_previews: HashMap<Uuid, CashFlow> = HashMap::new();

        for (target, old_amount_minor, new_amount_minor) in updates {
            match *target {
                LegTarget::Wallet { wallet_id } => {
                    // Wallets currently have no constraints (they can go negative), so we only
                    // compute the resulting balances. Flows need a preview because they can
                    // reject changes (caps / non-negativity / recovery mode).
                    let wallet = vault
                        .wallet
                        .get(&wallet_id)
                        .ok_or_else(|| EngineError::KeyNotFound("wallet not exists".to_string()))?;
                    let entry = wallet_new_balances
                        .entry(wallet_id)
                        .or_insert(wallet.balance);
                    *entry = *entry - *old_amount_minor + *new_amount_minor;
                }
                LegTarget::Flow { flow_id } => {
                    let flow = vault.cash_flow.get(&flow_id).ok_or_else(|| {
                        EngineError::KeyNotFound("cash_flow not exists".to_string())
                    })?;
                    let entry = flow_previews.entry(flow_id).or_insert_with(|| flow.clone());
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

    fn apply_updates_to_memory(
        &mut self,
        vault_id: &str,
        user_id: &str,
        updates: Vec<(LegTarget, i64, i64)>,
    ) -> ResultEngine<()> {
        let vault = self.vault_mut(vault_id, user_id)?;
        for (target, old_amount_minor, new_amount_minor) in updates {
            match target {
                LegTarget::Wallet { wallet_id } => {
                    let wallet = vault
                        .wallet
                        .get_mut(&wallet_id)
                        .ok_or_else(|| EngineError::KeyNotFound("wallet not exists".to_string()))?;
                    wallet.apply_leg_change(old_amount_minor, new_amount_minor);
                }
                LegTarget::Flow { flow_id } => {
                    let flow = vault.cash_flow.get_mut(&flow_id).ok_or_else(|| {
                        EngineError::KeyNotFound("cash_flow not exists".to_string())
                    })?;
                    flow.apply_leg_change(old_amount_minor, new_amount_minor)?;
                }
            }
        }
        Ok(())
    }

    /// Create an income transaction (increases both wallet and flow).
    pub async fn income(
        &mut self,
        vault_id: &str,
        amount_minor: i64,
        flow_id: Option<Uuid>,
        wallet_id: Option<Uuid>,
        category: Option<&str>,
        note: Option<&str>,
        user_id: &str,
        occurred_at: DateTime<Utc>,
    ) -> ResultEngine<Uuid> {
        let (currency, resolved_flow_id, resolved_wallet_id) = {
            let vault = self.vault(Some(vault_id), None, user_id)?;
            (
                vault.currency,
                self.resolve_flow_id(vault, flow_id)?,
                self.resolve_wallet_id(vault, wallet_id)?,
            )
        };

        let tx = Transaction::new(
            vault_id.to_string(),
            TransactionKind::Income,
            occurred_at,
            amount_minor,
            currency,
            category.map(|s| s.to_string()),
            note.map(|s| s.to_string()),
            user_id.to_string(),
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

        self.create_transaction_with_legs(vault_id, user_id, tx, legs)
            .await
    }

    /// Create an expense transaction (decreases both wallet and flow).
    pub async fn expense(
        &mut self,
        vault_id: &str,
        amount_minor: i64,
        flow_id: Option<Uuid>,
        wallet_id: Option<Uuid>,
        category: Option<&str>,
        note: Option<&str>,
        user_id: &str,
        occurred_at: DateTime<Utc>,
    ) -> ResultEngine<Uuid> {
        let (currency, resolved_flow_id, resolved_wallet_id) = {
            let vault = self.vault(Some(vault_id), None, user_id)?;
            (
                vault.currency,
                self.resolve_flow_id(vault, flow_id)?,
                self.resolve_wallet_id(vault, wallet_id)?,
            )
        };

        let tx = Transaction::new(
            vault_id.to_string(),
            TransactionKind::Expense,
            occurred_at,
            amount_minor,
            currency,
            category.map(|s| s.to_string()),
            note.map(|s| s.to_string()),
            user_id.to_string(),
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

        self.create_transaction_with_legs(vault_id, user_id, tx, legs)
            .await
    }

    pub async fn transfer_wallet(
        &mut self,
        vault_id: &str,
        amount_minor: i64,
        from_wallet_id: Uuid,
        to_wallet_id: Uuid,
        note: Option<&str>,
        user_id: &str,
        occurred_at: DateTime<Utc>,
    ) -> ResultEngine<Uuid> {
        if from_wallet_id == to_wallet_id {
            return Err(EngineError::InvalidAmount(
                "from_wallet_id and to_wallet_id must differ".to_string(),
            ));
        }
        let currency = {
            let vault = self.vault(Some(vault_id), None, user_id)?;
            vault.currency
        };

        let tx = Transaction::new(
            vault_id.to_string(),
            TransactionKind::TransferWallet,
            occurred_at,
            amount_minor,
            currency,
            None,
            note.map(|s| s.to_string()),
            user_id.to_string(),
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

        self.create_transaction_with_legs(vault_id, user_id, tx, legs)
            .await
    }

    pub async fn transfer_flow(
        &mut self,
        vault_id: &str,
        amount_minor: i64,
        from_flow_id: Uuid,
        to_flow_id: Uuid,
        note: Option<&str>,
        user_id: &str,
        occurred_at: DateTime<Utc>,
    ) -> ResultEngine<Uuid> {
        if from_flow_id == to_flow_id {
            return Err(EngineError::InvalidAmount(
                "from_flow_id and to_flow_id must differ".to_string(),
            ));
        }
        let currency = {
            let vault = self.vault(Some(vault_id), None, user_id)?;
            vault.currency
        };

        let tx = Transaction::new(
            vault_id.to_string(),
            TransactionKind::TransferFlow,
            occurred_at,
            amount_minor,
            currency,
            None,
            note.map(|s| s.to_string()),
            user_id.to_string(),
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

        self.create_transaction_with_legs(vault_id, user_id, tx, legs)
            .await
    }

    /// Voids a transaction (soft delete).
    ///
    /// This:
    /// - sets `voided_at`/`voided_by` on the transaction row
    /// - reverts all legs effects on wallet/flow balances
    ///
    /// Voided transactions are hidden by default in lists/reports.
    pub async fn void_transaction(
        &mut self,
        vault_id: &str,
        transaction_id: Uuid,
        user_id: &str,
        voided_at: DateTime<Utc>,
    ) -> ResultEngine<()> {
        let db_tx = self.database.begin().await?;

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
            self.preview_apply_leg_updates(vault_id, user_id, &updates)?;

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

        self.apply_updates_to_memory(vault_id, user_id, updates)?;
        Ok(())
    }

    /// Updates the amount/metadata of an existing transaction.
    ///
    /// Targets (wallet/flow ids) are kept unchanged. For transfers, the sign of
    /// the two legs is preserved (one negative, one positive).
    pub async fn update_transaction(
        &mut self,
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
            self.preview_apply_leg_updates(vault_id, user_id, &updates)?;

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

        self.apply_updates_to_memory(vault_id, user_id, updates)?;
        Ok(())
    }

    /// Return a [`CashFlow`]
    pub fn cash_flow(
        &self,
        cash_flow_id: Uuid,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<&CashFlow> {
        let vault = self.vault(Some(vault_id), None, user_id)?;

        vault
            .cash_flow
            .get(&cash_flow_id)
            .ok_or(EngineError::KeyNotFound("cash_flow not exists".to_string()))
    }

    pub fn cash_flow_by_name(
        &self,
        name: &str,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<&CashFlow> {
        let vault = self.vault(Some(vault_id), None, user_id)?;
        vault
            .cash_flow
            .values()
            .find(|flow| flow.name == name)
            .ok_or(EngineError::KeyNotFound("cash_flow not exists".to_string()))
    }

    /// Delete a cash flow contained by a vault.
    pub async fn delete_cash_flow(
        &mut self,
        vault_id: &str,
        cash_flow_id: Uuid,
        archive: bool,
    ) -> ResultEngine<()> {
        match self.vaults.get_mut(vault_id) {
            Some(vault) => {
                let mut flow_model = vault.delete_flow(&cash_flow_id, archive)?;
                flow_model.vault_id = ActiveValue::Set(vault.id.clone());

                if archive {
                    flow_model.archived = ActiveValue::Set(true);
                    flow_model.save(&self.database).await.unwrap();
                } else {
                    flow_model.delete(&self.database).await.unwrap();
                }
                Ok(())
            }
            None => Err(EngineError::KeyNotFound(vault_id.to_string())),
        }
    }

    /// Delete or archive a vault
    /// TODO: Add `archive`
    pub async fn delete_vault(&mut self, vault_id: &str) -> ResultEngine<()> {
        match self.vaults.remove(vault_id) {
            Some(vault) => {
                let vault_model: vault::ActiveModel = (&vault).into();
                vault_model.delete(&self.database).await?;
                Ok(())
            }
            None => Err(EngineError::KeyNotFound(vault_id.to_string())),
        }
    }

    /// Add a new vault
    pub async fn new_vault(
        &mut self,
        name: &str,
        user_id: &str,
        currency: Option<Currency>,
    ) -> ResultEngine<String> {
        let mut new_vault = Vault::new(name.to_string(), user_id);
        new_vault.currency = currency.unwrap_or_default();
        let new_vault_id = new_vault.id.clone();
        let vault_entry: vault::ActiveModel = (&new_vault).into();

        vault_entry.insert(&self.database).await?;

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
        unallocated_model.insert(&self.database).await?;
        new_vault.cash_flow.insert(unallocated.id, unallocated);

        // Create a default wallet ("Cash") so clients can start immediately.
        let default_wallet = Wallet::new("Cash".to_string(), 0, new_vault.currency);
        let default_wallet_id = default_wallet.id;
        let mut default_wallet_model: wallets::ActiveModel = (&default_wallet).into();
        default_wallet_model.vault_id = ActiveValue::Set(new_vault_id.clone());
        default_wallet_model.insert(&self.database).await?;
        new_vault.wallet.insert(default_wallet_id, default_wallet);

        self.vaults.insert(new_vault_id.clone(), new_vault);
        Ok(new_vault_id)
    }

    /// Add a new cash flow inside a vault.
    pub async fn new_cash_flow(
        &mut self,
        vault_id: &str,
        name: &str,
        balance: i64,
        max_balance: Option<i64>,
        income_bounded: Option<bool>,
    ) -> ResultEngine<Uuid> {
        match self.vaults.get_mut(vault_id) {
            Some(vault) => {
                let (id, mut flow) =
                    vault.new_flow(name.to_string(), balance, max_balance, income_bounded)?;
                flow.vault_id = ActiveValue::Set(vault.id.clone());
                flow.insert(&self.database).await?;
                Ok(id)
            }
            None => Err(EngineError::KeyNotFound(vault_id.to_string())),
        }
    }

    /// Add a new wallet inside a vault.
    pub async fn new_wallet(
        &mut self,
        vault_id: &str,
        name: &str,
        balance_minor: i64,
        user_id: &str,
    ) -> ResultEngine<Uuid> {
        let (vault_db_id, currency) = {
            let vault = self.vault(Some(vault_id), None, user_id)?;
            if vault.wallet.values().any(|w| w.name == name) {
                return Err(EngineError::ExistingKey(name.to_string()));
            }
            (vault.id.clone(), vault.currency)
        };

        let wallet = Wallet::new(name.to_string(), balance_minor, currency);
        let wallet_id = wallet.id;
        let mut wallet_model: wallets::ActiveModel = (&wallet).into();
        wallet_model.vault_id = ActiveValue::Set(vault_db_id);
        wallet_model.insert(&self.database).await?;

        let vault = self.vault_mut(vault_id, user_id)?;
        vault.wallet.insert(wallet_id, wallet);
        Ok(wallet_id)
    }

    /// Return a user `Vault`.
    pub fn vault(
        &self,
        vault_id: Option<&str>,
        vault_name: Option<String>,
        user_id: &str,
    ) -> ResultEngine<&Vault> {
        if vault_id.is_none() && vault_name.is_none() {
            return Err(EngineError::KeyNotFound(
                "missing vault id or name".to_string(),
            ));
        }

        let vault = if let Some(id) = vault_id {
            match self.vaults.get(id) {
                None => return Err(EngineError::KeyNotFound("vault not exists".to_string())),
                Some(vault) => {
                    if vault.user_id == user_id {
                        vault
                    } else {
                        return Err(EngineError::KeyNotFound("vault not exists".to_string()));
                    }
                }
            }
        } else {
            let name = vault_name.unwrap();

            match self
                .vaults
                .iter()
                .find(|(_, vault)| vault.name == name && vault.user_id == user_id)
            {
                Some((_, vault)) => vault,
                None => return Err(EngineError::KeyNotFound("vault not exists".to_string())),
            }
        };

        Ok(vault)
    }

    /// Return a wallet.
    pub fn wallet(&self, wallet_id: Uuid, vault_id: &str, user_id: &str) -> ResultEngine<&Wallet> {
        let vault = self.vault(Some(vault_id), None, user_id)?;

        vault
            .wallet
            .get(&wallet_id)
            .ok_or(EngineError::KeyNotFound("wallet not exists".to_string()))
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
        let vault = self.vault(Some(vault_id), None, user_id)?;
        let currency = vault.currency;
        let balance_minor: i64 = vault
            .wallet
            .values()
            .filter(|w| !w.archived)
            .map(|w| w.balance)
            .sum();

        let backend = self.database.get_database_backend();
        let (void_cond, void_args) = if include_voided {
            ("", Vec::<Value>::new())
        } else {
            (" AND voided_at IS NULL", Vec::<Value>::new())
        };

        let total_income_minor: i64 = {
            let stmt = Statement::from_sql_and_values(
                backend,
                format!(
                    "SELECT COALESCE(SUM(amount_minor), 0) AS sum \
                     FROM transactions \
                     WHERE vault_id = ? AND kind = ?{void_cond}"
                ),
                {
                    let mut v = Vec::new();
                    v.push(vault_id.into());
                    v.push(TransactionKind::Income.as_str().into());
                    v.extend(void_args.clone());
                    v
                },
            );
            let row = self.database.query_one(stmt).await?;
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
                {
                    let mut v = Vec::new();
                    v.push(vault_id.into());
                    v.push(TransactionKind::Expense.as_str().into());
                    v.extend(void_args);
                    v
                },
            );
            let row = self.database.query_one(stmt).await?;
            row.and_then(|r| r.try_get("", "sum").ok()).unwrap_or(0)
        };

        Ok((
            currency,
            balance_minor,
            total_income_minor,
            total_expenses_minor,
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
        // Authorization check via vault lookup.
        self.vault(Some(vault_id), None, user_id)?;

        let mut query = legs::Entity::find()
            .filter(legs::Column::TargetKind.eq(crate::legs::LegTargetKind::Flow.as_str()))
            .filter(legs::Column::TargetId.eq(flow_id.to_string()))
            .join(JoinType::InnerJoin, legs::Relation::Transactions.def())
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

        let rows: Vec<(legs::Model, Option<transactions::Model>)> = query
            .find_also_related(transactions::Entity)
            .all(&self.database)
            .await?;

        let mut out = Vec::with_capacity(rows.len());
        for (leg_model, tx_model) in rows {
            let Some(tx_model) = tx_model else { continue };
            let tx = Transaction::try_from(tx_model)?;
            out.push((tx, leg_model.amount_minor));
        }
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
        // Authorization check via vault lookup.
        self.vault(Some(vault_id), None, user_id)?;

        let mut query = legs::Entity::find()
            .filter(legs::Column::TargetKind.eq(crate::legs::LegTargetKind::Wallet.as_str()))
            .filter(legs::Column::TargetId.eq(wallet_id.to_string()))
            .join(JoinType::InnerJoin, legs::Relation::Transactions.def())
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

        let rows: Vec<(legs::Model, Option<transactions::Model>)> = query
            .find_also_related(transactions::Entity)
            .all(&self.database)
            .await?;

        let mut out = Vec::with_capacity(rows.len());
        for (leg_model, tx_model) in rows {
            let Some(tx_model) = tx_model else { continue };
            let tx = Transaction::try_from(tx_model)?;
            out.push((tx, leg_model.amount_minor));
        }
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
    pub async fn build(self) -> Engine {
        let mut vaults = HashMap::new();

        let vault_models: Vec<vault::Model> =
            vault::Entity::find().all(&self.database).await.unwrap();

        for vault_model in vault_models {
            let vault_currency =
                Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

            let mut flows = HashMap::new();
            let flow_models: Vec<cash_flows::Model> = cash_flows::Entity::find()
                .filter(cash_flows::Column::VaultId.eq(vault_model.id.clone()))
                .all(&self.database)
                .await
                .unwrap();

            for flow_model in flow_models {
                let id = Uuid::parse_str(&flow_model.id).unwrap();
                let system_kind = flow_model
                    .system_kind
                    .as_deref()
                    .and_then(|k| cash_flows::SystemFlowKind::try_from(k).ok());
                let flow = CashFlow {
                    id,
                    name: flow_model.name,
                    system_kind,
                    balance: flow_model.balance,
                    max_balance: flow_model.max_balance,
                    income_balance: flow_model.income_balance,
                    currency: Currency::try_from(flow_model.currency.as_str())
                        .unwrap_or(vault_currency),
                    archived: flow_model.archived,
                };
                flows.insert(id, flow);
            }

            let mut wallets = HashMap::new();
            let wallet_models: Vec<wallets::Model> = wallets::Entity::find()
                .filter(wallets::Column::VaultId.eq(vault_model.id.clone()))
                .all(&self.database)
                .await
                .unwrap();

            for wallet_model in wallet_models {
                let id = Uuid::parse_str(&wallet_model.id).unwrap();
                let wallet = Wallet {
                    id,
                    name: wallet_model.name,
                    balance: wallet_model.balance,
                    currency: Currency::try_from(wallet_model.currency.as_str())
                        .unwrap_or(vault_currency),
                    archived: wallet_model.archived,
                };
                wallets.insert(id, wallet);
            }

            vaults.insert(
                vault_model.id.clone(),
                Vault {
                    id: vault_model.id,
                    name: vault_model.name,
                    cash_flow: flows,
                    wallet: wallets,
                    user_id: vault_model.user_id,
                    currency: vault_currency,
                },
            );
        }

        Engine {
            vaults,
            database: self.database,
        }
    }
}
