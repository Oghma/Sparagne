use sea_orm::{ActiveValue, QueryFilter, TransactionTrait, prelude::*};

use crate::{
    legs, transactions, EngineError, Leg, LegTarget, ResultEngine, TransactionKind,
    UpdateTransactionCmd,
};

use super::common::TransferTargetKind;
use super::super::helpers::{
    apply_flow_wallet_leg_updates, apply_optional_datetime_patch, apply_optional_text_patch,
    extract_flow_wallet_targets, validate_update_fields,
};
use super::super::super::{flow_wallet_signed_amount, parse_vault_currency, with_tx, Engine};

impl Engine {
    /// Updates an existing transaction (amount, metadata, and/or targets).
    ///
    /// The allowed target edits depend on the transaction kind:
    /// - `Income`/`Expense`/`Refund`: wallet and/or flow can be changed
    /// - `TransferWallet`: from/to wallets can be changed
    /// - `TransferFlow`: from/to flows can be changed
    pub async fn update_transaction(&self, cmd: UpdateTransactionCmd) -> ResultEngine<()> {
        let vault_id = cmd.vault_id;
        let vault_id = vault_id.as_str();
        let transaction_id = cmd.transaction_id;
        let user_id = cmd.user_id;
        let user_id = user_id.as_str();
        let amount_minor = cmd.amount_minor;
        let wallet_id = cmd.wallet_id;
        let flow_id = cmd.flow_id;
        let from_wallet_id = cmd.from_wallet_id;
        let to_wallet_id = cmd.to_wallet_id;
        let from_flow_id = cmd.from_flow_id;
        let to_flow_id = cmd.to_flow_id;
        let category = cmd.category.as_deref();
        let note = cmd.note.as_deref();
        let occurred_at = cmd.occurred_at;
        with_tx!(self, |db_tx| {
            let vault_model = self
                .require_vault_by_id_write(&db_tx, vault_id, user_id)
                .await?;
            let vault_currency = parse_vault_currency(vault_model.currency.as_str())?;

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
            let new_amount_minor = amount_minor.unwrap_or(tx_model.amount_minor);
            if new_amount_minor <= 0 {
                return Err(EngineError::InvalidAmount(
                    "amount_minor must be > 0".to_string(),
                ));
            }

            let new_occurred_at = apply_optional_datetime_patch(tx_model.occurred_at, occurred_at);
            let new_category = apply_optional_text_patch(tx_model.category.clone(), category);
            let new_note = apply_optional_text_patch(tx_model.note.clone(), note);

            let leg_models = legs::Entity::find()
                .filter(legs::Column::TransactionId.eq(transaction_id.to_string()))
                .all(&db_tx)
                .await?;

            let mut leg_pairs: Vec<(legs::Model, Leg)> = Vec::with_capacity(leg_models.len());
            for leg_model in leg_models {
                let leg = Leg::try_from(leg_model.clone())?;
                leg_pairs.push((leg_model, leg));
            }

            let mut balance_updates: Vec<(LegTarget, i64, i64)> = Vec::new();
            let mut leg_updates: Vec<(String, LegTarget, i64)> = Vec::new();

            match kind {
                TransactionKind::Income | TransactionKind::Expense | TransactionKind::Refund => {
                    let (existing_wallet_id, existing_flow_id) =
                        extract_flow_wallet_targets(&leg_pairs)?;
                    let new_wallet_id = wallet_id.unwrap_or(existing_wallet_id);
                    let new_flow_id = flow_id.unwrap_or(existing_flow_id);
                    self.require_wallet_in_vault(&db_tx, vault_id, new_wallet_id)
                        .await?;
                    self.require_flow_in_vault(&db_tx, vault_id, new_flow_id)
                        .await?;

                    let new_signed_amount = flow_wallet_signed_amount(kind, new_amount_minor)?;

                    apply_flow_wallet_leg_updates(
                        &leg_pairs,
                        vault_currency,
                        new_wallet_id,
                        new_flow_id,
                        new_signed_amount,
                        &mut balance_updates,
                        &mut leg_updates,
                    )?;
                }
                TransactionKind::TransferWallet => {
                    self.update_transfer_targets(
                        &db_tx,
                        vault_id,
                        &leg_pairs,
                        from_wallet_id,
                        to_wallet_id,
                        TransferTargetKind::Wallet,
                        vault_currency,
                        new_amount_minor,
                        &mut balance_updates,
                        &mut leg_updates,
                    )
                    .await?;
                }
                TransactionKind::TransferFlow => {
                    self.update_transfer_targets(
                        &db_tx,
                        vault_id,
                        &leg_pairs,
                        from_flow_id,
                        to_flow_id,
                        TransferTargetKind::Flow,
                        vault_currency,
                        new_amount_minor,
                        &mut balance_updates,
                        &mut leg_updates,
                    )
                    .await?;
                }
            }

            // Reject unexpected target fields for this kind (avoid silent no-ops).
            validate_update_fields(
                kind,
                wallet_id,
                flow_id,
                from_wallet_id,
                to_wallet_id,
                from_flow_id,
                to_flow_id,
            )?;

            let (wallet_new_balances, flow_previews) = self
                .preview_apply_leg_updates(&db_tx, vault_id, vault_currency, &balance_updates)
                .await?;

            let tx_active = transactions::ActiveModel {
                id: ActiveValue::Set(transaction_id.to_string()),
                amount_minor: ActiveValue::Set(new_amount_minor),
                category: ActiveValue::Set(new_category),
                note: ActiveValue::Set(new_note),
                occurred_at: ActiveValue::Set(new_occurred_at),
                ..Default::default()
            };
            tx_active.update(&db_tx).await?;

            for (leg_id, new_target, new_amount_minor) in leg_updates {
                let (target_kind, target_id) = match new_target {
                    LegTarget::Wallet { wallet_id } => ("wallet".to_string(), wallet_id.to_string()),
                    LegTarget::Flow { flow_id } => ("flow".to_string(), flow_id.to_string()),
                };
                let leg_active = legs::ActiveModel {
                    id: ActiveValue::Set(leg_id),
                    target_kind: ActiveValue::Set(target_kind),
                    target_id: ActiveValue::Set(target_id),
                    amount_minor: ActiveValue::Set(new_amount_minor),
                    ..Default::default()
                };
                leg_active.update(&db_tx).await?;
            }

            self.persist_targets(&db_tx, wallet_new_balances, flow_previews)
                .await?;

            Ok(())
        })
    }
}
