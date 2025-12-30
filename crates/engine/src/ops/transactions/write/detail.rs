use uuid::Uuid;

use sea_orm::{QueryFilter, QueryOrder, TransactionTrait, prelude::*};

use crate::{
    EngineError, Leg, ResultEngine, Transaction, legs, transactions, vault, vault_memberships,
};

use super::super::super::{Engine, parse_vault_uuid, with_tx};

impl Engine {
    /// Returns a single transaction with all its legs (detail view).
    ///
    /// Authorization: requires vault read access.
    pub async fn transaction_with_legs(
        &self,
        vault_id: &str,
        transaction_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<Transaction> {
        with_tx!(self, |db_tx| {
            let vault_uuid = parse_vault_uuid(vault_id)?;
            let vault_model = vault::Entity::find_by_id(vault_uuid)
                .one(&db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
            if vault_model.user_id != user_id {
                let member =
                    vault_memberships::Entity::find_by_id((vault_uuid, user_id.to_string()))
                        .one(&db_tx)
                        .await?;
                if member.is_none() {
                    return Err(EngineError::Forbidden("forbidden".to_string()));
                }
            }

            let tx_model = transactions::Entity::find_by_id(transaction_id)
                .one(&db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("transaction not exists".to_string()))?;
            if tx_model.vault_id != vault_uuid {
                return Err(EngineError::KeyNotFound(
                    "transaction not exists".to_string(),
                ));
            }

            let mut tx = Transaction::try_from(tx_model)?;

            let leg_models: Vec<legs::Model> = legs::Entity::find()
                .filter(legs::Column::TransactionId.eq(transaction_id))
                .order_by_asc(legs::Column::Id)
                .all(&db_tx)
                .await?;
            let mut out = Vec::with_capacity(leg_models.len());
            for leg_model in leg_models {
                out.push(Leg::try_from(leg_model)?);
            }
            tx.legs = out;

            Ok(tx)
        })
    }
}
