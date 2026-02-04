use crate::{
    app::{
        App, QuickAddAmbiguousKind, ToastLevel,
        errors::login_message_for_error,
        format::map_currency,
        resolve::{default_wallet_flow, resolve_flow_name, resolve_wallet_name},
    },
    error::{AppError, Result},
    quick_add::QuickAddKind,
    text::{TextKey, t},
};
use api_types::transaction::{ExpenseNew, IncomeNew, Refund, TransferFlowNew, TransferWalletNew};

impl App {
    pub(crate) async fn submit_quick_add(&mut self) -> Result<()> {
        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.id.as_deref())
            .map(|s| s.to_string())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))?;

        let (mut wallet_id, mut flow_id, _wallet_name, _flow_name) =
            match default_wallet_flow(&self.state, self.state.locale) {
                Ok(res) => res,
                Err(message) => {
                    self.state.transactions.quick_error = Some(message);
                    return Ok(());
                }
            };

        let currency = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.currency.as_ref())
            .map(map_currency)
            .unwrap_or(engine::Currency::Eur);

        let parsed = match crate::quick_add::parse(&self.state.transactions.quick_input, currency, self.state.locale) {
            Ok(parsed) => parsed,
            Err(message) => {
                self.state.transactions.quick_error = Some(message);
                return Ok(());
            }
        };

        // Check for ambiguous selection for wallet
        if let Some(wallet_query) = parsed.wallet.as_deref() {
            let resolved = self
                .state
                .transactions
                .quick_ambiguous
                .as_ref()
                .filter(|amb| amb.kind == QuickAddAmbiguousKind::Wallet)
                .and_then(|amb| amb.current())
                .map(|(id, _)| *id);

            if let Some(id) = resolved {
                wallet_id = id;
            } else {
                match resolve_wallet_name(&self.state, wallet_query) {
                    Some((resolved_id, _, _)) => wallet_id = resolved_id,
                    None => {
                        self.state.transactions.quick_error =
                            Some(crate::text::format(self.state.locale, TextKey::QuickAddWalletNotFound, &[("query", wallet_query)]));
                        return Ok(());
                    }
                }
            }
        }

        // Check for ambiguous selection for flow
        if let Some(flow_query) = parsed.flow.as_deref() {
            let resolved = self
                .state
                .transactions
                .quick_ambiguous
                .as_ref()
                .filter(|amb| amb.kind == QuickAddAmbiguousKind::Flow)
                .and_then(|amb| amb.current())
                .map(|(id, _)| *id);

            if let Some(id) = resolved {
                flow_id = id;
            } else {
                match resolve_flow_name(&self.state, flow_query) {
                    Some((resolved_id, _, _)) => flow_id = resolved_id,
                    None => {
                        self.state.transactions.quick_error =
                            Some(crate::text::format(self.state.locale, TextKey::QuickAddEnvelopeNotFound, &[("query", flow_query)]));
                        return Ok(());
                    }
                }
            }
        }

        // Check for ambiguous selection for category
        let category = if let Some(category_query) = &parsed.category {
            let resolved = self
                .state
                .transactions
                .quick_ambiguous
                .as_ref()
                .filter(|amb| amb.kind == QuickAddAmbiguousKind::Category)
                .and_then(|amb| amb.current())
                .map(|(_, name)| name.clone());

            Some(resolved.unwrap_or_else(|| category_query.clone()))
        } else {
            None
        };

        let occurred_at = self.now_in_timezone();

        // Handle transfers separately
        if parsed.kind == QuickAddKind::TransferWallet {
            return self
                .submit_quick_add_transfer_wallet(&vault_id, &parsed, occurred_at)
                .await;
        }
        if parsed.kind == QuickAddKind::TransferFlow {
            return self
                .submit_quick_add_transfer_flow(&vault_id, &parsed, occurred_at)
                .await;
        }

        let res = match parsed.kind {
            QuickAddKind::Income => {
                self.client
                    .income_new(
                        self.state.login.username.as_str(),
                        self.state.login.password.as_str(),
                        IncomeNew {
                            vault_id: vault_id.clone(),
                            amount_minor: parsed.amount_minor,
                            flow_id: Some(flow_id),
                            wallet_id: Some(wallet_id),
                            category_id: None,
                            category: category.clone(),
                            note: parsed.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
            QuickAddKind::Expense => {
                self.client
                    .expense_new(
                        self.state.login.username.as_str(),
                        self.state.login.password.as_str(),
                        ExpenseNew {
                            vault_id: vault_id.clone(),
                            amount_minor: parsed.amount_minor,
                            flow_id: Some(flow_id),
                            wallet_id: Some(wallet_id),
                            category_id: None,
                            category: category.clone(),
                            note: parsed.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
            QuickAddKind::Refund => {
                self.client
                    .refund_new(
                        self.state.login.username.as_str(),
                        self.state.login.password.as_str(),
                        Refund {
                            vault_id: vault_id.clone(),
                            amount_minor: parsed.amount_minor,
                            flow_id: Some(flow_id),
                            wallet_id: Some(wallet_id),
                            category_id: None,
                            category: category.clone(),
                            note: parsed.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
            QuickAddKind::TransferWallet | QuickAddKind::TransferFlow => {
                // Already handled above
                unreachable!()
            }
        };

        match res {
            Ok(created) => {
                self.state.last_flow_id = Some(flow_id);
                self.state.transactions.last_created_id = Some(created.id);
                self.state.transactions.quick_input.clear();
                self.state.transactions.quick_error = None;
                self.state.transactions.quick_ambiguous = None;
                self.set_toast(&t(self.state.locale, TextKey::SuccessTransactionSaved), ToastLevel::Success);
                self.load_transactions(true).await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.quick_error = Some(login_message_for_error(err, self.state.locale));
                self.set_toast(&t(self.state.locale, TextKey::ErrorSaving), ToastLevel::Error);
            }
        }

        Ok(())
    }

    async fn submit_quick_add_transfer_wallet(
        &mut self,
        vault_id: &str,
        parsed: &crate::quick_add::QuickAddParsed,
        occurred_at: chrono::DateTime<chrono::FixedOffset>,
    ) -> Result<()> {
        let from_query = parsed.from_wallet.as_deref().unwrap_or("");
        let to_query = parsed.to_wallet.as_deref().unwrap_or("");

        let from_id = match resolve_wallet_name(&self.state, from_query) {
            Some((id, _, _)) => id,
            None => {
                self.state.transactions.quick_error =
                    Some(crate::text::format(self.state.locale, TextKey::QuickAddWalletNotFound, &[("query", from_query)]));
                return Ok(());
            }
        };
        let to_id = match resolve_wallet_name(&self.state, to_query) {
            Some((id, _, _)) => id,
            None => {
                self.state.transactions.quick_error =
                    Some(crate::text::format(self.state.locale, TextKey::QuickAddWalletNotFound, &[("query", to_query)]));
                return Ok(());
            }
        };

        if from_id == to_id {
            self.state.transactions.quick_error =
                Some(t(self.state.locale, TextKey::QuickAddWalletsMustBeDifferent).to_string());
            return Ok(());
        }

        let res = self
            .client
            .transfer_wallet_new(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                TransferWalletNew {
                    vault_id: vault_id.to_string(),
                    amount_minor: parsed.amount_minor,
                    from_wallet_id: from_id,
                    to_wallet_id: to_id,
                    note: parsed.note.clone(),
                    idempotency_key: None,
                    occurred_at,
                },
            )
            .await;

        match res {
            Ok(created) => {
                self.state.transactions.last_created_id = Some(created.id);
                self.state.transactions.quick_input.clear();
                self.state.transactions.quick_error = None;
                self.state.transactions.quick_ambiguous = None;
                self.set_toast(&t(self.state.locale, TextKey::SuccessTransferWalletSaved), ToastLevel::Success);
                self.load_transactions(true).await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.quick_error = Some(login_message_for_error(err, self.state.locale));
                self.set_toast(&t(self.state.locale, TextKey::ErrorSaving), ToastLevel::Error);
            }
        }

        Ok(())
    }

    async fn submit_quick_add_transfer_flow(
        &mut self,
        vault_id: &str,
        parsed: &crate::quick_add::QuickAddParsed,
        occurred_at: chrono::DateTime<chrono::FixedOffset>,
    ) -> Result<()> {
        let from_query = parsed.from_flow.as_deref().unwrap_or("");
        let to_query = parsed.to_flow.as_deref().unwrap_or("");

        let from_id = match resolve_flow_name(&self.state, from_query) {
            Some((id, _, _)) => id,
            None => {
                self.state.transactions.quick_error =
                    Some(crate::text::format(self.state.locale, TextKey::QuickAddFlowNotFound, &[("query", from_query)]));
                return Ok(());
            }
        };
        let to_id = match resolve_flow_name(&self.state, to_query) {
            Some((id, _, _)) => id,
            None => {
                self.state.transactions.quick_error =
                    Some(crate::text::format(self.state.locale, TextKey::QuickAddFlowNotFound, &[("query", to_query)]));
                return Ok(());
            }
        };

        if from_id == to_id {
            self.state.transactions.quick_error =
                Some(t(self.state.locale, TextKey::QuickAddFlowsMustBeDifferent).to_string());
            return Ok(());
        }

        let res = self
            .client
            .transfer_flow_new(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                TransferFlowNew {
                    vault_id: vault_id.to_string(),
                    amount_minor: parsed.amount_minor,
                    from_flow_id: from_id,
                    to_flow_id: to_id,
                    note: parsed.note.clone(),
                    idempotency_key: None,
                    occurred_at,
                },
            )
            .await;

        match res {
            Ok(created) => {
                self.state.transactions.last_created_id = Some(created.id);
                self.state.transactions.quick_input.clear();
                self.state.transactions.quick_error = None;
                self.state.transactions.quick_ambiguous = None;
                self.set_toast(&t(self.state.locale, TextKey::SuccessTransferFlowSaved), ToastLevel::Success);
                self.load_transactions(true).await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.quick_error = Some(login_message_for_error(err, self.state.locale));
                self.set_toast(&t(self.state.locale, TextKey::ErrorSaving), ToastLevel::Error);
            }
        }

        Ok(())
    }
}
