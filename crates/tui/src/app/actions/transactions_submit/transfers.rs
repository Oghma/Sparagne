use crate::{
    app::{
        App, ToastLevel, TransactionsMode, TransferFormState,
    },
    error::Result,
    text::{TextKey, t},
};
use api_types::transaction::{TransactionUpdate, TransferFlowNew, TransferWalletNew};

impl App {
    pub(crate) async fn submit_transfer_wallet(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let editing_id = self.state.transactions.transfer.editing_id;

        // Validate minimum count
        let ids = self.active_wallet_ids();
        if let Err(message) = super::super::validate_minimum_count(ids.len(), super::super::TransferType::Wallet, self.state.locale) {
            self.state.transactions.transfer.error = Some(message);
            return Ok(());
        }

        // Get and validate IDs
        let from_id = ids[self.state.transactions.transfer.from_index];
        let to_id = ids[self.state.transactions.transfer.to_index];
        if super::super::validate_different_ids(from_id, to_id, self.state.locale).is_err() {
            self.state.transactions.transfer.error = Some(t(self.state.locale, TextKey::ValidationTransferSameSource).to_string());
            return Ok(());
        }

        // Validate amount
        let currency = self.current_currency();
        let amount = match super::super::validate_transfer_amount(
            self.state.transactions.transfer.amount.value().trim(),
            currency,
            self.state.locale,
        ) {
            Ok(amount) => amount,
            Err(message) => {
                self.state.transactions.transfer.error = Some(message);
                return Ok(());
            }
        };

        // Parse occurred_at
        let note = self.state.transactions.transfer.note.value().trim();
        let occurred_raw = self.state.transactions.transfer.occurred_at.value.trim();
        let occurred_at = if occurred_raw.is_empty() {
            None
        } else {
            match self.parse_local_datetime(occurred_raw) {
                Ok(dt) => Some(dt),
                Err(message) => {
                    self.state.transactions.transfer.error = Some(message);
                    return Ok(());
                }
            }
        };
        let occurred_at_new = occurred_at.unwrap_or_else(|| self.now_in_timezone());

        if let Some(transaction_id) = editing_id {
            let res = self
                .client
                .transaction_update(
                    transaction_id,
                    TransactionUpdate {
                        vault_id,
                        amount_minor: Some(amount),
                        wallet_id: None,
                        flow_id: None,
                        from_wallet_id: Some(from_id),
                        to_wallet_id: Some(to_id),
                        from_flow_id: None,
                        to_flow_id: None,
                        category_id: None,
                        category: None,
                        note: Some(note.to_string()),
                        occurred_at,
                    },
                )
                .await;

            match res {
                Ok(()) => {
                    self.state.transactions.transfer = TransferFormState::default();
                    self.set_toast(t(self.state.locale, TextKey::SuccessTransferWalletUpdated), ToastLevel::Success);
                    self.load_transactions(true).await?;
                    self.open_transaction_detail_by_id(transaction_id).await?;
                }
                Err(err) => {
                    let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorTransferWallet) else { return Ok(()); };
                    self.state.transactions.transfer.error = Some(msg);
                }
            }
        } else {
            let res = self
                .client
                .transfer_wallet_new(
                    TransferWalletNew {
                        vault_id,
                        amount_minor: amount,
                        from_wallet_id: from_id,
                        to_wallet_id: to_id,
                        note: if note.is_empty() {
                            None
                        } else {
                            Some(note.to_string())
                        },
                        idempotency_key: None,
                        occurred_at: occurred_at_new,
                    },
                )
                .await;

            match res {
                Ok(created) => {
                    self.state.transactions.mode = TransactionsMode::List;
                    self.state.transactions.transfer = TransferFormState::default();
                    self.state.transactions.last_created_id = Some(created.id);
                    self.set_toast(t(self.state.locale, TextKey::SuccessTransferWalletSaved), ToastLevel::Success);
                    self.load_transactions(true).await?;
                }
                Err(err) => {
                    let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorTransferWallet) else { return Ok(()); };
                    self.state.transactions.transfer.error = Some(msg);
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn submit_transfer_flow(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let editing_id = self.state.transactions.transfer.editing_id;

        // Validate minimum count
        let ids = self.active_flow_ids();
        if let Err(message) = super::super::validate_minimum_count(ids.len(), super::super::TransferType::Flow, self.state.locale) {
            self.state.transactions.transfer.error = Some(message);
            return Ok(());
        }

        // Get and validate IDs
        let from_id = ids[self.state.transactions.transfer.from_index];
        let to_id = ids[self.state.transactions.transfer.to_index];
        if super::super::validate_different_ids(from_id, to_id, self.state.locale).is_err() {
            self.state.transactions.transfer.error = Some(t(self.state.locale, TextKey::ValidationTransferSameDestination).to_string());
            return Ok(());
        }

        // Validate amount
        let currency = self.current_currency();
        let amount = match super::super::validate_transfer_amount(
            self.state.transactions.transfer.amount.value().trim(),
            currency,
            self.state.locale,
        ) {
            Ok(amount) => amount,
            Err(message) => {
                self.state.transactions.transfer.error = Some(message);
                return Ok(());
            }
        };

        // Parse occurred_at
        let note = self.state.transactions.transfer.note.value().trim();
        let occurred_raw = self.state.transactions.transfer.occurred_at.value.trim();
        let occurred_at = if occurred_raw.is_empty() {
            None
        } else {
            match self.parse_local_datetime(occurred_raw) {
                Ok(dt) => Some(dt),
                Err(message) => {
                    self.state.transactions.transfer.error = Some(message);
                    return Ok(());
                }
            }
        };
        let occurred_at_new = occurred_at.unwrap_or_else(|| self.now_in_timezone());

        if let Some(transaction_id) = editing_id {
            let res = self
                .client
                .transaction_update(
                    transaction_id,
                    TransactionUpdate {
                        vault_id,
                        amount_minor: Some(amount),
                        wallet_id: None,
                        flow_id: None,
                        from_wallet_id: None,
                        to_wallet_id: None,
                        from_flow_id: Some(from_id),
                        to_flow_id: Some(to_id),
                        category_id: None,
                        category: None,
                        note: Some(note.to_string()),
                        occurred_at,
                    },
                )
                .await;

            match res {
                Ok(()) => {
                    self.state.transactions.transfer = TransferFormState::default();
                    self.set_toast(t(self.state.locale, TextKey::SuccessTransferFlowUpdated), ToastLevel::Success);
                    self.load_transactions(true).await?;
                    self.open_transaction_detail_by_id(transaction_id).await?;
                }
                Err(err) => {
                    let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorTransferFlow) else { return Ok(()); };
                    self.state.transactions.transfer.error = Some(msg);
                }
            }
        } else {
            let res = self
                .client
                .transfer_flow_new(
                    TransferFlowNew {
                        vault_id,
                        amount_minor: amount,
                        from_flow_id: from_id,
                        to_flow_id: to_id,
                        note: if note.is_empty() {
                            None
                        } else {
                            Some(note.to_string())
                        },
                        idempotency_key: None,
                        occurred_at: occurred_at_new,
                    },
                )
                .await;

            match res {
                Ok(created) => {
                    self.state.transactions.mode = TransactionsMode::List;
                    self.state.transactions.transfer = TransferFormState::default();
                    self.state.transactions.last_created_id = Some(created.id);
                    self.set_toast(t(self.state.locale, TextKey::SuccessTransferFlowSaved), ToastLevel::Success);
                    self.load_transactions(true).await?;
                }
                Err(err) => {
                    let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorTransferFlow) else { return Ok(()); };
                    self.state.transactions.transfer.error = Some(msg);
                }
            }
        }

        Ok(())
    }
}
