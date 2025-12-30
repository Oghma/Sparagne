use uuid::Uuid;

use sea_orm::EntityTrait;

use crate::{
    EngineError, ResultEngine, TransactionKind, TransferFlowCmd, TransferWalletCmd,
    util::normalize_optional_text, vault,
};

use super::{
    super::super::{Engine, parse_vault_uuid, transfer_flow_legs, transfer_wallet_legs},
    common::TransferTransactionInput,
};

impl Engine {
    pub async fn transfer_wallet(&self, cmd: TransferWalletCmd) -> ResultEngine<Uuid> {
        if cmd.from_wallet_id == cmd.to_wallet_id {
            return Err(EngineError::InvalidAmount(
                "from_wallet_id and to_wallet_id must differ".to_string(),
            ));
        }
        let TransferWalletCmd {
            vault_id,
            amount_minor,
            from_wallet_id,
            to_wallet_id,
            note,
            idempotency_key,
            occurred_at,
            user_id,
        } = cmd;
        let note = normalize_optional_text(note.as_deref());
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                let vault_model = engine
                    .require_vault_by_id_write(db_tx, &vault_id, &user_id)
                    .await?;
                let currency = vault_model.currency;
                // Ensure wallets belong to the vault.
                engine
                    .resolve_wallet_id(db_tx, &vault_id, Some(from_wallet_id))
                    .await?;
                engine
                    .resolve_wallet_id(db_tx, &vault_id, Some(to_wallet_id))
                    .await?;

                let id = engine
                    .create_transfer_transaction(
                        db_tx,
                        TransferTransactionInput {
                            vault_id: &vault_id,
                            user_id: &user_id,
                            amount_minor,
                            occurred_at,
                            note,
                            idempotency_key,
                            kind: TransactionKind::TransferWallet,
                            currency,
                        },
                        |tx_id| {
                            transfer_wallet_legs(
                                tx_id,
                                from_wallet_id,
                                to_wallet_id,
                                amount_minor,
                                currency,
                            )
                        },
                    )
                    .await?;
                Ok(id)
            })
        })
        .await
    }

    pub async fn transfer_flow(&self, cmd: TransferFlowCmd) -> ResultEngine<Uuid> {
        if cmd.from_flow_id == cmd.to_flow_id {
            return Err(EngineError::InvalidAmount(
                "from_flow_id and to_flow_id must differ".to_string(),
            ));
        }
        let TransferFlowCmd {
            vault_id,
            amount_minor,
            from_flow_id,
            to_flow_id,
            note,
            idempotency_key,
            occurred_at,
            user_id,
        } = cmd;
        let note = normalize_optional_text(note.as_deref());
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                let vault_uuid = parse_vault_uuid(&vault_id)?;
                let vault_model = vault::Entity::find_by_id(vault_uuid)
                    .one(db_tx)
                    .await?
                    .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
                let currency = vault_model.currency;
                // AuthZ:
                // - Vault owner/editor can transfer between any flows in the vault.
                // - Otherwise, user must be editor/owner on both flows (via flow_memberships).
                if engine
                    .has_vault_write_access(db_tx, &vault_id, &user_id)
                    .await?
                {
                    engine
                        .resolve_flow_id(db_tx, &vault_id, Some(from_flow_id))
                        .await?;
                    engine
                        .resolve_flow_id(db_tx, &vault_id, Some(to_flow_id))
                        .await?;
                } else {
                    engine
                        .require_flow_write(db_tx, &vault_id, from_flow_id, &user_id)
                        .await?;
                    engine
                        .require_flow_write(db_tx, &vault_id, to_flow_id, &user_id)
                        .await?;
                }

                let id = engine
                    .create_transfer_transaction(
                        db_tx,
                        TransferTransactionInput {
                            vault_id: &vault_id,
                            user_id: &user_id,
                            amount_minor,
                            occurred_at,
                            note,
                            idempotency_key,
                            kind: TransactionKind::TransferFlow,
                            currency,
                        },
                        |tx_id| {
                            transfer_flow_legs(
                                tx_id,
                                from_flow_id,
                                to_flow_id,
                                amount_minor,
                                currency,
                            )
                        },
                    )
                    .await?;
                Ok(id)
            })
        })
        .await
    }
}
