use super::super::*;

use crate::{
    app::{
        errors::login_message_for_error,
        resolve::{extract_flow_transfer, extract_wallet_flow, extract_wallet_transfer},
    },
    error::{AppError, Result},
};
use api_types::transaction::{
    ExpenseNew, IncomeNew, Refund, TransactionGet, TransactionList, TransactionListResponse,
    TransactionVoid, TransferFlowNew, TransferWalletNew,
};

impl App {
    pub(crate) async fn load_transactions(&mut self, reset: bool) -> Result<()> {
        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.id.as_deref())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))?;

        if reset {
            self.state.transactions.reset();
        }

        let payload = TransactionList {
            vault_id: vault_id.to_string(),
            flow_id: self.state.transactions.scope_flow_id,
            wallet_id: self.state.transactions.scope_wallet_id,
            limit: Some(20),
            cursor: self.state.transactions.cursor.clone(),
            from: self.state.transactions.filter_from,
            to: self.state.transactions.filter_to,
            kinds: self.state.transactions.filter_kinds.clone(),
            include_voided: Some(self.state.transactions.include_voided),
            include_transfers: Some(self.state.transactions.include_transfers),
        };

        let res = self
            .client
            .transactions_list(
                payload,
            )
            .await;

        match res {
            Ok(TransactionListResponse {
                transactions,
                next_cursor,
            }) => {
                self.state.transactions.items = transactions;
                self.state.transactions.next_cursor = next_cursor;
                self.state.transactions.error = None;
                self.state.transactions.selected = 0;
                self.update_recent_categories_from_items();
                if reset {
                    self.refresh_recent_targets().await?;
                }
                self.connection_ok(None);
                self.refresh_transactions_search().await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.error = Some(login_message_for_error(err, self.state.locale));
                self.connection_error("Errore connessione");
            }
        }

        Ok(())
    }

    pub(crate) async fn load_transactions_next(&mut self) -> Result<()> {
        if let Some(next) = self.state.transactions.next_cursor.take() {
            let prev = self.state.transactions.cursor.take();
            self.state.transactions.push_cursor(prev);
            self.state.transactions.cursor = Some(next);
            self.load_transactions(false).await?;
        }
        Ok(())
    }

    pub(crate) async fn load_transactions_prev(&mut self) -> Result<()> {
        if let Some(prev) = self.state.transactions.pop_cursor() {
            self.state.transactions.cursor = prev;
            self.load_transactions(false).await?;
        }
        Ok(())
    }
    pub(crate) async fn open_transaction_detail(&mut self) -> Result<()> {
        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.id.as_deref())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))?;
        let indices = transactions_visible_indices(&self.state);
        let Some(item_index) = indices.get(self.state.transactions.selected).copied() else {
            return Ok(());
        };
        let Some(selected) = self.state.transactions.items.get(item_index) else {
            return Ok(());
        };

        let res = self
            .client
            .transaction_detail(
                TransactionGet {
                    vault_id: vault_id.to_string(),
                    id: selected.id,
                },
            )
            .await;

        match res {
            Ok(detail) => {
                self.state.transactions.detail = Some(detail);
                self.state.transactions.mode = TransactionsMode::Detail;
                self.state.transactions.form = TransactionFormState::default();
                self.state.transactions.transfer = TransferFormState::default();
                self.connection_ok(None);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.error = Some(login_message_for_error(err, self.state.locale));
                self.connection_error("Errore connessione");
            }
        }

        Ok(())
    }

    pub(crate) async fn open_transaction_detail_by_id(
        &mut self,
        transaction_id: uuid::Uuid,
    ) -> Result<()> {
        if self.select_transaction_by_id(transaction_id) {
            return self.open_transaction_detail().await;
        }

        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.id.as_deref())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))?;
        let res = self
            .client
            .transaction_detail(
                TransactionGet {
                    vault_id: vault_id.to_string(),
                    id: transaction_id,
                },
            )
            .await;

        match res {
            Ok(detail) => {
                self.state.transactions.detail = Some(detail);
                self.state.transactions.mode = TransactionsMode::Detail;
                self.state.transactions.form = TransactionFormState::default();
                self.state.transactions.transfer = TransferFormState::default();
                self.connection_ok(None);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.error = Some(login_message_for_error(err, self.state.locale));
                self.connection_error("Errore connessione");
            }
        }

        Ok(())
    }
    pub(crate) async fn void_transaction(&mut self) -> Result<()> {
        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.id.as_deref())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))?;
        let Some(detail) = self.state.transactions.detail.as_ref() else {
            return Ok(());
        };

        let res = self
            .client
            .transaction_void(
                detail.transaction.id,
                TransactionVoid {
                    vault_id: vault_id.to_string(),
                    voided_at: None,
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.state.transactions.mode = TransactionsMode::List;
                self.state.transactions.detail = None;
                self.set_toast(&t(self.state.locale, TextKey::SuccessTransactionVoided), ToastLevel::Success);
                self.load_transactions(true).await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.error = Some(login_message_for_error(err, self.state.locale));
                self.set_toast(&t(self.state.locale, TextKey::ErrorVoiding), ToastLevel::Error);
            }
        }

        Ok(())
    }

    pub(crate) async fn repeat_transaction(&mut self) -> Result<()> {
        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.id.as_deref())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))?;
        let Some(detail) = self.state.transactions.detail.as_ref() else {
            return Ok(());
        };
        let occurred_at = self.now_in_timezone();

        let mut last_flow_id = None;
        let res = match detail.transaction.kind {
            api_types::transaction::TransactionKind::Income => {
                let (wallet_id, flow_id) = extract_wallet_flow(detail);
                last_flow_id = flow_id;
                self.client
                    .income_new(
                        IncomeNew {
                            vault_id: vault_id.to_string(),
                            amount_minor: detail.transaction.amount_minor,
                            flow_id,
                            wallet_id,
                            category_id: Some(detail.transaction.category_id),
                            category: detail.transaction.category.clone(),
                            note: detail.transaction.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
            api_types::transaction::TransactionKind::Expense => {
                let (wallet_id, flow_id) = extract_wallet_flow(detail);
                last_flow_id = flow_id;
                self.client
                    .expense_new(
                        ExpenseNew {
                            vault_id: vault_id.to_string(),
                            amount_minor: detail.transaction.amount_minor,
                            flow_id,
                            wallet_id,
                            category_id: Some(detail.transaction.category_id),
                            category: detail.transaction.category.clone(),
                            note: detail.transaction.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
            api_types::transaction::TransactionKind::Refund => {
                let (wallet_id, flow_id) = extract_wallet_flow(detail);
                last_flow_id = flow_id;
                self.client
                    .refund_new(
                        Refund {
                            vault_id: vault_id.to_string(),
                            amount_minor: detail.transaction.amount_minor,
                            flow_id,
                            wallet_id,
                            category_id: Some(detail.transaction.category_id),
                            category: detail.transaction.category.clone(),
                            note: detail.transaction.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
            api_types::transaction::TransactionKind::TransferWallet => {
                let (from_wallet_id, to_wallet_id) = extract_wallet_transfer(detail, self.state.locale)?;
                self.client
                    .transfer_wallet_new(
                        TransferWalletNew {
                            vault_id: vault_id.to_string(),
                            amount_minor: detail.transaction.amount_minor,
                            from_wallet_id,
                            to_wallet_id,
                            note: detail.transaction.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
            api_types::transaction::TransactionKind::TransferFlow => {
                let (from_flow_id, to_flow_id) = extract_flow_transfer(detail, self.state.locale)?;
                self.client
                    .transfer_flow_new(
                        TransferFlowNew {
                            vault_id: vault_id.to_string(),
                            amount_minor: detail.transaction.amount_minor,
                            from_flow_id,
                            to_flow_id,
                            note: detail.transaction.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
        };

        match res {
            Ok(created) => {
                if let Some(flow_id) = last_flow_id {
                    self.state.last_flow_id = Some(flow_id);
                }
                self.state.transactions.last_created_id = Some(created.id);
                self.set_toast(&t(self.state.locale, TextKey::SuccessTransactionRepeated), ToastLevel::Success);
                self.load_transactions(true).await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.error = Some(login_message_for_error(err, self.state.locale));
                self.set_toast(&t(self.state.locale, TextKey::ErrorRepeating), ToastLevel::Error);
            }
        }

        Ok(())
    }
}
