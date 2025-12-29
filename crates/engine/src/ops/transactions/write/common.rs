use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

use sea_orm::{ActiveValue, DatabaseTransaction, QueryFilter, TransactionTrait, prelude::*};

use crate::{
    cash_flows, legs, transactions, wallets, Currency, EngineError, Leg, LegTarget, ResultEngine,
    Transaction, TransactionKind, TxMeta,
};
use crate::util::{ensure_vault_currency, model_currency, validate_flow_mode_fields};

use super::super::helpers::{
    apply_transfer_leg_updates, normalize_tx_meta, parse_transfer_leg_pairs,
    resolve_transfer_targets, validate_flow_wallet_legs, validate_transfer_legs,
};
use super::super::super::{
    build_transaction, flow_wallet_legs, flow_wallet_signed_amount, parse_vault_currency, with_tx,
    Engine,
};

#[derive(Clone, Copy, Debug)]
pub(super) enum TransferTargetKind {
    Wallet,
    Flow,
}

impl Engine {
    async fn create_flow_wallet_transaction(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
        flow_id: Option<Uuid>,
        wallet_id: Option<Uuid>,
        amount_minor: i64,
        kind: TransactionKind,
        meta: TxMeta,
    ) -> ResultEngine<Uuid> {
        let (category, note) = normalize_tx_meta(&meta);
        let vault_model = self
            .require_vault_by_id_write(db_tx, vault_id, user_id)
            .await?;
        let currency = parse_vault_currency(vault_model.currency.as_str())?;
        let resolved_flow_id = self.resolve_flow_id(db_tx, vault_id, flow_id).await?;
        let resolved_wallet_id = self.resolve_wallet_id(db_tx, vault_id, wallet_id).await?;
        let leg_amount_minor = flow_wallet_signed_amount(kind, amount_minor)?;

        let tx = build_transaction(
            vault_id,
            kind,
            meta.occurred_at,
            amount_minor,
            currency,
            category,
            note,
            user_id,
            meta.idempotency_key,
            None,
        )?;
        let legs = flow_wallet_legs(
            tx.id,
            resolved_wallet_id,
            resolved_flow_id,
            leg_amount_minor,
            currency,
        );

        self.create_transaction_with_legs(db_tx, vault_id, currency, &tx, &legs)
            .await
    }

    pub(super) async fn create_flow_wallet_transaction_cmd(
        &self,
        vault_id: String,
        amount_minor: i64,
        flow_id: Option<Uuid>,
        wallet_id: Option<Uuid>,
        meta: TxMeta,
        user_id: String,
        kind: TransactionKind,
    ) -> ResultEngine<Uuid> {
        with_tx!(self, |db_tx| {
            let id = self
                .create_flow_wallet_transaction(
                    &db_tx,
                    &vault_id,
                    &user_id,
                    flow_id,
                    wallet_id,
                    amount_minor,
                    kind,
                    meta,
                )
                .await?;
            Ok(id)
        })
    }

    pub(super) async fn create_transfer_transaction(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
        amount_minor: i64,
        occurred_at: DateTime<Utc>,
        note: Option<String>,
        idempotency_key: Option<String>,
        kind: TransactionKind,
        currency: Currency,
        build_legs: impl FnOnce(Uuid) -> Vec<Leg>,
    ) -> ResultEngine<Uuid> {
        let tx = build_transaction(
            vault_id,
            kind,
            occurred_at,
            amount_minor,
            currency,
            None,
            note,
            user_id,
            idempotency_key,
            None,
        )?;
        let legs = build_legs(tx.id);
        self.create_transaction_with_legs(db_tx, vault_id, currency, &tx, &legs)
            .await
    }

    pub(super) async fn update_transfer_targets(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        leg_pairs: &[(legs::Model, Leg)],
        from_override: Option<Uuid>,
        to_override: Option<Uuid>,
        kind: TransferTargetKind,
        vault_currency: Currency,
        new_amount_minor: i64,
        balance_updates: &mut Vec<(LegTarget, i64, i64)>,
        leg_updates: &mut Vec<(String, LegTarget, i64)>,
    ) -> ResultEngine<()> {
        let (kind_label, target_label, diff_error) = match kind {
            TransferTargetKind::Wallet => (
                "transfer_wallet",
                "wallet",
                "from_wallet_id and to_wallet_id must differ",
            ),
            TransferTargetKind::Flow => (
                "transfer_flow",
                "flow",
                "from_flow_id and to_flow_id must differ",
            ),
        };

        let info = parse_transfer_leg_pairs(leg_pairs, kind_label, target_label, |target| match target
        {
            LegTarget::Wallet { wallet_id } => match kind {
                TransferTargetKind::Wallet => Some(*wallet_id),
                TransferTargetKind::Flow => None,
            },
            LegTarget::Flow { flow_id } => match kind {
                TransferTargetKind::Flow => Some(*flow_id),
                TransferTargetKind::Wallet => None,
            },
        })?;
        let (new_from, new_to) =
            resolve_transfer_targets(&info, from_override, to_override, diff_error)?;

        match kind {
            TransferTargetKind::Wallet => {
                self.require_wallet_in_vault(db_tx, vault_id, new_from)
                    .await?;
                self.require_wallet_in_vault(db_tx, vault_id, new_to)
                    .await?;
            }
            TransferTargetKind::Flow => {
                self.require_flow_in_vault(db_tx, vault_id, new_from)
                    .await?;
                self.require_flow_in_vault(db_tx, vault_id, new_to)
                    .await?;
            }
        }

        apply_transfer_leg_updates(
            leg_pairs,
            kind_label,
            vault_currency,
            info.from_leg_id,
            info.to_leg_id,
            new_from,
            new_to,
            new_amount_minor,
            |id| match kind {
                TransferTargetKind::Wallet => LegTarget::Wallet { wallet_id: id },
                TransferTargetKind::Flow => LegTarget::Flow { flow_id: id },
            },
            balance_updates,
            leg_updates,
        )?;

        Ok(())
    }

    async fn apply_wallet_delta(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        vault_currency: Currency,
        wallet_new_balances: &mut HashMap<Uuid, i64>,
        wallet_id: Uuid,
        delta_minor: i64,
    ) -> ResultEngine<()> {
        let wallet_model = wallets::Entity::find_by_id(wallet_id.to_string())
            .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
            .one(db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("wallet not exists".to_string()))?;
        let wallet_currency = model_currency(wallet_model.currency.as_str())?;
        ensure_vault_currency(vault_currency, wallet_currency)?;

        let entry = wallet_new_balances
            .entry(wallet_id)
            .or_insert(wallet_model.balance);
        *entry += delta_minor;
        Ok(())
    }

    async fn apply_flow_change(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        vault_currency: Currency,
        flow_previews: &mut HashMap<Uuid, crate::CashFlow>,
        flow_id: Uuid,
        old_amount_minor: i64,
        new_amount_minor: i64,
    ) -> ResultEngine<()> {
        let flow_model = cash_flows::Entity::find_by_id(flow_id.to_string())
            .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
            .one(db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;
        validate_flow_mode_fields(
            &flow_model.name,
            flow_model.max_balance,
            flow_model.income_balance,
        )?;
        let flow_currency = model_currency(flow_model.currency.as_str())?;
        ensure_vault_currency(vault_currency, flow_currency)?;
        let system_kind = flow_model
            .system_kind
            .as_deref()
            .and_then(|k| cash_flows::SystemFlowKind::try_from(k).ok());
        let entry = flow_previews.entry(flow_id).or_insert_with(|| crate::CashFlow {
            id: flow_id,
            name: flow_model.name.clone(),
            system_kind,
            balance: flow_model.balance,
            max_balance: flow_model.max_balance,
            income_balance: flow_model.income_balance,
            currency: flow_currency,
            archived: flow_model.archived,
        });
        entry.apply_leg_change(old_amount_minor, new_amount_minor)?;
        Ok(())
    }

    pub(in crate::ops) async fn create_transaction_with_legs(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        vault_currency: Currency,
        tx: &Transaction,
        legs: &[Leg],
    ) -> ResultEngine<Uuid> {
        if legs.is_empty() {
            return Err(EngineError::InvalidAmount(
                "transaction must have at least one leg".to_string(),
            ));
        }
        for leg in legs {
            if leg.transaction_id != tx.id {
                return Err(EngineError::InvalidAmount(
                    "invalid leg: transaction_id mismatch".to_string(),
                ));
            }
            if leg.amount_minor == 0 {
                return Err(EngineError::InvalidAmount(
                    "invalid leg: amount_minor must not be 0".to_string(),
                ));
            }
        }

        // Validate kind-specific invariants (kept strict for now).
        match tx.kind {
            TransactionKind::Income | TransactionKind::Expense | TransactionKind::Refund => {
                validate_flow_wallet_legs(tx.kind, tx.amount_minor, legs)?;
            }
            TransactionKind::TransferWallet => {
                validate_transfer_legs(
                    legs,
                    tx.amount_minor,
                    "transfer",
                    "transfer_wallet",
                    "wallet",
                    |target| match target {
                        LegTarget::Wallet { wallet_id } => Some(*wallet_id),
                        _ => None,
                    },
                )?;
            }
            TransactionKind::TransferFlow => {
                validate_transfer_legs(
                    legs,
                    tx.amount_minor,
                    "transfer",
                    "transfer_flow",
                    "flow",
                    |target| match target {
                        LegTarget::Flow { flow_id } => Some(*flow_id),
                        _ => None,
                    },
                )?;
            }
        }

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

        // Validate currency and domain invariants by simulating balance changes, while
        // also computing the resulting denormalized balances to persist.
        let mut updates: Vec<(LegTarget, i64, i64)> = Vec::with_capacity(legs.len());
        for leg in legs {
            if leg.currency != vault_currency {
                return Err(EngineError::CurrencyMismatch(format!(
                    "vault currency is {}, got {}",
                    vault_currency.code(),
                    leg.currency.code()
                )));
            }
            updates.push((leg.target, 0, leg.amount_minor));
        }
        let (wallet_new_balances, flow_previews) = self
            .preview_apply_leg_updates(db_tx, vault_id, vault_currency, &updates)
            .await?;

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

        self.persist_targets(db_tx, wallet_new_balances, flow_previews)
            .await?;

        Ok(tx.id)
    }

    pub(super) async fn preview_apply_leg_updates(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        vault_currency: Currency,
        updates: &[(LegTarget, i64, i64)],
    ) -> ResultEngine<(HashMap<Uuid, i64>, HashMap<Uuid, crate::CashFlow>)> {
        let mut wallet_new_balances: HashMap<Uuid, i64> = HashMap::new();
        let mut flow_previews: HashMap<Uuid, crate::CashFlow> = HashMap::new();

        for (target, old_amount_minor, new_amount_minor) in updates {
            match *target {
                LegTarget::Wallet { wallet_id } => {
                    let delta_minor = *new_amount_minor - *old_amount_minor;
                    self.apply_wallet_delta(
                        db_tx,
                        vault_id,
                        vault_currency,
                        &mut wallet_new_balances,
                        wallet_id,
                        delta_minor,
                    )
                    .await?;
                }
                LegTarget::Flow { flow_id } => {
                    self.apply_flow_change(
                        db_tx,
                        vault_id,
                        vault_currency,
                        &mut flow_previews,
                        flow_id,
                        *old_amount_minor,
                        *new_amount_minor,
                    )
                    .await?;
                }
            }
        }

        Ok((wallet_new_balances, flow_previews))
    }

    pub(super) async fn persist_targets(
        &self,
        db_tx: &DatabaseTransaction,
        wallet_new_balances: HashMap<Uuid, i64>,
        flow_previews: HashMap<Uuid, crate::CashFlow>,
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
}
