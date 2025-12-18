use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    #[default]
    Eur,
}

pub mod cash_flow {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct CashFlowGet {
        pub vault_id: String,
        /// Cash flow id (UUID).
        ///
        /// This is serialized as a string in JSON.
        pub id: Option<Uuid>,
        /// Cash flow name (legacy convenience).
        pub name: Option<String>,
    }
}

pub mod vault {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct VaultNew {
        pub name: String,
        pub currency: Option<Currency>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Vault {
        pub id: Option<String>,
        pub name: Option<String>,
        pub currency: Option<Currency>,
    }

    /// A vault snapshot for UI clients (bot/TUI).
    ///
    /// This is a single "read model" response that includes wallets and flows,
    /// so clients don't need DB access or multiple ad-hoc queries.
    #[derive(Debug, Serialize, Deserialize)]
    pub struct VaultSnapshot {
        pub id: String,
        pub name: String,
        pub currency: Currency,
        pub wallets: Vec<WalletView>,
        pub flows: Vec<FlowView>,
        pub unallocated_flow_id: Uuid,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct WalletView {
        pub id: Uuid,
        pub name: String,
        pub balance_minor: i64,
        pub archived: bool,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct FlowView {
        pub id: Uuid,
        pub name: String,
        pub balance_minor: i64,
        pub archived: bool,
        pub is_unallocated: bool,
    }
}

pub mod user {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct PairUser {
        pub code: String,
        pub telegram_id: String,
    }
}

pub mod membership {
    use super::*;

    /// Role of a user in a shared resource (vault or flow).
    ///
    /// The server treats roles as:
    /// - `owner`: full access and can manage members.
    /// - `editor`: can write but cannot manage members.
    /// - `viewer`: read-only.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum MembershipRole {
        Owner,
        Editor,
        Viewer,
    }

    impl MembershipRole {
        /// Returns the canonical role string used by the engine/database.
        pub fn as_str(self) -> &'static str {
            match self {
                Self::Owner => "owner",
                Self::Editor => "editor",
                Self::Viewer => "viewer",
            }
        }
    }

    /// Request body for adding/updating a member.
    #[derive(Debug, Serialize, Deserialize)]
    pub struct MemberUpsert {
        pub username: String,
        pub role: MembershipRole,
    }

    /// Response body for listing members.
    #[derive(Debug, Serialize, Deserialize)]
    pub struct MembersResponse {
        pub members: Vec<MemberView>,
    }

    /// A member with their role.
    #[derive(Debug, Serialize, Deserialize)]
    pub struct MemberView {
        pub username: String,
        pub role: MembershipRole,
    }
}

pub mod stats {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Statistic {
        pub currency: Currency,
        pub balance_minor: i64,
        pub total_income_minor: i64,
        pub total_expenses_minor: i64,
    }
}

pub mod transaction {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum TransactionKind {
        Income,
        Expense,
        TransferWallet,
        TransferFlow,
        Refund,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct TransactionList {
        pub vault_id: String,
        pub flow_id: Option<Uuid>,
        pub wallet_id: Option<Uuid>,
        pub limit: Option<u64>,
        /// Opaque pagination cursor (base64), from `next_cursor`.
        ///
        /// Newest → older pagination.
        pub cursor: Option<String>,
        /// Inclusive start of the time range filter (RFC3339, with offset).
        pub from: Option<DateTime<FixedOffset>>,
        /// Exclusive end of the time range filter (RFC3339, with offset).
        pub to: Option<DateTime<FixedOffset>>,
        /// Optional allow-list of kinds to include.
        pub kinds: Option<Vec<TransactionKind>>,
        pub include_voided: Option<bool>,
        pub include_transfers: Option<bool>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct TransactionView {
        pub id: Uuid,
        pub kind: TransactionKind,
        /// RFC3339 timestamp, including timezone offset (local user time).
        pub occurred_at: DateTime<FixedOffset>,
        /// Signed amount for the selected target (wallet/flow).
        pub amount_minor: i64,
        pub category: Option<String>,
        pub note: Option<String>,
        pub voided: bool,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct TransactionListResponse {
        pub transactions: Vec<TransactionView>,
        /// Opaque cursor for fetching the next page (older items).
        pub next_cursor: Option<String>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct TransactionGet {
        pub vault_id: String,
        pub id: Uuid,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(tag = "target", rename_all = "snake_case")]
    pub enum LegTarget {
        Wallet { wallet_id: Uuid },
        Flow { flow_id: Uuid },
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct TransactionLegView {
        #[serde(flatten)]
        pub target: LegTarget,
        pub amount_minor: i64,
        pub attributed_user_id: Option<String>,
        pub currency: Currency,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct TransactionHeaderView {
        pub id: Uuid,
        pub kind: TransactionKind,
        /// RFC3339 timestamp, including timezone offset (local user time).
        pub occurred_at: DateTime<FixedOffset>,
        /// Positive absolute amount of the transaction.
        pub amount_minor: i64,
        pub currency: Currency,
        pub category: Option<String>,
        pub note: Option<String>,
        pub voided: bool,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct TransactionDetailResponse {
        pub transaction: TransactionHeaderView,
        pub legs: Vec<TransactionLegView>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct TransactionCreated {
        pub id: Uuid,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct IncomeNew {
        pub vault_id: String,
        pub amount_minor: i64,
        pub flow_id: Option<Uuid>,
        pub wallet_id: Option<Uuid>,
        pub category: Option<String>,
        pub note: Option<String>,
        /// Optional idempotency key for safely retrying the same create
        /// request.
        pub idempotency_key: Option<String>,
        /// RFC3339 timestamp, including timezone offset (local user time).
        pub occurred_at: DateTime<FixedOffset>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct ExpenseNew {
        pub vault_id: String,
        pub amount_minor: i64,
        pub flow_id: Option<Uuid>,
        pub wallet_id: Option<Uuid>,
        pub category: Option<String>,
        pub note: Option<String>,
        /// Optional idempotency key for safely retrying the same create
        /// request.
        pub idempotency_key: Option<String>,
        /// RFC3339 timestamp, including timezone offset (local user time).
        pub occurred_at: DateTime<FixedOffset>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Refund {
        pub vault_id: String,
        /// Must be > 0. The kind defines the sign of the legs.
        pub amount_minor: i64,
        pub flow_id: Option<Uuid>,
        pub wallet_id: Option<Uuid>,
        pub category: Option<String>,
        pub note: Option<String>,
        /// Optional idempotency key for safely retrying the same create
        /// request.
        pub idempotency_key: Option<String>,
        /// RFC3339 timestamp, including timezone offset (local user time).
        pub occurred_at: DateTime<FixedOffset>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct TransferWalletNew {
        pub vault_id: String,
        pub amount_minor: i64,
        pub from_wallet_id: Uuid,
        pub to_wallet_id: Uuid,
        pub note: Option<String>,
        /// Optional idempotency key for safely retrying the same create
        /// request.
        pub idempotency_key: Option<String>,
        /// RFC3339 timestamp, including timezone offset (local user time).
        pub occurred_at: DateTime<FixedOffset>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct TransferFlowNew {
        pub vault_id: String,
        pub amount_minor: i64,
        pub from_flow_id: Uuid,
        pub to_flow_id: Uuid,
        pub note: Option<String>,
        /// Optional idempotency key for safely retrying the same create
        /// request.
        pub idempotency_key: Option<String>,
        /// RFC3339 timestamp, including timezone offset (local user time).
        pub occurred_at: DateTime<FixedOffset>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct TransactionUpdate {
        pub vault_id: String,
        /// If present, updates the transaction amount (must be `> 0`).
        pub amount_minor: Option<i64>,
        /// If present, updates the wallet target (Income/Expense/Refund).
        pub wallet_id: Option<Uuid>,
        /// If present, updates the flow target (Income/Expense/Refund).
        pub flow_id: Option<Uuid>,
        /// If present, updates the "from" wallet (TransferWallet).
        pub from_wallet_id: Option<Uuid>,
        /// If present, updates the "to" wallet (TransferWallet).
        pub to_wallet_id: Option<Uuid>,
        /// If present, updates the "from" flow (TransferFlow).
        pub from_flow_id: Option<Uuid>,
        /// If present, updates the "to" flow (TransferFlow).
        pub to_flow_id: Option<Uuid>,
        pub category: Option<String>,
        pub note: Option<String>,
        pub occurred_at: Option<DateTime<FixedOffset>>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct TransactionVoid {
        pub vault_id: String,
        /// Optional: if absent, server uses now().
        pub voided_at: Option<DateTime<FixedOffset>>,
    }
}
