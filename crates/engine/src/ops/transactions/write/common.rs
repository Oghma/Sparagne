use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

use sea_orm::{ActiveValue, DatabaseTransaction, QueryFilter, TransactionTrait, prelude::*};

use crate::{
    Currency, EngineError, Leg, LegTarget, ResultEngine, Transaction, TransactionKind, TxMeta,
    cash_flows, legs, transactions,
    util::{ensure_vault_currency, model_currency, validate_flow_mode_fields},
    wallets,
};

use super::super::{
    super::{
        Engine, TransactionBuildInput, build_transaction, flow_wallet_legs,
        flow_wallet_signed_amount, parse_vault_currency, parse_vault_uuid, with_tx,
    },
    helpers::{
        apply_transfer_leg_updates, normalize_tx_meta, parse_transfer_leg_pairs,
        resolve_transfer_targets, validate_flow_wallet_legs, validate_transfer_legs,
    },
};

#[derive(Clone, Copy, Debug)]
pub(super) enum TransferTargetKind {
    Wallet,
    Flow,
}

pub(super) struct FlowWalletCmd {
    pub(super) vault_id: String,
    pub(super) amount_minor: i64,
    pub(super) flow_id: Option<Uuid>,
    pub(super) wallet_id: Option<Uuid>,
    pub(super) meta: TxMeta,
    pub(super) user_id: String,
    pub(super) kind: TransactionKind,
}

pub(super) struct TransferTransactionInput<'a> {
    pub(super) vault_id: &'a str,
    pub(super) user_id: &'a str,
    pub(super) amount_minor: i64,
    pub(super) occurred_at: DateTime<Utc>,
    pub(super) note: Option<String>,
    pub(super) idempotency_key: Option<String>,
    pub(super) kind: TransactionKind,
    pub(super) currency: Currency,
}

pub(super) struct TransferUpdateInput<'a> {
    pub(super) db_tx: &'a DatabaseTransaction,
    pub(super) vault_id: &'a str,
    pub(super) leg_pairs: &'a [(legs::Model, Leg)],
    pub(super) from_override: Option<Uuid>,
    pub(super) to_override: Option<Uuid>,
    pub(super) kind: TransferTargetKind,
    pub(super) vault_currency: Currency,
    pub(super) new_amount_minor: i64,
}

pub(super) struct TransferUpdateOutput<'a> {
    pub(super) balance_updates: &'a mut Vec<(LegTarget, i64, i64)>,
    pub(super) leg_updates: &'a mut Vec<(Uuid, LegTarget, i64)>,
}

struct FlowChangeInput<'a> {
    db_tx: &'a DatabaseTransaction,
    vault_id: &'a str,
    vault_currency: Currency,
    flow_previews: &'a mut HashMap<Uuid, crate::CashFlow>,
    flow_id: Uuid,
    old_amount_minor: i64,
    new_amount_minor: i64,
}

impl Engine {
    pub(super) async fn create_flow_wallet_transaction_cmd(
        &self,
        cmd: FlowWalletCmd,
    ) -> ResultEngine<Uuid> {
        with_tx!(self, |db_tx| {
            let (category, note) = normalize_tx_meta(&cmd.meta);
            let vault_model = self
                .require_vault_by_id_write(&db_tx, &cmd.vault_id, &cmd.user_id)
                .await?;
            let currency = parse_vault_currency(vault_model.currency.as_str())?;
            let resolved_flow_id = self
                .resolve_flow_id(&db_tx, &cmd.vault_id, cmd.flow_id)
                .await?;
            let resolved_wallet_id = self
                .resolve_wallet_id(&db_tx, &cmd.vault_id, cmd.wallet_id)
                .await?;
            let leg_amount_minor = flow_wallet_signed_amount(cmd.kind, cmd.amount_minor)?;

            let tx = build_transaction(TransactionBuildInput {
                vault_id: &cmd.vault_id,
                kind: cmd.kind,
                occurred_at: cmd.meta.occurred_at,
                amount_minor: cmd.amount_minor,
                currency,
                category,
                note,
                created_by: &cmd.user_id,
                idempotency_key: cmd.meta.idempotency_key.clone(),
                refunded_transaction_id: None,
            })?;
            let legs = flow_wallet_legs(
                tx.id,
                resolved_wallet_id,
                resolved_flow_id,
                leg_amount_minor,
                currency,
            );

            self.create_transaction_with_legs(&db_tx, &cmd.vault_id, currency, &tx, &legs)
                .await
        })
    }

    pub(super) async fn create_transfer_transaction(
        &self,
        db_tx: &DatabaseTransaction,
        input: TransferTransactionInput<'_>,
        build_legs: impl FnOnce(Uuid) -> Vec<Leg>,
    ) -> ResultEngine<Uuid> {
        let tx = build_transaction(TransactionBuildInput {
            vault_id: input.vault_id,
            kind: input.kind,
            occurred_at: input.occurred_at,
            amount_minor: input.amount_minor,
            currency: input.currency,
            category: None,
            note: input.note,
            created_by: input.user_id,
            idempotency_key: input.idempotency_key,
            refunded_transaction_id: None,
        })?;
        let legs = build_legs(tx.id);
        self.create_transaction_with_legs(db_tx, input.vault_id, input.currency, &tx, &legs)
            .await
    }

    pub(super) async fn update_transfer_targets(
        &self,
        input: TransferUpdateInput<'_>,
        output: TransferUpdateOutput<'_>,
    ) -> ResultEngine<()> {
        let (kind_label, target_label, diff_error) = match input.kind {
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

        let info = parse_transfer_leg_pairs(input.leg_pairs, kind_label, target_label, |target| {
            match target {
                LegTarget::Wallet { wallet_id } => match input.kind {
                    TransferTargetKind::Wallet => Some(*wallet_id),
                    TransferTargetKind::Flow => None,
                },
                LegTarget::Flow { flow_id } => match input.kind {
                    TransferTargetKind::Flow => Some(*flow_id),
                    TransferTargetKind::Wallet => None,
                },
            }
        })?;
        let (new_from, new_to) =
            resolve_transfer_targets(&info, input.from_override, input.to_override, diff_error)?;

        match input.kind {
            TransferTargetKind::Wallet => {
                self.require_wallet_in_vault(input.db_tx, input.vault_id, new_from)
                    .await?;
                self.require_wallet_in_vault(input.db_tx, input.vault_id, new_to)
                    .await?;
            }
            TransferTargetKind::Flow => {
                self.require_flow_in_vault(input.db_tx, input.vault_id, new_from)
                    .await?;
                self.require_flow_in_vault(input.db_tx, input.vault_id, new_to)
                    .await?;
            }
        }

        let ctx = super::super::helpers::TransferLegUpdateContext {
            kind_label,
            vault_currency: input.vault_currency,
            from_leg_id: info.from_leg_id,
            to_leg_id: info.to_leg_id,
            new_from,
            new_to,
            new_amount_minor: input.new_amount_minor,
        };
        let sink = super::super::helpers::TransferLegUpdateSink {
            balance_updates: output.balance_updates,
            leg_updates: output.leg_updates,
        };
        apply_transfer_leg_updates(input.leg_pairs, ctx, sink, |id| match input.kind {
            TransferTargetKind::Wallet => LegTarget::Wallet { wallet_id: id },
            TransferTargetKind::Flow => LegTarget::Flow { flow_id: id },
        })?;

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
        let vault_uuid = parse_vault_uuid(vault_id)?;
        let wallet_model = wallets::Entity::find_by_id(wallet_id)
            .filter(wallets::Column::VaultId.eq(vault_uuid))
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

    async fn apply_flow_change(&self, input: FlowChangeInput<'_>) -> ResultEngine<()> {
        let vault_uuid = parse_vault_uuid(input.vault_id)?;
        let flow_model = cash_flows::Entity::find_by_id(input.flow_id)
            .filter(cash_flows::Column::VaultId.eq(vault_uuid))
            .one(input.db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;
        validate_flow_mode_fields(
            &flow_model.name,
            flow_model.max_balance,
            flow_model.income_balance,
        )?;
        let flow_currency = model_currency(flow_model.currency.as_str())?;
        ensure_vault_currency(input.vault_currency, flow_currency)?;
        let entry = input
            .flow_previews
            .entry(input.flow_id)
            .or_insert_with(|| crate::CashFlow {
                id: input.flow_id,
                name: flow_model.name.clone(),
                system_kind: flow_model.system_kind,
                balance: flow_model.balance,
                max_balance: flow_model.max_balance,
                income_balance: flow_model.income_balance,
                currency: flow_currency,
                archived: flow_model.archived,
            });
        entry.apply_leg_change(input.old_amount_minor, input.new_amount_minor)?;
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

        let vault_uuid_early = parse_vault_uuid(vault_id)?;
        if let Some(key) = tx.idempotency_key.as_deref() {
            let existing = transactions::Entity::find()
                .filter(transactions::Column::VaultId.eq(vault_uuid_early))
                .filter(transactions::Column::CreatedBy.eq(tx.created_by.clone()))
                .filter(transactions::Column::IdempotencyKey.eq(key.to_string()))
                .one(db_tx)
                .await?;
            if let Some(existing) = existing {
                return Ok(existing.id);
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

        let vault_uuid = parse_vault_uuid(vault_id)?;
        if let Err(err) = transactions::ActiveModel::from(tx).insert(db_tx).await {
            if tx.idempotency_key.is_some() {
                let key = tx.idempotency_key.as_deref().unwrap_or_default();
                let existing = transactions::Entity::find()
                    .filter(transactions::Column::VaultId.eq(vault_uuid))
                    .filter(transactions::Column::CreatedBy.eq(tx.created_by.clone()))
                    .filter(transactions::Column::IdempotencyKey.eq(key.to_string()))
                    .one(db_tx)
                    .await?;
                if let Some(existing) = existing {
                    return Ok(existing.id);
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
                    self.apply_flow_change(FlowChangeInput {
                        db_tx,
                        vault_id,
                        vault_currency,
                        flow_previews: &mut flow_previews,
                        flow_id,
                        old_amount_minor: *old_amount_minor,
                        new_amount_minor: *new_amount_minor,
                    })
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
                id: ActiveValue::Set(wallet_id),
                balance: ActiveValue::Set(new_balance),
                ..Default::default()
            };
            wallet_model.update(db_tx).await?;
        }

        for (flow_id, flow) in flow_previews {
            let flow_model = cash_flows::ActiveModel {
                id: ActiveValue::Set(flow_id),
                balance: ActiveValue::Set(flow.balance),
                income_balance: ActiveValue::Set(flow.income_balance),
                ..Default::default()
            };
            flow_model.update(db_tx).await?;
        }

        Ok(())
    }
}
