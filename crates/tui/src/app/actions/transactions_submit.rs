use super::super::*;

use crate::{
    app::helpers::{
        default_wallet_flow, login_message_for_error, map_currency, resolve_flow_name,
        resolve_wallet_name,
    },
    error::Result,
    quick_add::QuickAddKind,
};
use api_types::transaction::{
    ExpenseNew, IncomeNew, Refund, TransactionKind, TransactionUpdate, TransactionVoid,
    TransferFlowNew, TransferWalletNew,
};
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
                form.amount.trim().to_string(),
                form.wallet_index,
                form.flow_index,
                form.category.trim().to_string(),
                form.note.trim().to_string(),
                form.occurred_at.value.trim().to_string(),
            )
        };
        let editing_id = self.state.transactions.form.editing_id;

        let amount_raw = amount_raw.as_str();
        if amount_raw.is_empty() {
            self.set_transaction_form_error("Inserisci un importo.");
            return Ok(());
        }
        let amount_minor = match Money::parse_major(amount_raw, currency) {
            Ok(money) => money.minor().abs(),
            Err(_) => {
                self.set_transaction_form_error("Importo non valido.");
                return Ok(());
            }
        };
        if amount_minor <= 0 {
            self.set_transaction_form_error("Importo deve essere > 0.");
            return Ok(());
        }

        let wallet_ids = self.ordered_wallet_ids();
        if wallet_ids.is_empty() {
            self.set_transaction_form_error("Nessun wallet disponibile.");
            return Ok(());
        }
        let wallet_id = match wallet_ids.get(wallet_index) {
            Some(id) => *id,
            None => {
                self.set_transaction_form_error("Wallet non valido.");
                return Ok(());
            }
        };

        let flow_ids = self.ordered_flow_ids();
        if flow_ids.is_empty() {
            self.set_transaction_form_error("Nessun flow disponibile.");
            return Ok(());
        }
        let flow_id = match flow_ids.get(flow_index) {
            Some(id) => *id,
            None => {
                self.set_transaction_form_error("Flow non valido.");
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
                    self.state.login.username.as_str(),
                    self.state.login.password.as_str(),
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
                    self.set_toast("Transazione aggiornata.", ToastLevel::Success);
                    self.load_transactions(true).await?;
                    self.open_transaction_detail_by_id(transaction_id).await?;
                }
                Err(err) => {
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    self.state.transactions.form.error = Some(login_message_for_error(err));
                    self.set_toast("Errore aggiornamento.", ToastLevel::Error);
                }
            }
        } else {
            let res = match kind {
                TransactionKind::Income => {
                    self.client
                        .income_new(
                            self.state.login.username.as_str(),
                            self.state.login.password.as_str(),
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
                            self.state.login.username.as_str(),
                            self.state.login.password.as_str(),
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
                            self.state.login.username.as_str(),
                            self.state.login.password.as_str(),
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
                    self.set_transaction_form_error("Usa il form transfer dedicato.");
                    return Ok(());
                }
            };

            match res {
                Ok(created) => {
                    self.state.last_flow_id = Some(flow_id);
                    self.state.transactions.last_created_id = Some(created.id);
                    self.state.transactions.mode = TransactionsMode::List;
                    self.state.transactions.form = TransactionFormState::default();
                    self.set_toast("Transazione salvata.", ToastLevel::Success);
                    self.load_transactions(true).await?;
                }
                Err(err) => {
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    self.state.transactions.form.error = Some(login_message_for_error(err));
                    self.set_toast("Errore salvataggio.", ToastLevel::Error);
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn submit_transfer_wallet(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let editing_id = self.state.transactions.transfer.editing_id;
        let ids = self.active_wallet_ids();
        if ids.len() < 2 {
            self.state.transactions.transfer.error = Some("Servono almeno 2 wallet.".to_string());
            return Ok(());
        }
        let from_id = ids[self.state.transactions.transfer.from_index];
        let to_id = ids[self.state.transactions.transfer.to_index];
        if from_id == to_id {
            self.state.transactions.transfer.error = Some("Scegli due wallet diversi.".to_string());
            return Ok(());
        }

        let currency = self.current_currency();
        let amount =
            match Money::parse_major(self.state.transactions.transfer.amount.trim(), currency) {
                Ok(money) => money.minor().abs(),
                Err(_) => {
                    self.state.transactions.transfer.error =
                        Some("Importo non valido.".to_string());
                    return Ok(());
                }
            };
        if amount <= 0 {
            self.state.transactions.transfer.error = Some("Importo deve essere > 0.".to_string());
            return Ok(());
        }

        let note = self.state.transactions.transfer.note.trim();
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
                    self.state.login.username.as_str(),
                    self.state.login.password.as_str(),
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
                    self.set_toast("Transfer wallet aggiornato.", ToastLevel::Success);
                    self.load_transactions(true).await?;
                    self.open_transaction_detail_by_id(transaction_id).await?;
                }
                Err(err) => {
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    self.state.transactions.transfer.error = Some(login_message_for_error(err));
                    self.set_toast("Errore transfer wallet.", ToastLevel::Error);
                }
            }
        } else {
            let res = self
                .client
                .transfer_wallet_new(
                    self.state.login.username.as_str(),
                    self.state.login.password.as_str(),
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
                    self.set_toast("Transfer wallet salvato.", ToastLevel::Success);
                    self.load_transactions(true).await?;
                }
                Err(err) => {
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    self.state.transactions.transfer.error = Some(login_message_for_error(err));
                    self.set_toast("Errore transfer wallet.", ToastLevel::Error);
                }
            }
        }

        Ok(())
    }
    pub(crate) async fn submit_transfer_flow(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let editing_id = self.state.transactions.transfer.editing_id;
        let ids = self.active_flow_ids();
        if ids.len() < 2 {
            self.state.transactions.transfer.error = Some("Servono almeno 2 flow.".to_string());
            return Ok(());
        }
        let from_id = ids[self.state.transactions.transfer.from_index];
        let to_id = ids[self.state.transactions.transfer.to_index];
        if from_id == to_id {
            self.state.transactions.transfer.error = Some("Scegli due flow diversi.".to_string());
            return Ok(());
        }

        let currency = self.current_currency();
        let amount =
            match Money::parse_major(self.state.transactions.transfer.amount.trim(), currency) {
                Ok(money) => money.minor().abs(),
                Err(_) => {
                    self.state.transactions.transfer.error =
                        Some("Importo non valido.".to_string());
                    return Ok(());
                }
            };
        if amount <= 0 {
            self.state.transactions.transfer.error = Some("Importo deve essere > 0.".to_string());
            return Ok(());
        }

        let note = self.state.transactions.transfer.note.trim();
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
                    self.state.login.username.as_str(),
                    self.state.login.password.as_str(),
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
                    self.set_toast("Transfer flow aggiornato.", ToastLevel::Success);
                    self.load_transactions(true).await?;
                    self.open_transaction_detail_by_id(transaction_id).await?;
                }
                Err(err) => {
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    self.state.transactions.transfer.error = Some(login_message_for_error(err));
                    self.set_toast("Errore transfer flow.", ToastLevel::Error);
                }
            }
        } else {
            let res = self
                .client
                .transfer_flow_new(
                    self.state.login.username.as_str(),
                    self.state.login.password.as_str(),
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
                    self.set_toast("Transfer flow salvato.", ToastLevel::Success);
                    self.load_transactions(true).await?;
                }
                Err(err) => {
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    self.state.transactions.transfer.error = Some(login_message_for_error(err));
                    self.set_toast("Errore transfer flow.", ToastLevel::Error);
                }
            }
        }

        Ok(())
    }
    pub(crate) async fn apply_filter(&mut self) -> Result<()> {
        let (from_input, to_input, kind_income, kind_expense, kind_refund, kind_tw, kind_tf) = {
            let filter = &self.state.transactions.filter;
            (
                filter.from_input.clone(),
                filter.to_input.clone(),
                filter.kind_income,
                filter.kind_expense,
                filter.kind_refund,
                filter.kind_transfer_wallet,
                filter.kind_transfer_flow,
            )
        };

        let from = if from_input.trim().is_empty() {
            None
        } else {
            match self.parse_local_datetime(&from_input) {
                Ok(dt) => Some(dt),
                Err(message) => {
                    self.state.transactions.filter.error = Some(message);
                    return Ok(());
                }
            }
        };

        let to = if to_input.trim().is_empty() {
            None
        } else {
            match self.parse_local_datetime(&to_input) {
                Ok(dt) => Some(dt),
                Err(message) => {
                    self.state.transactions.filter.error = Some(message);
                    return Ok(());
                }
            }
        };

        let mut kinds = Vec::new();
        if kind_income {
            kinds.push(api_types::transaction::TransactionKind::Income);
        }
        if kind_expense {
            kinds.push(api_types::transaction::TransactionKind::Expense);
        }
        if kind_refund {
            kinds.push(api_types::transaction::TransactionKind::Refund);
        }
        if kind_tw {
            kinds.push(api_types::transaction::TransactionKind::TransferWallet);
        }
        if kind_tf {
            kinds.push(api_types::transaction::TransactionKind::TransferFlow);
        }

        self.state.transactions.filter_from = from;
        self.state.transactions.filter_to = to;
        self.state.transactions.filter_kinds = if kinds.is_empty() { None } else { Some(kinds) };

        self.state.transactions.filter.error = None;
        self.state.transactions.mode = TransactionsMode::List;
        self.load_transactions(true).await?;
        Ok(())
    }
    pub(crate) async fn clear_filters(&mut self) -> Result<()> {
        self.state.transactions.scope_wallet_id = None;
        self.state.transactions.scope_flow_id = None;
        self.state.transactions.filter_from = None;
        self.state.transactions.filter_to = None;
        self.state.transactions.filter_kinds = None;
        self.state.transactions.filter = TransactionsFilterState::default();
        self.load_transactions(true).await?;
        Ok(())
    }
    pub(crate) async fn undo_last_transaction(&mut self) -> Result<()> {
        let Some(id) = self.state.transactions.last_created_id else {
            self.set_toast("Nessuna transazione da annullare.", ToastLevel::Info);
            return Ok(());
        };
        self.void_transaction_by_id(id, Some("Transazione annullata."))
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
                    self.state.login.username.as_str(),
                    self.state.login.password.as_str(),
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
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    failures += 1;
                    last_error = Some(login_message_for_error(err));
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
            self.set_toast("Categoria non valida.", ToastLevel::Error);
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
                    self.state.login.username.as_str(),
                    self.state.login.password.as_str(),
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
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    failures += 1;
                    last_error = Some(login_message_for_error(err));
                }
            }
        }

        if successes > 0 {
            self.exit_visual_mode();
            self.load_transactions(true).await?;
            self.set_toast(
                format!("Categorized {successes} transactions as #{category_clean}").as_str(),
                ToastLevel::Success,
            );
        }

        if failures > 0 {
            let base = last_error.unwrap_or_else(|| "Errore aggiornamento.".to_string());
            self.set_toast(
                format!("{base} ({failures}/{total})", total = transaction_ids.len()).as_str(),
                ToastLevel::Error,
            );
        }

        Ok(())
    }
    pub(crate) async fn submit_quick_add(&mut self) -> Result<()> {
        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.id.as_deref())
            .map(|s| s.to_string())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))?;

        let (mut wallet_id, mut flow_id, _wallet_name, _flow_name) =
            match default_wallet_flow(&self.state) {
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

        let parsed = match crate::quick_add::parse(&self.state.transactions.quick_input, currency) {
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
                            Some(format!("Wallet non trovato: @{wallet_query}"));
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
                            Some(format!("Envelope non trovato: >{flow_query}"));
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
                self.set_toast("Transazione salvata.", ToastLevel::Success);
                self.load_transactions(true).await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.quick_error = Some(login_message_for_error(err));
                self.set_toast("Errore durante il salvataggio.", ToastLevel::Error);
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
                    Some(format!("Wallet non trovato: @{from_query}"));
                return Ok(());
            }
        };
        let to_id = match resolve_wallet_name(&self.state, to_query) {
            Some((id, _, _)) => id,
            None => {
                self.state.transactions.quick_error =
                    Some(format!("Wallet non trovato: @{to_query}"));
                return Ok(());
            }
        };

        if from_id == to_id {
            self.state.transactions.quick_error =
                Some("I due wallet devono essere diversi.".to_string());
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
                self.set_toast("Transfer wallet salvato.", ToastLevel::Success);
                self.load_transactions(true).await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.quick_error = Some(login_message_for_error(err));
                self.set_toast("Errore durante il salvataggio.", ToastLevel::Error);
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
                    Some(format!("Flow non trovato: >{from_query}"));
                return Ok(());
            }
        };
        let to_id = match resolve_flow_name(&self.state, to_query) {
            Some((id, _, _)) => id,
            None => {
                self.state.transactions.quick_error =
                    Some(format!("Flow non trovato: >{to_query}"));
                return Ok(());
            }
        };

        if from_id == to_id {
            self.state.transactions.quick_error =
                Some("I due flow devono essere diversi.".to_string());
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
                self.set_toast("Transfer flow salvato.", ToastLevel::Success);
                self.load_transactions(true).await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.quick_error = Some(login_message_for_error(err));
                self.set_toast("Errore durante il salvataggio.", ToastLevel::Error);
            }
        }

        Ok(())
    }
}
