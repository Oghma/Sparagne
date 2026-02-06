use crate::{
    app::{
        App, ToastLevel, TransactionFormState, TransactionsMode,
    },
    error::Result,
    text::{TextKey, t},
};
use api_types::transaction::{ExpenseNew, IncomeNew, Refund, TransactionKind, TransactionUpdate};
use engine::Money;

impl App {
    pub(crate) async fn submit_transaction_form(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let currency = self.current_currency();
        self.state.transactions.form.error = None;
        let (kind, amount_raw, wallet_index, flow_index, category_raw, note_raw, occurred_raw) = {
            let form = &self.state.transactions.form;
            (
                form.kind,
                form.amount.value().trim().to_string(),
                form.wallet_index,
                form.flow_index,
                form.category.value().trim().to_string(),
                form.note.value().trim().to_string(),
                form.occurred_at.value.trim().to_string(),
            )
        };
        let editing_id = self.state.transactions.form.editing_id;

        let amount_raw = amount_raw.as_str();
        if amount_raw.is_empty() {
            self.set_transaction_form_error(t(self.state.locale, TextKey::ValidationAmountRequired));
            return Ok(());
        }
        let amount_minor = match Money::parse_major(amount_raw, currency) {
            Ok(money) => money.minor().abs(),
            Err(_) => {
                self.set_transaction_form_error(t(self.state.locale, TextKey::ValidationAmountInvalid));
                return Ok(());
            }
        };
        if amount_minor <= 0 {
            self.set_transaction_form_error(t(self.state.locale, TextKey::ValidationAmountPositive));
            return Ok(());
        }

        let wallet_ids = self.ordered_wallet_ids();
        if wallet_ids.is_empty() {
            self.set_transaction_form_error(t(self.state.locale, TextKey::StateNoWalletAvailable));
            return Ok(());
        }
        let wallet_id = match wallet_ids.get(wallet_index) {
            Some(id) => *id,
            None => {
                self.set_transaction_form_error(t(self.state.locale, TextKey::ValidationWalletInvalid));
                return Ok(());
            }
        };

        let flow_ids = self.ordered_flow_ids();
        if flow_ids.is_empty() {
            self.set_transaction_form_error(t(self.state.locale, TextKey::StateUnallocatedMissing));
            return Ok(());
        }
        let flow_id = match flow_ids.get(flow_index) {
            Some(id) => *id,
            None => {
                self.set_transaction_form_error(t(self.state.locale, TextKey::ValidationFlowInvalid));
                return Ok(());
            }
        };

        let occurred_at = if occurred_raw.is_empty() {
            None
        } else {
            match self.parse_local_datetime(occurred_raw.as_str()) {
                Ok(dt) => Some(dt),
                Err(message) => {
                    self.set_transaction_form_error(&message);
                    return Ok(());
                }
            }
        };
        let occurred_at_new = occurred_at.unwrap_or_else(|| self.now_in_timezone());

        let category_clean = category_raw.trim_start_matches('#').trim();
        let category = if editing_id.is_some() {
            Some(category_clean.to_string())
        } else if category_clean.is_empty() {
            None
        } else {
            Some(category_clean.to_string())
        };
        let note = if editing_id.is_some() {
            Some(note_raw)
        } else if note_raw.is_empty() {
            None
        } else {
            Some(note_raw)
        };

        if let Some(transaction_id) = editing_id {
            let res = self
                .client
                .transaction_update(
                    transaction_id,
                    TransactionUpdate {
                        vault_id: vault_id.to_string(),
                        amount_minor: Some(amount_minor),
                        wallet_id: Some(wallet_id),
                        flow_id: Some(flow_id),
                        from_wallet_id: None,
                        to_wallet_id: None,
                        from_flow_id: None,
                        to_flow_id: None,
                        category_id: None,
                        category,
                        note,
                        occurred_at,
                    },
                )
                .await;

            match res {
                Ok(()) => {
                    self.state.last_flow_id = Some(flow_id);
                    self.state.transactions.form = TransactionFormState::default();
                    self.set_toast(t(self.state.locale, TextKey::SuccessTransactionUpdated), ToastLevel::Success);
                    self.load_transactions(true).await?;
                    self.open_transaction_detail_by_id(transaction_id).await?;
                }
                Err(err) => {
                    let Some(msg) = self.client_error_message(err) else { return Ok(()); };
                    self.state.transactions.form.error = Some(msg);
                    self.set_toast(t(self.state.locale, TextKey::ErrorUpdating), ToastLevel::Error);
                }
            }
        } else {
            let res = match kind {
                TransactionKind::Income => {
                    self.client
                        .income_new(
                            IncomeNew {
                                vault_id: vault_id.to_string(),
                                amount_minor,
                                flow_id: Some(flow_id),
                                wallet_id: Some(wallet_id),
                                category_id: None,
                                category,
                                note,
                                idempotency_key: None,
                                occurred_at: occurred_at_new,
                            },
                        )
                        .await
                }
                TransactionKind::Expense => {
                    self.client
                        .expense_new(
                            ExpenseNew {
                                vault_id: vault_id.to_string(),
                                amount_minor,
                                flow_id: Some(flow_id),
                                wallet_id: Some(wallet_id),
                                category_id: None,
                                category,
                                note,
                                idempotency_key: None,
                                occurred_at: occurred_at_new,
                            },
                        )
                        .await
                }
                TransactionKind::Refund => {
                    self.client
                        .refund_new(
                            Refund {
                                vault_id: vault_id.to_string(),
                                amount_minor,
                                flow_id: Some(flow_id),
                                wallet_id: Some(wallet_id),
                                category_id: None,
                                category,
                                note,
                                idempotency_key: None,
                                occurred_at: occurred_at_new,
                            },
                        )
                        .await
                }
                TransactionKind::TransferWallet | TransactionKind::TransferFlow => {
                    self.set_transaction_form_error(t(self.state.locale, TextKey::PromptUseDedicatedTransferForm));
                    return Ok(());
                }
            };

            match res {
                Ok(created) => {
                    self.state.last_flow_id = Some(flow_id);
                    self.state.transactions.last_created_id = Some(created.id);
                    self.state.transactions.mode = TransactionsMode::List;
                    self.state.transactions.form = TransactionFormState::default();
                    self.set_toast(t(self.state.locale, TextKey::SuccessTransactionSaved), ToastLevel::Success);
                    self.load_transactions(true).await?;
                }
                Err(err) => {
                    let Some(msg) = self.client_error_message(err) else { return Ok(()); };
                    self.state.transactions.form.error = Some(msg);
                    self.set_toast(t(self.state.locale, TextKey::ErrorSaving), ToastLevel::Error);
                }
            }
        }

        Ok(())
    }

    pub(crate) fn set_transaction_form_error(&mut self, message: &str) {
        self.state.transactions.form.error = Some(message.to_string());
    }
}
