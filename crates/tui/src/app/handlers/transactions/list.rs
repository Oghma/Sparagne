//! Transaction list operations.
//!
//! Contains methods for transaction list selection, filtering, and navigation.

use crate::app::{App, FilterField, TransactionsMode, transactions_visible_indices};

impl App {
    pub(crate) fn selected_transaction(&self) -> Option<&api_types::transaction::TransactionView> {
        let indices = transactions_visible_indices(&self.state);
        let index = indices.get(self.state.transactions.selected).copied()?;
        self.state.transactions.items.get(index)
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
        filter.include_transfers = self.state.transactions.include_transfers;

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
