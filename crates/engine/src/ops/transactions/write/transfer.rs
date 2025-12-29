use uuid::Uuid;

use sea_orm::{EntityTrait, TransactionTrait};

use crate::{vault, EngineError, ResultEngine, TransactionKind, TransferFlowCmd, TransferWalletCmd};

use super::super::super::{
    normalize_optional_text, parse_vault_currency, transfer_flow_legs, transfer_wallet_legs,
    with_tx, Engine,
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
        with_tx!(self, |db_tx| {
            let vault_model = self
                .require_vault_by_id_write(&db_tx, &vault_id, &user_id)
                .await?;
            let currency = parse_vault_currency(vault_model.currency.as_str())?;
            // Ensure wallets belong to the vault.
            self.resolve_wallet_id(&db_tx, &vault_id, Some(from_wallet_id))
                .await?;
            self.resolve_wallet_id(&db_tx, &vault_id, Some(to_wallet_id))
                .await?;

            let id = self
                .create_transfer_transaction(
                    &db_tx,
                    &vault_id,
                    &user_id,
                    amount_minor,
                    occurred_at,
                    note,
                    idempotency_key,
                    TransactionKind::TransferWallet,
                    currency,
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
        with_tx!(self, |db_tx| {
            let vault_model = vault::Entity::find_by_id(vault_id.to_string())
                .one(&db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
            let currency = parse_vault_currency(vault_model.currency.as_str())?;
            // AuthZ:
            // - Vault owner/editor can transfer between any flows in the vault.
            // - Otherwise, user must be editor/owner on both flows (via flow_memberships).
            if self
                .has_vault_write_access(&db_tx, &vault_id, &user_id)
                .await?
            {
                self.resolve_flow_id(&db_tx, &vault_id, Some(from_flow_id))
                    .await?;
                self.resolve_flow_id(&db_tx, &vault_id, Some(to_flow_id))
                    .await?;
            } else {
                self.require_flow_write(&db_tx, &vault_id, from_flow_id, &user_id)
                    .await?;
                self.require_flow_write(&db_tx, &vault_id, to_flow_id, &user_id)
                    .await?;
            }

            let id = self
                .create_transfer_transaction(
                    &db_tx,
                    &vault_id,
                    &user_id,
                    amount_minor,
                    occurred_at,
                    note,
                    idempotency_key,
                    TransactionKind::TransferFlow,
                    currency,
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
    }
}
