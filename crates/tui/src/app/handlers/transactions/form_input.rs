//! Transaction form input handling.
//!
//! Contains methods for transaction form input, focus management, and form initialization.

use crate::{
    app::{
        format::format_amount_input,
        ordering::ordered_ids,
        resolve::{default_wallet_flow, extract_flow_transfer, extract_wallet_flow, extract_wallet_transfer},
        App, Section, ToastLevel, TransactionFormField, TransactionFormState, TransactionsMode,
        TransferField, TransferFormState,
    },
    error::Result,
    text::{TextKey, t},
    ui::forms::{AmountField, TextField},
    validation::DateField,
};
use api_types::transaction::TransactionKind;

impl App {
    pub(crate) async fn start_transaction_form(&mut self, kind: TransactionKind) -> Result<()> {
        self.state.section = Section::Transactions;
        if self.state.snapshot.is_none() {
            self.refresh_snapshot().await?;
        }
        self.ensure_last_flow();

        let (wallet_index, flow_index) = match self.default_transaction_form_indices() {
            Ok(indices) => indices,
            Err(message) => {
                self.set_toast(&message, ToastLevel::Error);
                return Ok(());
            }
        };

        let occurred_at_str = self.format_local_datetime(self.now_in_timezone());
        self.state.transactions.form = TransactionFormState {
            kind,
            amount: AmountField::new("Amount"),
            wallet_index,
            flow_index,
            category: TextField::new("Category"),
            note: TextField::new("Note"),
            occurred_at: DateField::with_value(occurred_at_str),
            focus: TransactionFormField::Amount,
            error: None,
            category_index: None,
            editing_id: None,
        };
        self.state.transactions.quick_active = false;
        self.state.transactions.quick_input.clear();
        self.state.transactions.quick_error = None;
        self.state.transactions.mode = TransactionsMode::Form;
        Ok(())
    }

    pub(crate) async fn start_transaction_edit(&mut self) -> Result<()> {
        if self.state.snapshot.is_none() {
            self.refresh_snapshot().await?;
        }

        let Some(detail) = self.state.transactions.detail.as_ref() else {
            return Ok(());
        };
        if detail.transaction.voided {
            self.set_toast(
                t(self.state.locale, TextKey::ValidationTransactionVoided),
                ToastLevel::Error,
            );
            return Ok(());
        }

        let currency = self.current_currency();
        let occurred_at_str = self.format_local_datetime(detail.transaction.occurred_at);
        let amount = format_amount_input(detail.transaction.amount_minor, currency);

        match detail.transaction.kind {
            TransactionKind::Income | TransactionKind::Expense | TransactionKind::Refund => {
                let (wallet_id, flow_id) = extract_wallet_flow(detail);
                let (Some(wallet_id), Some(flow_id)) = (wallet_id, flow_id) else {
                    self.set_toast(t(self.state.locale, TextKey::ValidationTransactionInvalid), ToastLevel::Error);
                    return Ok(());
                };

                let wallet_ids = self.ordered_wallet_ids();
                let flow_ids = self.ordered_flow_ids();
                let Some(wallet_index) = wallet_ids.iter().position(|id| *id == wallet_id) else {
                    self.set_toast(
                        t(self.state.locale, TextKey::ValidationWalletArchived),
                        ToastLevel::Error,
                    );
                    return Ok(());
                };
                let Some(flow_index) = flow_ids.iter().position(|id| *id == flow_id) else {
                    self.set_toast(
                        t(self.state.locale, TextKey::ValidationFlowArchived),
                        ToastLevel::Error,
                    );
                    return Ok(());
                };

                self.state.transactions.form = TransactionFormState {
                    kind: detail.transaction.kind,
                    amount: AmountField::new("Amount").with_value(amount),
                    wallet_index,
                    flow_index,
                    category: TextField::new("Category")
                        .with_value(detail.transaction.category.clone().unwrap_or_default()),
                    note: TextField::new("Note")
                        .with_value(detail.transaction.note.clone().unwrap_or_default()),
                    occurred_at: DateField::with_value(occurred_at_str.clone()),
                    focus: TransactionFormField::Amount,
                    error: None,
                    category_index: None,
                    editing_id: Some(detail.transaction.id),
                };
                self.state.transactions.quick_active = false;
                self.state.transactions.quick_input.clear();
                self.state.transactions.quick_error = None;
                self.state.transactions.mode = TransactionsMode::Edit;
            }
            TransactionKind::TransferWallet => {
                let (from_id, to_id) = match extract_wallet_transfer(detail, self.state.locale) {
                    Ok(values) => values,
                    Err(_) => {
                        self.set_toast(t(self.state.locale, TextKey::ValidationTransferWalletInvalid), ToastLevel::Error);
                        return Ok(());
                    }
                };
                let ids = self.active_wallet_ids();
                let Some(from_index) = ids.iter().position(|id| *id == from_id) else {
                    self.set_toast(
                        t(self.state.locale, TextKey::ValidationWalletArchived),
                        ToastLevel::Error,
                    );
                    return Ok(());
                };
                let Some(to_index) = ids.iter().position(|id| *id == to_id) else {
                    self.set_toast(
                        t(self.state.locale, TextKey::ValidationWalletArchived),
                        ToastLevel::Error,
                    );
                    return Ok(());
                };

                self.state.transactions.transfer = TransferFormState {
                    from_index,
                    to_index,
                    amount: AmountField::new("Amount").with_value(amount),
                    note: TextField::new("Note")
                        .with_value(detail.transaction.note.clone().unwrap_or_default()),
                    occurred_at: DateField::with_value(occurred_at_str.clone()),
                    focus: TransferField::From,
                    error: None,
                    editing_id: Some(detail.transaction.id),
                };
                self.state.transactions.quick_active = false;
                self.state.transactions.quick_input.clear();
                self.state.transactions.quick_error = None;
                self.state.transactions.mode = TransactionsMode::TransferWallet;
            }
            TransactionKind::TransferFlow => {
                let (from_id, to_id) = match extract_flow_transfer(detail, self.state.locale) {
                    Ok(values) => values,
                    Err(_) => {
                        self.set_toast(t(self.state.locale, TextKey::ValidationTransferFlowInvalid), ToastLevel::Error);
                        return Ok(());
                    }
                };
                let ids = self.active_flow_ids();
                let Some(from_index) = ids.iter().position(|id| *id == from_id) else {
                    self.set_toast(
                        t(self.state.locale, TextKey::ValidationFlowArchived),
                        ToastLevel::Error,
                    );
                    return Ok(());
                };
                let Some(to_index) = ids.iter().position(|id| *id == to_id) else {
                    self.set_toast(
                        t(self.state.locale, TextKey::ValidationFlowArchived),
                        ToastLevel::Error,
                    );
                    return Ok(());
                };

                self.state.transactions.transfer = TransferFormState {
                    from_index,
                    to_index,
                    amount: AmountField::new("Amount").with_value(amount),
                    note: TextField::new("Note")
                        .with_value(detail.transaction.note.clone().unwrap_or_default()),
                    occurred_at: DateField::with_value(occurred_at_str),
                    focus: TransferField::From,
                    error: None,
                    editing_id: Some(detail.transaction.id),
                };
                self.state.transactions.quick_active = false;
                self.state.transactions.quick_input.clear();
                self.state.transactions.quick_error = None;
                self.state.transactions.mode = TransactionsMode::TransferFlow;
            }
        }

        Ok(())
    }

    pub(crate) fn default_transaction_form_indices(
        &self,
    ) -> std::result::Result<(usize, usize), String> {
        let (default_wallet_id, default_flow_id, _wallet_name, _flow_name) =
            default_wallet_flow(&self.state, self.state.locale)?;
        let wallet_ids = self.ordered_wallet_ids();
        let flow_ids = self.ordered_flow_ids();
        if wallet_ids.is_empty() {
            return Err(t(self.state.locale, TextKey::ValidationNoWalletAvailable).to_string());
        }
        if flow_ids.is_empty() {
            return Err(t(self.state.locale, TextKey::ValidationNoFlowAvailable).to_string());
        }
        let wallet_id = if self.state.transactions.scope_wallet_id.is_some() {
            default_wallet_id
        } else {
            self.state
                .transactions
                .recent_wallet_ids
                .iter()
                .find(|id| wallet_ids.contains(id))
                .copied()
                .unwrap_or(default_wallet_id)
        };
        let flow_id = if self.state.transactions.scope_flow_id.is_some() {
            default_flow_id
        } else {
            self.state
                .transactions
                .recent_flow_ids
                .iter()
                .find(|id| flow_ids.contains(id))
                .copied()
                .unwrap_or(default_flow_id)
        };

        let wallet_index = wallet_ids
            .iter()
            .position(|id| *id == wallet_id)
            .unwrap_or(0);
        let flow_index = flow_ids.iter().position(|id| *id == flow_id).unwrap_or(0);
        Ok((wallet_index, flow_index))
    }

    pub(crate) fn advance_transaction_form_focus(&mut self) {
        let form = &mut self.state.transactions.form;
        form.error = None;
        form.focus = match form.focus {
            TransactionFormField::Amount => TransactionFormField::Wallet,
            TransactionFormField::Wallet => TransactionFormField::Flow,
            TransactionFormField::Flow => TransactionFormField::Category,
            TransactionFormField::Category => TransactionFormField::Note,
            TransactionFormField::Note => TransactionFormField::OccurredAt,
            TransactionFormField::OccurredAt => TransactionFormField::Amount,
        };
    }

    pub(crate) fn handle_transaction_form_input(&mut self, ch: char) {
        let form = &mut self.state.transactions.form;
        form.error = None;
        match form.focus {
            TransactionFormField::Amount => form.amount.push(ch),
            TransactionFormField::Category => {
                form.category.push(ch);
                form.category_index = None;
            }
            TransactionFormField::Note => form.note.push(ch),
            TransactionFormField::OccurredAt => form.occurred_at.push(ch),
            TransactionFormField::Wallet | TransactionFormField::Flow => {}
        }
    }

    pub(crate) fn backspace_transaction_form(&mut self) {
        let form = &mut self.state.transactions.form;
        form.error = None;
        match form.focus {
            TransactionFormField::Amount => {
                form.amount.pop();
            }
            TransactionFormField::Category => {
                form.category.pop();
                form.category_index = None;
            }
            TransactionFormField::Note => {
                form.note.pop();
            }
            TransactionFormField::OccurredAt => {
                form.occurred_at.pop();
            }
            TransactionFormField::Wallet | TransactionFormField::Flow => {}
        }
    }

    pub(crate) fn transaction_form_select_next(&mut self) {
        let focus = self.state.transactions.form.focus;
        self.state.transactions.form.error = None;
        match focus {
            TransactionFormField::Wallet => {
                let len = self.active_wallets_len();
                if len > 0 {
                    let current = self.state.transactions.form.wallet_index;
                    self.state.transactions.form.wallet_index = (current + 1) % len;
                }
            }
            TransactionFormField::Flow => {
                let len = self.active_flows_len();
                if len > 0 {
                    let current = self.state.transactions.form.flow_index;
                    self.state.transactions.form.flow_index = (current + 1) % len;
                }
            }
            TransactionFormField::Category => {
                self.select_category_next();
            }
            _ => {}
        }
    }

    pub(crate) fn transaction_form_select_prev(&mut self) {
        let focus = self.state.transactions.form.focus;
        self.state.transactions.form.error = None;
        match focus {
            TransactionFormField::Wallet => {
                let len = self.active_wallets_len();
                if len > 0 {
                    let current = self.state.transactions.form.wallet_index;
                    self.state.transactions.form.wallet_index = (current + len - 1) % len;
                }
            }
            TransactionFormField::Flow => {
                let len = self.active_flows_len();
                if len > 0 {
                    let current = self.state.transactions.form.flow_index;
                    self.state.transactions.form.flow_index = (current + len - 1) % len;
                }
            }
            TransactionFormField::Category => {
                self.select_category_prev();
            }
            _ => {}
        }
    }

    pub(crate) fn active_wallets_len(&self) -> usize {
        self.state
            .snapshot
            .as_ref()
            .map(|snap| snap.wallets.iter().filter(|w| !w.archived).count())
            .unwrap_or(0)
    }

    pub(crate) fn active_flows_len(&self) -> usize {
        self.state
            .snapshot
            .as_ref()
            .map(|snap| snap.flows.iter().filter(|f| !f.archived).count())
            .unwrap_or(0)
    }

    pub(crate) fn active_wallet_ids(&self) -> Vec<uuid::Uuid> {
        self.state
            .snapshot
            .as_ref()
            .map(|snap| {
                snap.wallets
                    .iter()
                    .filter(|wallet| !wallet.archived)
                    .map(|wallet| wallet.id)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn active_flow_ids(&self) -> Vec<uuid::Uuid> {
        self.state
            .snapshot
            .as_ref()
            .map(|snap| {
                snap.flows
                    .iter()
                    .filter(|flow| !flow.archived)
                    .map(|flow| flow.id)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn ordered_wallet_ids(&self) -> Vec<uuid::Uuid> {
        let mut priority = Vec::new();
        if let Some(default_id) = self.state.default_wallet_id {
            priority.push(default_id);
        }
        priority.extend(self.state.transactions.recent_wallet_ids.iter().copied());
        ordered_ids(self.active_wallet_ids(), &priority)
    }

    pub(crate) fn ordered_flow_ids(&self) -> Vec<uuid::Uuid> {
        let mut priority = Vec::new();
        if let Some(default_id) = self.state.default_flow_id {
            priority.push(default_id);
        }
        priority.extend(self.state.transactions.recent_flow_ids.iter().copied());
        ordered_ids(self.active_flow_ids(), &priority)
    }
}
