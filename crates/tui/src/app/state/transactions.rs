use chrono::{DateTime, FixedOffset};
use std::collections::BTreeSet;

use api_types::transaction::{TransactionDetailResponse, TransactionKind, TransactionView};
use uuid::Uuid;

use crate::app::helpers::{normalize_query, transaction_matches_query};

/// Represents an ambiguous match in quick-add where multiple options are
/// available.
#[derive(Debug, Clone)]
pub struct QuickAddAmbiguous {
    pub kind: QuickAddAmbiguousKind,
    pub query: String,
    pub options: Vec<(Uuid, String)>, // (id, name)
    pub selected: usize,
}

impl QuickAddAmbiguous {
    pub fn new(kind: QuickAddAmbiguousKind, query: String, options: Vec<(Uuid, String)>) -> Self {
        Self {
            kind,
            query,
            options,
            selected: 0,
        }
    }

    pub fn cycle_next(&mut self) {
        if !self.options.is_empty() {
            self.selected = (self.selected + 1) % self.options.len();
        }
    }

    pub fn cycle_prev(&mut self) {
        if !self.options.is_empty() {
            self.selected = (self.selected + self.options.len() - 1) % self.options.len();
        }
    }

    pub fn current(&self) -> Option<&(Uuid, String)> {
        self.options.get(self.selected)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickAddAmbiguousKind {
    Category,
    Wallet,
    Flow,
}

#[derive(Debug)]
pub struct TransactionsState {
    pub items: Vec<TransactionView>,
    pub cursor: Option<String>,
    pub next_cursor: Option<String>,
    pub prev_cursors: Vec<Option<String>>,
    pub selected: usize,
    pub pending_delete_ids: BTreeSet<uuid::Uuid>,
    pub visual_mode: bool,
    pub visual_selected: BTreeSet<uuid::Uuid>,
    pub scope_wallet_id: Option<uuid::Uuid>,
    pub scope_flow_id: Option<uuid::Uuid>,
    pub picker_index: usize,
    pub include_voided: bool,
    pub include_transfers: bool,
    pub error: Option<String>,
    pub mode: TransactionsMode,
    pub grouping_mode: GroupingMode,
    pub detail: Option<TransactionDetailResponse>,
    pub quick_input: String,
    pub quick_error: Option<String>,
    pub quick_active: bool,
    pub quick_ambiguous: Option<QuickAddAmbiguous>,
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
            pending_delete_ids: BTreeSet::new(),
            visual_mode: false,
            visual_selected: BTreeSet::new(),
            scope_wallet_id: None,
            scope_flow_id: None,
            picker_index: 0,
            include_voided: false,
            include_transfers: false,
            error: None,
            mode: TransactionsMode::List,
            grouping_mode: GroupingMode::Date,
            detail: None,
            quick_input: String::new(),
            quick_error: None,
            quick_active: false,
            quick_ambiguous: None,
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
        self.pending_delete_ids.clear();
        self.visual_mode = false;
        self.visual_selected.clear();
        self.mode = TransactionsMode::List;
        self.grouping_mode = GroupingMode::Date;
        self.detail = None;
        self.quick_input.clear();
        self.quick_error = None;
        self.quick_active = false;
        self.quick_ambiguous = None;
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
        self.items
            .iter()
            .filter(|tx| {
                if self.pending_delete_ids.contains(&tx.id) {
                    return false;
                }
                if query.is_empty() {
                    return true;
                }
                transaction_matches_query(tx, query.as_str())
            })
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupingMode {
    Date,
    Category,
    Wallet,
    Envelope,
}

impl GroupingMode {
    pub const ALL: [Self; 4] = [Self::Date, Self::Category, Self::Wallet, Self::Envelope];

    pub fn index(self) -> usize {
        match self {
            Self::Date => 0,
            Self::Category => 1,
            Self::Wallet => 2,
            Self::Envelope => 3,
        }
    }

    pub fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Category,
            2 => Self::Wallet,
            3 => Self::Envelope,
            _ => Self::Date,
        }
    }

    pub fn next(self) -> Self {
        Self::from_index((self.index() + 1) % Self::ALL.len())
    }

    pub fn prev(self) -> Self {
        let len = Self::ALL.len();
        Self::from_index((self.index() + len - 1) % len)
    }
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

impl TransferFormState {
    pub(crate) fn is_dirty(&self) -> bool {
        self.editing_id.is_some() || !self.amount.trim().is_empty() || !self.note.trim().is_empty()
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

impl TransactionFormState {
    pub(crate) fn is_dirty(&self) -> bool {
        self.editing_id.is_some()
            || !self.amount.trim().is_empty()
            || !self.category.trim().is_empty()
            || !self.note.trim().is_empty()
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
