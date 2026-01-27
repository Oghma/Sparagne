use super::super::*;

use crate::{
    app::helpers::{
        default_wallet_flow, extract_flow_transfer, extract_wallet_flow, extract_wallet_transfer,
        format_amount_input, map_currency, ordered_ids,
    },
    error::Result,
};
use api_types::transaction::TransactionKind;
use engine::Money;

impl App {
    pub(crate) fn transactions_picker_next(&mut self) {
        let len = self.transactions_picker_len();
        if len == 0 {
            return;
        }
        self.state.transactions.picker_index =
            (self.state.transactions.picker_index + 1).min(len - 1);
    }

    pub(crate) fn transactions_picker_prev(&mut self) {
        let len = self.transactions_picker_len();
        if len == 0 {
            return;
        }
        self.state.transactions.picker_index =
            self.state.transactions.picker_index.saturating_sub(1);
    }

    pub(crate) fn transactions_picker_len(&self) -> usize {
        let Some(snapshot) = self.state.snapshot.as_ref() else {
            return 0;
        };
        match self.state.transactions.mode {
            TransactionsMode::PickWallet => snapshot.wallets.len() + 1,
            TransactionsMode::PickFlow => snapshot.flows.len() + 1,
            _ => 0,
        }
    }

    pub(crate) fn selected_transaction(&self) -> Option<&api_types::transaction::TransactionView> {
        let indices = transactions_visible_indices(&self.state);
        let index = indices.get(self.state.transactions.selected).copied()?;
        self.state.transactions.items.get(index)
    }

    pub(crate) fn toggle_visual_mode(&mut self) {
        if self.state.transactions.visual_mode {
            self.state.transactions.visual_mode = false;
            self.state.transactions.visual_selected.clear();
        } else {
            self.state.transactions.visual_mode = true;
            self.state.transactions.visual_selected.clear();
            self.state.transactions.quick_active = false;
            self.state.transactions.quick_input.clear();
            self.state.transactions.quick_error = None;
        }
    }

    pub(crate) fn exit_visual_mode(&mut self) {
        if self.state.transactions.visual_mode {
            self.state.transactions.visual_mode = false;
            self.state.transactions.visual_selected.clear();
        }
    }

    pub(crate) fn toggle_visual_selection(&mut self) {
        let Some(tx_id) = self.selected_transaction().map(|tx| tx.id) else {
            return;
        };
        if self.state.transactions.visual_selected.contains(&tx_id) {
            self.state.transactions.visual_selected.remove(&tx_id);
        } else {
            self.state.transactions.visual_selected.insert(tx_id);
        }
    }

    pub(crate) async fn queue_transaction_delete(&mut self) -> Result<()> {
        self.finalize_pending_undo().await?;
        let visual_mode = self.state.transactions.visual_mode;
        let visual_ids = if visual_mode {
            self.state
                .transactions
                .visual_selected
                .iter()
                .copied()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let selected_tx_id = self.selected_transaction().map(|tx| tx.id);
        let Some(tx_id) = selected_tx_id else {
            self.state.transactions.error = Some("Nessuna transazione selezionata.".to_string());
            return Ok(());
        };

        let ids = if visual_mode && !visual_ids.is_empty() {
            visual_ids
        } else {
            vec![tx_id]
        };

        let single_info = if ids.len() == 1 {
            self.state
                .transactions
                .items
                .iter()
                .find(|tx| tx.id == ids[0])
                .map(|tx| (tx.amount_minor, tx.note.clone()))
        } else {
            None
        };

        if self.state.transactions.mode == TransactionsMode::Detail {
            self.state.transactions.mode = TransactionsMode::List;
            self.state.transactions.detail = None;
        }

        for id in &ids {
            self.state.transactions.pending_delete_ids.insert(*id);
            self.state.transactions.visual_selected.remove(id);
        }
        if visual_mode {
            self.exit_visual_mode();
        }
        let visible_len = transactions_visible_indices(&self.state).len();
        if visible_len == 0 {
            self.state.transactions.selected = 0;
        } else if self.state.transactions.selected >= visible_len {
            self.state.transactions.selected = visible_len - 1;
        }

        let currency = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.currency.as_ref())
            .map(map_currency)
            .unwrap_or(engine::Currency::Eur);
        let message = if let Some((amount_minor, note)) = single_info {
            let amount = Money::new(amount_minor).format(currency);
            let label = note.as_deref().unwrap_or("Transaction");
            format!("Deleted \"{label}\" ({amount})")
        } else {
            format!("Deleted {} transactions", ids.len())
        };

        self.set_undo_toast(&message, UndoAction::TransactionVoid { ids });
        Ok(())
    }
    pub(crate) fn open_wallet_picker(&mut self) {
        self.state.transactions.quick_active = false;
        self.state.transactions.picker_index = self
            .state
            .transactions
            .scope_wallet_id
            .and_then(|wallet_id| {
                self.state.snapshot.as_ref().and_then(|snap| {
                    snap.wallets
                        .iter()
                        .position(|wallet| wallet.id == wallet_id)
                })
            })
            .map(|idx| idx + 1)
            .unwrap_or(0);
        self.state.transactions.mode = TransactionsMode::PickWallet;
    }

    pub(crate) fn open_flow_picker(&mut self) {
        self.state.transactions.quick_active = false;
        self.state.transactions.picker_index = self
            .state
            .transactions
            .scope_flow_id
            .and_then(|flow_id| {
                self.state
                    .snapshot
                    .as_ref()
                    .and_then(|snap| snap.flows.iter().position(|flow| flow.id == flow_id))
            })
            .map(|idx| idx + 1)
            .unwrap_or(0);
        self.state.transactions.mode = TransactionsMode::PickFlow;
    }

    pub(crate) async fn apply_wallet_picker(&mut self) -> Result<()> {
        let Some(snapshot) = self.state.snapshot.as_ref() else {
            self.state.transactions.error = Some("Snapshot non disponibile.".to_string());
            self.state.transactions.mode = TransactionsMode::List;
            return Ok(());
        };

        if self.state.transactions.picker_index == 0 {
            self.state.transactions.scope_wallet_id = None;
        } else {
            let index = self.state.transactions.picker_index - 1;
            if let Some(wallet) = snapshot.wallets.get(index) {
                self.state.transactions.scope_wallet_id = Some(wallet.id);
            }
        }

        self.state.transactions.scope_flow_id = None;
        self.state.transactions.mode = TransactionsMode::List;
        self.state.transactions.picker_index = 0;
        self.load_transactions(true).await?;
        Ok(())
    }

    pub(crate) async fn apply_flow_picker(&mut self) -> Result<()> {
        let Some(snapshot) = self.state.snapshot.as_ref() else {
            self.state.transactions.error = Some("Snapshot non disponibile.".to_string());
            self.state.transactions.mode = TransactionsMode::List;
            return Ok(());
        };

        if self.state.transactions.picker_index == 0 {
            self.state.transactions.scope_flow_id = None;
        } else {
            let index = self.state.transactions.picker_index - 1;
            if let Some(flow) = snapshot.flows.get(index) {
                self.state.transactions.scope_flow_id = Some(flow.id);
                self.state.last_flow_id = Some(flow.id);
            }
        }

        self.state.transactions.scope_wallet_id = None;
        self.state.transactions.mode = TransactionsMode::List;
        self.state.transactions.picker_index = 0;
        self.load_transactions(true).await?;
        Ok(())
    }

    pub(crate) fn start_transfer_wallet(&mut self) {
        let occurred_at = self.format_local_datetime(self.now_in_timezone());
        self.state.transactions.transfer = TransferFormState {
            occurred_at,
            ..TransferFormState::default()
        };
        self.state.transactions.mode = TransactionsMode::TransferWallet;
        self.init_transfer_indices();
    }

    pub(crate) fn start_transfer_flow(&mut self) {
        let occurred_at = self.format_local_datetime(self.now_in_timezone());
        self.state.transactions.transfer = TransferFormState {
            occurred_at,
            ..TransferFormState::default()
        };
        self.state.transactions.mode = TransactionsMode::TransferFlow;
        self.init_transfer_indices();
    }

    pub(crate) fn init_transfer_indices(&mut self) {
        let len = match self.state.transactions.mode {
            TransactionsMode::TransferWallet => self.active_wallets_len(),
            TransactionsMode::TransferFlow => self.active_flows_len(),
            _ => 0,
        };
        if len == 0 {
            self.state.transactions.transfer.error =
                Some("Nessun elemento disponibile.".to_string());
            return;
        }
        self.state.transactions.transfer.from_index = 0;
        self.state.transactions.transfer.to_index = if len > 1 { 1 } else { 0 };
    }

    pub(crate) fn transfer_select_next(&mut self) {
        let len = match self.state.transactions.mode {
            TransactionsMode::TransferWallet => self.active_wallets_len(),
            TransactionsMode::TransferFlow => self.active_flows_len(),
            _ => 0,
        };
        if len == 0 {
            return;
        }
        match self.state.transactions.transfer.focus {
            TransferField::From => {
                self.state.transactions.transfer.from_index =
                    (self.state.transactions.transfer.from_index + 1) % len;
            }
            TransferField::To => {
                self.state.transactions.transfer.to_index =
                    (self.state.transactions.transfer.to_index + 1) % len;
            }
            _ => {}
        }
    }

    pub(crate) fn transfer_select_prev(&mut self) {
        let len = match self.state.transactions.mode {
            TransactionsMode::TransferWallet => self.active_wallets_len(),
            TransactionsMode::TransferFlow => self.active_flows_len(),
            _ => 0,
        };
        if len == 0 {
            return;
        }
        match self.state.transactions.transfer.focus {
            TransferField::From => {
                self.state.transactions.transfer.from_index =
                    (self.state.transactions.transfer.from_index + len - 1) % len;
            }
            TransferField::To => {
                self.state.transactions.transfer.to_index =
                    (self.state.transactions.transfer.to_index + len - 1) % len;
            }
            _ => {}
        }
    }

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

        let occurred_at = self.format_local_datetime(self.now_in_timezone());
        self.state.transactions.form = TransactionFormState {
            kind,
            amount: String::new(),
            wallet_index,
            flow_index,
            category: String::new(),
            note: String::new(),
            occurred_at,
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
                "Transazione annullata: modifica non disponibile.",
                ToastLevel::Error,
            );
            return Ok(());
        }

        let currency = self.current_currency();
        let occurred_at = self.format_local_datetime(detail.transaction.occurred_at);
        let amount = format_amount_input(detail.transaction.amount_minor, currency);

        match detail.transaction.kind {
            TransactionKind::Income | TransactionKind::Expense | TransactionKind::Refund => {
                let (wallet_id, flow_id) = extract_wallet_flow(detail);
                let (Some(wallet_id), Some(flow_id)) = (wallet_id, flow_id) else {
                    self.set_toast("Transazione non valida.", ToastLevel::Error);
                    return Ok(());
                };

                let wallet_ids = self.ordered_wallet_ids();
                let flow_ids = self.ordered_flow_ids();
                let Some(wallet_index) = wallet_ids.iter().position(|id| *id == wallet_id) else {
                    self.set_toast(
                        "Wallet archiviato: modifica non disponibile.",
                        ToastLevel::Error,
                    );
                    return Ok(());
                };
                let Some(flow_index) = flow_ids.iter().position(|id| *id == flow_id) else {
                    self.set_toast(
                        "Flow archiviato: modifica non disponibile.",
                        ToastLevel::Error,
                    );
                    return Ok(());
                };

                self.state.transactions.form = TransactionFormState {
                    kind: detail.transaction.kind,
                    amount,
                    wallet_index,
                    flow_index,
                    category: detail.transaction.category.clone().unwrap_or_default(),
                    note: detail.transaction.note.clone().unwrap_or_default(),
                    occurred_at,
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
                let (from_id, to_id) = match extract_wallet_transfer(detail) {
                    Ok(values) => values,
                    Err(_) => {
                        self.set_toast("Transfer wallet non valido.", ToastLevel::Error);
                        return Ok(());
                    }
                };
                let ids = self.active_wallet_ids();
                let Some(from_index) = ids.iter().position(|id| *id == from_id) else {
                    self.set_toast(
                        "Wallet archiviato: modifica non disponibile.",
                        ToastLevel::Error,
                    );
                    return Ok(());
                };
                let Some(to_index) = ids.iter().position(|id| *id == to_id) else {
                    self.set_toast(
                        "Wallet archiviato: modifica non disponibile.",
                        ToastLevel::Error,
                    );
                    return Ok(());
                };

                self.state.transactions.transfer = TransferFormState {
                    from_index,
                    to_index,
                    amount,
                    note: detail.transaction.note.clone().unwrap_or_default(),
                    occurred_at,
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
                let (from_id, to_id) = match extract_flow_transfer(detail) {
                    Ok(values) => values,
                    Err(_) => {
                        self.set_toast("Transfer flow non valido.", ToastLevel::Error);
                        return Ok(());
                    }
                };
                let ids = self.active_flow_ids();
                let Some(from_index) = ids.iter().position(|id| *id == from_id) else {
                    self.set_toast(
                        "Flow archiviato: modifica non disponibile.",
                        ToastLevel::Error,
                    );
                    return Ok(());
                };
                let Some(to_index) = ids.iter().position(|id| *id == to_id) else {
                    self.set_toast(
                        "Flow archiviato: modifica non disponibile.",
                        ToastLevel::Error,
                    );
                    return Ok(());
                };

                self.state.transactions.transfer = TransferFormState {
                    from_index,
                    to_index,
                    amount,
                    note: detail.transaction.note.clone().unwrap_or_default(),
                    occurred_at,
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
            default_wallet_flow(&self.state)?;
        let wallet_ids = self.ordered_wallet_ids();
        let flow_ids = self.ordered_flow_ids();
        if wallet_ids.is_empty() {
            return Err("Nessun wallet disponibile.".to_string());
        }
        if flow_ids.is_empty() {
            return Err("Nessun flow disponibile.".to_string());
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

    pub(crate) fn select_category_next(&mut self) {
        let categories = self.state.transactions.recent_categories.clone();
        if categories.is_empty() {
            return;
        }
        let form = &mut self.state.transactions.form;
        let next = match form.category_index {
            Some(idx) => (idx + 1) % categories.len(),
            None => 0,
        };
        form.category_index = Some(next);
        form.category = categories[next].clone();
    }

    pub(crate) fn select_category_prev(&mut self) {
        let categories = self.state.transactions.recent_categories.clone();
        if categories.is_empty() {
            return;
        }
        let form = &mut self.state.transactions.form;
        let prev = match form.category_index {
            Some(idx) => (idx + categories.len() - 1) % categories.len(),
            None => categories.len() - 1,
        };
        form.category_index = Some(prev);
        form.category = categories[prev].clone();
    }
    pub(crate) fn set_transaction_form_error(&mut self, message: &str) {
        self.state.transactions.form.error = Some(message.to_string());
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
    pub(crate) fn open_filter(&mut self) {
        let from_input = self
            .state
            .transactions
            .filter_from
            .map(|dt| self.format_local_datetime(dt))
            .unwrap_or_default();
        let to_input = self
            .state
            .transactions
            .filter_to
            .map(|dt| self.format_local_datetime(dt))
            .unwrap_or_default();
        let kind_income = self.has_kind(api_types::transaction::TransactionKind::Income);
        let kind_expense = self.has_kind(api_types::transaction::TransactionKind::Expense);
        let kind_refund = self.has_kind(api_types::transaction::TransactionKind::Refund);
        let kind_transfer_wallet =
            self.has_kind(api_types::transaction::TransactionKind::TransferWallet);
        let kind_transfer_flow =
            self.has_kind(api_types::transaction::TransactionKind::TransferFlow);

        let filter = &mut self.state.transactions.filter;
        filter.error = None;
        filter.focus = FilterField::From;
        filter.from_input = from_input;
        filter.to_input = to_input;
        filter.kind_income = kind_income;
        filter.kind_expense = kind_expense;
        filter.kind_refund = kind_refund;
        filter.kind_transfer_wallet = kind_transfer_wallet;
        filter.kind_transfer_flow = kind_transfer_flow;

        self.state.transactions.mode = TransactionsMode::Filter;
    }
    pub(crate) fn has_kind(&self, kind: api_types::transaction::TransactionKind) -> bool {
        self.state
            .transactions
            .filter_kinds
            .as_ref()
            .map(|kinds| kinds.contains(&kind))
            .unwrap_or(false)
    }
    pub(crate) fn select_transaction_by_id(&mut self, transaction_id: uuid::Uuid) -> bool {
        let indices = transactions_visible_indices(&self.state);
        for (visible_idx, idx) in indices.iter().enumerate() {
            if self
                .state
                .transactions
                .items
                .get(*idx)
                .map(|tx| tx.id == transaction_id)
                .unwrap_or(false)
            {
                self.state.transactions.selected = visible_idx;
                return true;
            }
        }
        false
    }
}
