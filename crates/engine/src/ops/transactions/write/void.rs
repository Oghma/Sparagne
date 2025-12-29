use chrono::{DateTime, Utc};
use uuid::Uuid;

use sea_orm::{ActiveValue, QueryFilter, TransactionTrait, prelude::*};

use crate::{legs, transactions, EngineError, Leg, LegTarget, ResultEngine};

use super::super::super::{parse_vault_currency, with_tx, Engine};

impl Engine {
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

            let (wallet_new_balances, flow_previews) = self
                .preview_apply_leg_updates(&db_tx, vault_id, vault_currency, &updates)
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

            Ok(())
        })
    }
}
