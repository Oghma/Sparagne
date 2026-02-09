//! Command structs for engine operations.
//!
//! These types group parameters for write operations
//! (income/expense/transfer/update), keeping call sites readable and avoiding
//! long argument lists.

use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use crate::recurring_templates::RecurrenceFrequency;

/// Common metadata for transaction creation.
#[derive(Clone, Debug)]
pub struct TxMeta {
    /// Optional canonical category id (takes precedence over `category`).
    pub category_id: Option<Uuid>,
    pub category: Option<String>,
    pub note: Option<String>,
    pub idempotency_key: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

impl TxMeta {
    #[must_use]
    pub fn new(occurred_at: DateTime<Utc>) -> Self {
        Self {
            category_id: None,
            category: None,
            note: None,
            idempotency_key: None,
            occurred_at,
        }
    }

    /// Set a canonical category id (takes precedence over `category`).
    #[must_use]
    pub fn category_id(mut self, category_id: Uuid) -> Self {
        self.category_id = Some(category_id);
        self
    }

    #[must_use]
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    #[must_use]
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }
}

/// Create an income transaction.
#[derive(Clone, Debug)]
pub struct IncomeCmd {
    pub vault_id: String,
    pub amount_minor: i64,
    pub flow_id: Option<Uuid>,
    pub wallet_id: Option<Uuid>,
    pub meta: TxMeta,
    pub user_id: String,
}

impl IncomeCmd {
    #[must_use]
    pub fn new(
        vault_id: impl Into<String>,
        user_id: impl Into<String>,
        amount_minor: i64,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            vault_id: vault_id.into(),
            amount_minor,
            flow_id: None,
            wallet_id: None,
            meta: TxMeta::new(occurred_at),
            user_id: user_id.into(),
        }
    }

    #[must_use]
    pub fn flow_id(mut self, flow_id: Uuid) -> Self {
        self.flow_id = Some(flow_id);
        self
    }

    #[must_use]
    pub fn wallet_id(mut self, wallet_id: Uuid) -> Self {
        self.wallet_id = Some(wallet_id);
        self
    }

    #[must_use]
    pub fn meta(mut self, meta: TxMeta) -> Self {
        self.meta = meta;
        self
    }

    #[must_use]
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.meta.category = Some(category.into());
        self
    }

    #[must_use]
    pub fn category_id(mut self, category_id: Uuid) -> Self {
        self.meta.category_id = Some(category_id);
        self
    }

    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.meta.note = Some(note.into());
        self
    }

    #[must_use]
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.meta.idempotency_key = Some(key.into());
        self
    }
}

/// Create an expense transaction.
#[derive(Clone, Debug)]
pub struct ExpenseCmd {
    pub vault_id: String,
    pub amount_minor: i64,
    pub flow_id: Option<Uuid>,
    pub wallet_id: Option<Uuid>,
    pub meta: TxMeta,
    pub user_id: String,
}

impl ExpenseCmd {
    #[must_use]
    pub fn new(
        vault_id: impl Into<String>,
        user_id: impl Into<String>,
        amount_minor: i64,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            vault_id: vault_id.into(),
            amount_minor,
            flow_id: None,
            wallet_id: None,
            meta: TxMeta::new(occurred_at),
            user_id: user_id.into(),
        }
    }

    #[must_use]
    pub fn flow_id(mut self, flow_id: Uuid) -> Self {
        self.flow_id = Some(flow_id);
        self
    }

    #[must_use]
    pub fn wallet_id(mut self, wallet_id: Uuid) -> Self {
        self.wallet_id = Some(wallet_id);
        self
    }

    #[must_use]
    pub fn meta(mut self, meta: TxMeta) -> Self {
        self.meta = meta;
        self
    }

    #[must_use]
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.meta.category = Some(category.into());
        self
    }

    #[must_use]
    pub fn category_id(mut self, category_id: Uuid) -> Self {
        self.meta.category_id = Some(category_id);
        self
    }

    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.meta.note = Some(note.into());
        self
    }

    #[must_use]
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.meta.idempotency_key = Some(key.into());
        self
    }
}

/// Create a refund transaction.
#[derive(Clone, Debug)]
pub struct RefundCmd {
    pub vault_id: String,
    pub amount_minor: i64,
    pub flow_id: Option<Uuid>,
    pub wallet_id: Option<Uuid>,
    pub meta: TxMeta,
    pub user_id: String,
}

impl RefundCmd {
    #[must_use]
    pub fn new(
        vault_id: impl Into<String>,
        user_id: impl Into<String>,
        amount_minor: i64,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            vault_id: vault_id.into(),
            amount_minor,
            flow_id: None,
            wallet_id: None,
            meta: TxMeta::new(occurred_at),
            user_id: user_id.into(),
        }
    }

    #[must_use]
    pub fn flow_id(mut self, flow_id: Uuid) -> Self {
        self.flow_id = Some(flow_id);
        self
    }

    #[must_use]
    pub fn wallet_id(mut self, wallet_id: Uuid) -> Self {
        self.wallet_id = Some(wallet_id);
        self
    }

    #[must_use]
    pub fn meta(mut self, meta: TxMeta) -> Self {
        self.meta = meta;
        self
    }

    #[must_use]
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.meta.category = Some(category.into());
        self
    }

    #[must_use]
    pub fn category_id(mut self, category_id: Uuid) -> Self {
        self.meta.category_id = Some(category_id);
        self
    }

    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.meta.note = Some(note.into());
        self
    }

    #[must_use]
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.meta.idempotency_key = Some(key.into());
        self
    }
}

/// Create a wallet-to-wallet transfer transaction.
#[derive(Clone, Debug)]
pub struct TransferWalletCmd {
    pub vault_id: String,
    pub amount_minor: i64,
    pub from_wallet_id: Uuid,
    pub to_wallet_id: Uuid,
    pub note: Option<String>,
    pub idempotency_key: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub user_id: String,
}

impl TransferWalletCmd {
    #[must_use]
    pub fn new(
        vault_id: impl Into<String>,
        user_id: impl Into<String>,
        amount_minor: i64,
        from_wallet_id: Uuid,
        to_wallet_id: Uuid,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            vault_id: vault_id.into(),
            amount_minor,
            from_wallet_id,
            to_wallet_id,
            note: None,
            idempotency_key: None,
            occurred_at,
            user_id: user_id.into(),
        }
    }

    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    #[must_use]
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }
}

/// Create a flow-to-flow transfer transaction.
#[derive(Clone, Debug)]
pub struct TransferFlowCmd {
    pub vault_id: String,
    pub amount_minor: i64,
    pub from_flow_id: Uuid,
    pub to_flow_id: Uuid,
    pub note: Option<String>,
    pub idempotency_key: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub user_id: String,
}

impl TransferFlowCmd {
    #[must_use]
    pub fn new(
        vault_id: impl Into<String>,
        user_id: impl Into<String>,
        amount_minor: i64,
        from_flow_id: Uuid,
        to_flow_id: Uuid,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            vault_id: vault_id.into(),
            amount_minor,
            from_flow_id,
            to_flow_id,
            note: None,
            idempotency_key: None,
            occurred_at,
            user_id: user_id.into(),
        }
    }

    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    #[must_use]
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }
}

/// Update an existing transaction.
#[derive(Clone, Debug)]
pub struct UpdateTransactionCmd {
    pub vault_id: String,
    pub transaction_id: Uuid,
    pub user_id: String,

    pub amount_minor: Option<i64>,

    // Income/Expense/Refund retargeting.
    pub wallet_id: Option<Uuid>,
    pub flow_id: Option<Uuid>,

    // TransferWallet retargeting.
    pub from_wallet_id: Option<Uuid>,
    pub to_wallet_id: Option<Uuid>,

    // TransferFlow retargeting.
    pub from_flow_id: Option<Uuid>,
    pub to_flow_id: Option<Uuid>,

    pub category_id: Option<Uuid>,
    pub category: Option<String>,
    pub note: Option<String>,
    pub occurred_at: Option<DateTime<Utc>>,
}

impl UpdateTransactionCmd {
    #[must_use]
    pub fn new(
        vault_id: impl Into<String>,
        transaction_id: Uuid,
        user_id: impl Into<String>,
    ) -> Self {
        Self {
            vault_id: vault_id.into(),
            transaction_id,
            user_id: user_id.into(),
            amount_minor: None,
            wallet_id: None,
            flow_id: None,
            from_wallet_id: None,
            to_wallet_id: None,
            from_flow_id: None,
            to_flow_id: None,
            category_id: None,
            category: None,
            note: None,
            occurred_at: None,
        }
    }

    #[must_use]
    pub fn amount_minor(mut self, amount_minor: i64) -> Self {
        self.amount_minor = Some(amount_minor);
        self
    }

    #[must_use]
    pub fn wallet_id(mut self, wallet_id: Uuid) -> Self {
        self.wallet_id = Some(wallet_id);
        self
    }

    #[must_use]
    pub fn flow_id(mut self, flow_id: Uuid) -> Self {
        self.flow_id = Some(flow_id);
        self
    }

    #[must_use]
    pub fn from_wallet_id(mut self, wallet_id: Uuid) -> Self {
        self.from_wallet_id = Some(wallet_id);
        self
    }

    #[must_use]
    pub fn to_wallet_id(mut self, wallet_id: Uuid) -> Self {
        self.to_wallet_id = Some(wallet_id);
        self
    }

    #[must_use]
    pub fn from_flow_id(mut self, flow_id: Uuid) -> Self {
        self.from_flow_id = Some(flow_id);
        self
    }

    #[must_use]
    pub fn to_flow_id(mut self, flow_id: Uuid) -> Self {
        self.to_flow_id = Some(flow_id);
        self
    }

    #[must_use]
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Set a canonical category id (takes precedence over `category`).
    #[must_use]
    pub fn category_id(mut self, category_id: Uuid) -> Self {
        self.category_id = Some(category_id);
        self
    }

    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    #[must_use]
    pub fn occurred_at(mut self, occurred_at: DateTime<Utc>) -> Self {
        self.occurred_at = Some(occurred_at);
        self
    }
}

/// Create a recurring transaction template.
#[derive(Clone, Debug)]
pub struct CreateRecurringCmd {
    pub vault_id: String,
    pub user_id: String,
    pub kind: crate::TransactionKind,
    pub amount_minor: i64,
    pub wallet_id: Option<Uuid>,
    pub flow_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub category: Option<String>,
    pub note: Option<String>,
    pub frequency: RecurrenceFrequency,
    pub day_of_period: i32,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
}

impl CreateRecurringCmd {
    #[must_use]
    pub fn new(
        vault_id: impl Into<String>,
        user_id: impl Into<String>,
        kind: crate::TransactionKind,
        amount_minor: i64,
        frequency: RecurrenceFrequency,
        day_of_period: i32,
        start_date: NaiveDate,
    ) -> Self {
        Self {
            vault_id: vault_id.into(),
            user_id: user_id.into(),
            kind,
            amount_minor,
            wallet_id: None,
            flow_id: None,
            category_id: None,
            category: None,
            note: None,
            frequency,
            day_of_period,
            start_date,
            end_date: None,
        }
    }

    #[must_use]
    pub fn wallet_id(mut self, wallet_id: Uuid) -> Self {
        self.wallet_id = Some(wallet_id);
        self
    }

    #[must_use]
    pub fn flow_id(mut self, flow_id: Uuid) -> Self {
        self.flow_id = Some(flow_id);
        self
    }

    #[must_use]
    pub fn category_id(mut self, category_id: Uuid) -> Self {
        self.category_id = Some(category_id);
        self
    }

    #[must_use]
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    #[must_use]
    pub fn end_date(mut self, end_date: NaiveDate) -> Self {
        self.end_date = Some(end_date);
        self
    }
}

/// Update an existing recurring template.
#[derive(Clone, Debug)]
pub struct UpdateRecurringCmd {
    pub vault_id: String,
    pub template_id: Uuid,
    pub user_id: String,
    pub amount_minor: Option<i64>,
    pub wallet_id: Option<Uuid>,
    pub flow_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub category: Option<String>,
    pub note: Option<String>,
    pub frequency: Option<RecurrenceFrequency>,
    pub day_of_period: Option<i32>,
    pub end_date: Option<Option<NaiveDate>>,
    pub enabled: Option<bool>,
}

impl UpdateRecurringCmd {
    #[must_use]
    pub fn new(
        vault_id: impl Into<String>,
        template_id: Uuid,
        user_id: impl Into<String>,
    ) -> Self {
        Self {
            vault_id: vault_id.into(),
            template_id,
            user_id: user_id.into(),
            amount_minor: None,
            wallet_id: None,
            flow_id: None,
            category_id: None,
            category: None,
            note: None,
            frequency: None,
            day_of_period: None,
            end_date: None,
            enabled: None,
        }
    }

    #[must_use]
    pub fn amount_minor(mut self, v: i64) -> Self {
        self.amount_minor = Some(v);
        self
    }

    #[must_use]
    pub fn wallet_id(mut self, v: Uuid) -> Self {
        self.wallet_id = Some(v);
        self
    }

    #[must_use]
    pub fn flow_id(mut self, v: Uuid) -> Self {
        self.flow_id = Some(v);
        self
    }

    #[must_use]
    pub fn category_id(mut self, v: Uuid) -> Self {
        self.category_id = Some(v);
        self
    }

    #[must_use]
    pub fn category(mut self, v: impl Into<String>) -> Self {
        self.category = Some(v.into());
        self
    }

    #[must_use]
    pub fn note(mut self, v: impl Into<String>) -> Self {
        self.note = Some(v.into());
        self
    }

    #[must_use]
    pub fn frequency(mut self, v: RecurrenceFrequency) -> Self {
        self.frequency = Some(v);
        self
    }

    #[must_use]
    pub fn day_of_period(mut self, v: i32) -> Self {
        self.day_of_period = Some(v);
        self
    }

    #[must_use]
    pub fn end_date(mut self, v: Option<NaiveDate>) -> Self {
        self.end_date = Some(v);
        self
    }

    #[must_use]
    pub fn enabled(mut self, v: bool) -> Self {
        self.enabled = Some(v);
        self
    }
}
