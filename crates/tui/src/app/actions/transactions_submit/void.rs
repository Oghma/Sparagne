use crate::{
    app::{App, ToastLevel},
    error::Result,
    text::{TextKey, format as t_format, t},
};
use api_types::transaction::{TransactionUpdate, TransactionVoid};

impl App {
    pub(crate) async fn undo_last_transaction(&mut self) -> Result<()> {
        let Some(id) = self.state.transactions.last_created_id else {
            self.set_toast(t(self.state.locale, TextKey::ValidationNoTransactionToVoid), ToastLevel::Info);
            return Ok(());
        };
        self.void_transaction_by_id(id, Some(t(self.state.locale, TextKey::SuccessTransactionVoided)))
            .await?;
        Ok(())
    }

    pub(crate) async fn void_transactions_by_ids(
        &mut self,
        transaction_ids: &[uuid::Uuid],
        toast_message: Option<&str>,
    ) -> Result<()> {
        if transaction_ids.is_empty() {
            return Ok(());
        }
        let vault_id = self.current_vault_id()?;
        let mut failures = 0usize;
        let mut any_success = false;
        let mut last_error: Option<String> = None;

        for transaction_id in transaction_ids.iter().copied() {
            let res = self
                .client
                .transaction_void(
                    transaction_id,
                    TransactionVoid {
                        vault_id: vault_id.clone(),
                        voided_at: None,
                    },
                )
                .await;

            match res {
                Ok(()) => {
                    any_success = true;
                    if self.state.transactions.last_created_id == Some(transaction_id) {
                        self.state.transactions.last_created_id = None;
                    }
                }
                Err(err) => {
                    let Some(msg) = self.client_error_message(err) else { return Ok(()); };
                    failures += 1;
                    last_error = Some(msg);
                }
            }
        }

        if any_success {
            self.load_transactions(true).await?;
        }

        if failures == 0 {
            if let Some(message) = toast_message {
                self.set_toast(message, ToastLevel::Success);
            }
        } else if let Some(err) = last_error {
            self.set_toast(
                format!("{err} ({failures}/{total})", total = transaction_ids.len()).as_str(),
                ToastLevel::Error,
            );
        }

        Ok(())
    }

    pub(crate) async fn void_transaction_by_id(
        &mut self,
        transaction_id: uuid::Uuid,
        toast_message: Option<&str>,
    ) -> Result<()> {
        self.void_transactions_by_ids(&[transaction_id], toast_message)
            .await
    }

    pub(crate) async fn bulk_categorize_transactions(
        &mut self,
        transaction_ids: &[uuid::Uuid],
        category: &str,
    ) -> Result<()> {
        if transaction_ids.is_empty() {
            return Ok(());
        }
        let category_clean = category.trim().trim_start_matches('#').trim();
        if category_clean.is_empty() {
            self.set_toast(t(self.state.locale, TextKey::ValidationCategoryInvalid), ToastLevel::Error);
            return Ok(());
        }

        let vault_id = self.current_vault_id()?;
        let mut successes = 0usize;
        let mut failures = 0usize;
        let mut last_error: Option<String> = None;

        for transaction_id in transaction_ids.iter().copied() {
            let res = self
                .client
                .transaction_update(
                    transaction_id,
                    TransactionUpdate {
                        vault_id: vault_id.to_string(),
                        amount_minor: None,
                        wallet_id: None,
                        flow_id: None,
                        from_wallet_id: None,
                        to_wallet_id: None,
                        from_flow_id: None,
                        to_flow_id: None,
                        category_id: None,
                        category: Some(category_clean.to_string()),
                        note: None,
                        occurred_at: None,
                    },
                )
                .await;

            match res {
                Ok(()) => {
                    successes += 1;
                }
                Err(err) => {
                    let Some(msg) = self.client_error_message(err) else { return Ok(()); };
                    failures += 1;
                    last_error = Some(msg);
                }
            }
        }

        if successes > 0 {
            self.exit_visual_mode();
            self.load_transactions(true).await?;
            self.set_toast(
                &t_format(self.state.locale, TextKey::SuccessCategorizedTransactions, &[("count", &successes.to_string()), ("category", category_clean)]),
                ToastLevel::Success,
            );
        }

        if failures > 0 {
            let base = last_error.unwrap_or_else(|| t(self.state.locale, TextKey::ErrorUpdating).to_string());
            self.set_toast(
                format!("{base} ({failures}/{total})", total = transaction_ids.len()).as_str(),
                ToastLevel::Error,
            );
        }

        Ok(())
    }
}
