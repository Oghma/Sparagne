use chrono::Utc;
use uuid::Uuid;

use sea_orm::{ActiveValue, QueryFilter, TransactionTrait, prelude::*, sea_query::Expr};

use crate::{EngineError, ResultEngine, TransactionKind, Wallet, wallets};

use super::{
    Engine, build_transaction, flow_wallet_legs, flow_wallet_signed_amount,
    normalize_required_name, parse_vault_currency, with_tx,
};

impl Engine {
    /// Return a wallet snapshot from DB.
    pub async fn wallet(
        &self,
        wallet_id: Uuid,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<Wallet> {
        with_tx!(self, |db_tx| {
            let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
            let vault_currency = parse_vault_currency(vault_model.currency.as_str())?;

            let wallet_id_str = wallet_id.to_string();
            let model = wallets::Entity::find_by_id(wallet_id_str.clone())
                .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
                .one(&db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("wallet not exists".to_string()))?;

            let wallet = Wallet::try_from((model, vault_currency))?;
            Ok(wallet)
        })
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
        let name = normalize_required_name(name, "wallet")?;
        with_tx!(self, |db_tx| {
            let vault_model = self
                .require_vault_by_id_write(&db_tx, vault_id, user_id)
                .await?;
            let currency = parse_vault_currency(vault_model.currency.as_str())?;

            let exists = wallets::Entity::find()
                .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
                .filter(Expr::cust("LOWER(name)").eq(name.to_lowercase()))
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
                let amount_minor = balance_minor.abs();
                let kind = if balance_minor > 0 {
                    TransactionKind::Income
                } else {
                    TransactionKind::Expense
                };
                let signed_amount = flow_wallet_signed_amount(kind, amount_minor)?;

                let tx = build_transaction(super::TransactionBuildInput {
                    vault_id,
                    kind,
                    occurred_at,
                    amount_minor,
                    currency,
                    category: Some("opening".to_string()),
                    note: Some(format!("opening balance for wallet '{name}'")),
                    created_by: user_id,
                    idempotency_key: None,
                    refunded_transaction_id: None,
                })?;

                let unallocated_flow_id = self.unallocated_flow_id(&db_tx, vault_id).await?;
                let legs = flow_wallet_legs(
                    tx.id,
                    wallet_id,
                    unallocated_flow_id,
                    signed_amount,
                    currency,
                );
                self.create_transaction_with_legs(&db_tx, vault_id, currency, &tx, &legs)
                    .await?;
            }

            Ok(wallet_id)
        })
    }

    /// Renames an existing wallet.
    ///
    /// Authorization: requires vault write access.
    pub async fn rename_wallet(
        &self,
        vault_id: &str,
        wallet_id: Uuid,
        new_name: &str,
        user_id: &str,
    ) -> ResultEngine<()> {
        let new_name = normalize_required_name(new_name, "wallet")?;
        with_tx!(self, |db_tx| {
            self.require_vault_by_id_write(&db_tx, vault_id, user_id)
                .await?;
            self.require_wallet_in_vault(&db_tx, vault_id, wallet_id)
                .await?;

            let exists = wallets::Entity::find()
                .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
                .filter(Expr::cust("LOWER(name)").eq(new_name.to_lowercase()))
                .filter(wallets::Column::Id.ne(wallet_id.to_string()))
                .one(&db_tx)
                .await?
                .is_some();
            if exists {
                return Err(EngineError::ExistingKey(new_name));
            }

            let active = wallets::ActiveModel {
                id: ActiveValue::Set(wallet_id.to_string()),
                name: ActiveValue::Set(new_name),
                ..Default::default()
            };
            active.update(&db_tx).await?;
            Ok(())
        })
    }

    /// Archives/unarchives an existing wallet.
    ///
    /// Authorization: requires vault write access.
    pub async fn set_wallet_archived(
        &self,
        vault_id: &str,
        wallet_id: Uuid,
        archived: bool,
        user_id: &str,
    ) -> ResultEngine<()> {
        with_tx!(self, |db_tx| {
            self.require_vault_by_id_write(&db_tx, vault_id, user_id)
                .await?;
            self.require_wallet_in_vault(&db_tx, vault_id, wallet_id)
                .await?;

            let active = wallets::ActiveModel {
                id: ActiveValue::Set(wallet_id.to_string()),
                archived: ActiveValue::Set(archived),
                ..Default::default()
            };
            active.update(&db_tx).await?;
            Ok(())
        })
    }
}
