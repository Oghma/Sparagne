use chrono::{DateTime, FixedOffset};

use api_types::transaction::{TransactionDetailResponse, TransactionKind, TransactionView};

use crate::app::helpers::{normalize_query, transaction_matches_query};

#[derive(Debug)]
pub struct TransactionsState {
    pub items: Vec<TransactionView>,
    pub cursor: Option<String>,
    pub next_cursor: Option<String>,
    pub prev_cursors: Vec<Option<String>>,
    pub selected: usize,
    pub scope_wallet_id: Option<uuid::Uuid>,
    pub scope_flow_id: Option<uuid::Uuid>,
    pub picker_index: usize,
    pub include_voided: bool,
    pub include_transfers: bool,
    pub error: Option<String>,
    pub mode: TransactionsMode,
    pub detail: Option<TransactionDetailResponse>,
    pub quick_input: String,
    pub quick_error: Option<String>,
    pub quick_active: bool,
    pub transfer: TransferFormState,
    pub form: TransactionFormState,
    pub filter_from: Option<DateTime<FixedOffset>>,
    pub filter_to: Option<DateTime<FixedOffset>>,
    pub filter_kinds: Option<Vec<TransactionKind>>,
    pub filter: TransactionsFilterState,
    pub last_created_id: Option<uuid::Uuid>,
    pub recent_categories: Vec<String>,
    pub recent_wallet_ids: Vec<uuid::Uuid>,
    pub recent_flow_ids: Vec<uuid::Uuid>,
    pub search_query: String,
    pub search_active: bool,
}

impl Default for TransactionsState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            cursor: None,
            next_cursor: None,
            prev_cursors: Vec::new(),
            selected: 0,
            scope_wallet_id: None,
            scope_flow_id: None,
            picker_index: 0,
            include_voided: false,
            include_transfers: false,
            error: None,
            mode: TransactionsMode::List,
            detail: None,
            quick_input: String::new(),
            quick_error: None,
            quick_active: false,
            transfer: TransferFormState::default(),
            form: TransactionFormState::default(),
            filter_from: None,
            filter_to: None,
            filter_kinds: None,
            filter: TransactionsFilterState::default(),
            last_created_id: None,
            recent_categories: Vec::new(),
            recent_wallet_ids: Vec::new(),
            recent_flow_ids: Vec::new(),
            search_query: String::new(),
            search_active: false,
        }
    }
}

impl TransactionsState {
    pub(crate) fn reset(&mut self) {
        self.cursor = None;
        self.next_cursor = None;
        self.prev_cursors.clear();
        self.items.clear();
        self.selected = 0;
        self.mode = TransactionsMode::List;
        self.detail = None;
        self.quick_input.clear();
        self.quick_error = None;
        self.quick_active = false;
        self.transfer = TransferFormState::default();
        self.form = TransactionFormState::default();
        self.filter = TransactionsFilterState::default();
        self.last_created_id = None;
        self.recent_categories.clear();
        self.recent_wallet_ids.clear();
        self.recent_flow_ids.clear();
    }

    pub(crate) fn push_cursor(&mut self, cursor: Option<String>) {
        self.prev_cursors.push(cursor);
    }

    pub(crate) fn pop_cursor(&mut self) -> Option<Option<String>> {
        self.prev_cursors.pop()
    }

    pub(crate) fn select_next(&mut self) {
        let visible_len = self.visible_len();
        if visible_len == 0 {
            return;
        }
        self.selected = (self.selected + 1).min(visible_len - 1);
    }

    pub(crate) fn select_prev(&mut self) {
        let visible_len = self.visible_len();
        if visible_len == 0 {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
    }

    pub(crate) fn visible_len(&self) -> usize {
        let query = normalize_query(self.search_query.as_str());
        if query.is_empty() {
            return self.items.len();
        }

        self.items
            .iter()
            .filter(|tx| transaction_matches_query(tx, query.as_str()))
            .count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionsMode {
    List,
    Detail,
    Edit,
    Form,
    PickWallet,
    PickFlow,
    TransferWallet,
    TransferFlow,
    Filter,
}

#[derive(Debug)]
pub struct TransferFormState {
    pub from_index: usize,
    pub to_index: usize,
    pub amount: String,
    pub note: String,
    pub occurred_at: String,
    pub focus: TransferField,
    pub error: Option<String>,
    pub editing_id: Option<uuid::Uuid>,
}

impl Default for TransferFormState {
    fn default() -> Self {
        Self {
            from_index: 0,
            to_index: 1,
            amount: String::new(),
            note: String::new(),
            occurred_at: String::new(),
            focus: TransferField::From,
            error: None,
            editing_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferField {
    From,
    To,
    Amount,
    Note,
    OccurredAt,
}

#[derive(Debug)]
pub struct TransactionFormState {
    pub kind: TransactionKind,
    pub amount: String,
    pub wallet_index: usize,
    pub flow_index: usize,
    pub category: String,
    pub note: String,
    pub occurred_at: String,
    pub focus: TransactionFormField,
    pub error: Option<String>,
    pub category_index: Option<usize>,
    pub editing_id: Option<uuid::Uuid>,
}

impl Default for TransactionFormState {
    fn default() -> Self {
        Self {
            kind: TransactionKind::Expense,
            amount: String::new(),
            wallet_index: 0,
            flow_index: 0,
            category: String::new(),
            note: String::new(),
            occurred_at: String::new(),
            focus: TransactionFormField::Amount,
            error: None,
            category_index: None,
            editing_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionFormField {
    Amount,
    Wallet,
    Flow,
    Category,
    Note,
    OccurredAt,
}

#[derive(Debug)]
pub struct TransactionsFilterState {
    pub from_input: String,
    pub to_input: String,
    pub focus: FilterField,
    pub error: Option<String>,
    pub kind_income: bool,
    pub kind_expense: bool,
    pub kind_refund: bool,
    pub kind_transfer_wallet: bool,
    pub kind_transfer_flow: bool,
}

impl Default for TransactionsFilterState {
    fn default() -> Self {
        Self {
            from_input: String::new(),
            to_input: String::new(),
            focus: FilterField::From,
            error: None,
            kind_income: false,
            kind_expense: false,
            kind_refund: false,
            kind_transfer_wallet: false,
            kind_transfer_flow: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterField {
    From,
    To,
    Kinds,
}
