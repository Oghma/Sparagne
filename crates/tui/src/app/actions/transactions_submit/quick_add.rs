use crate::{
    app::{
        App, AppState, QuickAddAmbiguousKind, ToastLevel,
        resolve::{default_wallet_flow, resolve_flow_name, resolve_wallet_name},
    },
    error::{AppError, Result},
    quick_add::{QuickAddKind, QuickAddParsed},
    text::{Locale, TextKey, t},
    ui::common::get_currency,
};
use api_types::transaction::{ExpenseNew, IncomeNew, Refund, TransferFlowNew, TransferWalletNew};
use uuid::Uuid;

/// Result of parsing and resolving a quick-add input.
///
/// Contains the parsed command, resolved wallet/flow IDs, and the category
/// string ready to be submitted to the server.
struct QuickAddResolved {
    parsed: QuickAddParsed,
    wallet_id: Uuid,
    flow_id: Uuid,
    category: Option<String>,
}

/// Parse the quick-add input string into a [`QuickAddParsed`] structure.
///
/// Returns `Ok(parsed)` on success or `Err(message)` with a user-facing error
/// string when the input cannot be parsed.
fn parse_quick_add_input(
    state: &AppState,
    locale: Locale,
) -> std::result::Result<QuickAddParsed, String> {
    let currency = get_currency(state);

    crate::quick_add::parse(&state.transactions.quick_input, currency, locale)
}

/// Resolve wallet, flow, and category targets from a parsed quick-add command.
///
/// Uses the default wallet/flow as a baseline, then overrides with any
/// explicit wallet/flow queries found in the parsed input, handling ambiguous
/// selections from the disambiguation UI if present.
///
/// Returns `Ok(resolved)` or `Err(message)` with a user-facing error when a
/// named target cannot be found.
fn resolve_quick_add_targets(
    state: &AppState,
    parsed: QuickAddParsed,
    locale: Locale,
) -> std::result::Result<QuickAddResolved, String> {
    let (mut wallet_id, mut flow_id, _wallet_name, _flow_name) =
        default_wallet_flow(state, locale)?;

    // Resolve wallet override
    if let Some(wallet_query) = parsed.wallet.as_deref() {
        let resolved = state
            .transactions
            .quick_ambiguous
            .as_ref()
            .filter(|amb| amb.kind == QuickAddAmbiguousKind::Wallet)
            .and_then(|amb| amb.current())
            .map(|(id, _)| *id);

        if let Some(id) = resolved {
            wallet_id = id;
        } else {
            match resolve_wallet_name(state, wallet_query) {
                Some((resolved_id, _, _)) => wallet_id = resolved_id,
                None => {
                    return Err(crate::text::format(
                        locale,
                        TextKey::QuickAddWalletNotFound,
                        &[("query", wallet_query)],
                    ));
                }
            }
        }
    }

    // Resolve flow override
    if let Some(flow_query) = parsed.flow.as_deref() {
        let resolved = state
            .transactions
            .quick_ambiguous
            .as_ref()
            .filter(|amb| amb.kind == QuickAddAmbiguousKind::Flow)
            .and_then(|amb| amb.current())
            .map(|(id, _)| *id);

        if let Some(id) = resolved {
            flow_id = id;
        } else {
            match resolve_flow_name(state, flow_query) {
                Some((resolved_id, _, _)) => flow_id = resolved_id,
                None => {
                    return Err(crate::text::format(
                        locale,
                        TextKey::QuickAddEnvelopeNotFound,
                        &[("query", flow_query)],
                    ));
                }
            }
        }
    }

    // Resolve category
    let category = if let Some(category_query) = &parsed.category {
        let resolved = state
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

    Ok(QuickAddResolved {
        parsed,
        wallet_id,
        flow_id,
        category,
    })
}

impl App {
    pub(crate) async fn submit_quick_add(&mut self) -> Result<()> {
        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.id.as_deref())
            .map(|s| s.to_string())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))?;

        // Phase 1: Parse
        let parsed = match parse_quick_add_input(&self.state, self.state.locale) {
            Ok(parsed) => parsed,
            Err(message) => {
                self.state.transactions.quick_error = Some(message);
                return Ok(());
            }
        };

        // Handle transfers separately (they resolve their own targets)
        if parsed.kind == QuickAddKind::TransferWallet {
            return self
                .submit_quick_add_transfer_wallet(&vault_id, &parsed, self.now_in_timezone())
                .await;
        }
        if parsed.kind == QuickAddKind::TransferFlow {
            return self
                .submit_quick_add_transfer_flow(&vault_id, &parsed, self.now_in_timezone())
                .await;
        }

        // Phase 2: Resolve targets
        let resolved = match resolve_quick_add_targets(&self.state, parsed, self.state.locale) {
            Ok(resolved) => resolved,
            Err(message) => {
                self.state.transactions.quick_error = Some(message);
                return Ok(());
            }
        };

        let occurred_at = self.now_in_timezone();

        // Phase 3: Submit
        let res = match resolved.parsed.kind {
            QuickAddKind::Income => {
                self.client
                    .income_new(IncomeNew {
                        vault_id: vault_id.clone(),
                        amount_minor: resolved.parsed.amount_minor,
                        flow_id: Some(resolved.flow_id),
                        wallet_id: Some(resolved.wallet_id),
                        category_id: None,
                        category: resolved.category.clone(),
                        note: resolved.parsed.note.clone(),
                        idempotency_key: None,
                        occurred_at,
                    })
                    .await
            }
            QuickAddKind::Expense => {
                self.client
                    .expense_new(ExpenseNew {
                        vault_id: vault_id.clone(),
                        amount_minor: resolved.parsed.amount_minor,
                        flow_id: Some(resolved.flow_id),
                        wallet_id: Some(resolved.wallet_id),
                        category_id: None,
                        category: resolved.category.clone(),
                        note: resolved.parsed.note.clone(),
                        idempotency_key: None,
                        occurred_at,
                    })
                    .await
            }
            QuickAddKind::Refund => {
                self.client
                    .refund_new(Refund {
                        vault_id: vault_id.clone(),
                        amount_minor: resolved.parsed.amount_minor,
                        flow_id: Some(resolved.flow_id),
                        wallet_id: Some(resolved.wallet_id),
                        category_id: None,
                        category: resolved.category.clone(),
                        note: resolved.parsed.note.clone(),
                        idempotency_key: None,
                        occurred_at,
                    })
                    .await
            }
            QuickAddKind::TransferWallet | QuickAddKind::TransferFlow => {
                // Already handled above
                unreachable!()
            }
        };

        match res {
            Ok(created) => {
                self.state.last_flow_id = Some(resolved.flow_id);
                self.state.transactions.last_created_id = Some(created.id);
                self.state.transactions.quick_input.clear();
                self.state.transactions.quick_error = None;
                self.state.transactions.quick_ambiguous = None;
                self.set_toast(t(self.state.locale, TextKey::SuccessTransactionSaved), ToastLevel::Success);
                self.load_transactions(true).await?;
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorSaving) else { return Ok(()); };
                self.state.transactions.quick_error = Some(msg);
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
                self.set_toast(t(self.state.locale, TextKey::SuccessTransferWalletSaved), ToastLevel::Success);
                self.load_transactions(true).await?;
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorSaving) else { return Ok(()); };
                self.state.transactions.quick_error = Some(msg);
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
                self.set_toast(t(self.state.locale, TextKey::SuccessTransferFlowSaved), ToastLevel::Success);
                self.load_transactions(true).await?;
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorSaving) else { return Ok(()); };
                self.state.transactions.quick_error = Some(msg);
            }
        }

        Ok(())
    }
}
