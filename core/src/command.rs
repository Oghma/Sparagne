//! Commands: the only way to change a vault.

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Currency, DomainError, FlowMode};

/// Kind of a transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionKind {
    Income,
    Expense,
    Refund,
    TransferWallet,
    TransferFlow,
}

impl TransactionKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Income => "income",
            Self::Expense => "expense",
            Self::Refund => "refund",
            Self::TransferWallet => "transfer_wallet",
            Self::TransferFlow => "transfer_flow",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "income" => Ok(Self::Income),
            "expense" => Ok(Self::Expense),
            "refund" => Ok(Self::Refund),
            "transfer_wallet" => Ok(Self::TransferWallet),
            "transfer_flow" => Ok(Self::TransferFlow),
            other => Err(DomainError::Storage(format!("unknown kind '{other}'"))),
        }
    }

    /// Transfers never count in statistics.
    #[must_use]
    pub const fn is_transfer(self) -> bool {
        matches!(self, Self::TransferWallet | Self::TransferFlow)
    }
}

/// Payload shared by income, expense and refund.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Absolute amount in minor units, `> 0`.
    pub amount: i64,
    /// `None` = the only active wallet of the vault.
    pub wallet_id: Option<Uuid>,
    /// `None` = the vault's Unallocated flow.
    pub flow_id: Option<Uuid>,
    /// Free text resolved by key, then alias; blank = Uncategorized.
    pub category: Option<String>,
    pub note: Option<String>,
    pub occurred_at: DateTime<FixedOffset>,
}

/// One unit of change. Serialized as JSON in the log with a `kind` tag.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Command {
    /// The vault id is the command id.
    CreateVault {
        name: String,
        currency: Currency,
    },
    /// Non-zero `opening_balance` creates an opening transaction on
    /// Unallocated with the system category `opening`.
    CreateWallet {
        name: String,
        opening_balance: i64,
        occurred_at: DateTime<FixedOffset>,
    },
    /// Positive `opening_allocation` moves money from Unallocated into the new
    /// flow, subject to its cap.
    CreateFlow {
        name: String,
        mode: FlowMode,
        allow_negative: bool,
        opening_allocation: i64,
        occurred_at: DateTime<FixedOffset>,
    },
    CreateCategory {
        name: String,
    },
    Income(Entry),
    Expense(Entry),
    Refund(Entry),
    TransferWallet {
        amount: i64,
        from_wallet_id: Uuid,
        to_wallet_id: Uuid,
        note: Option<String>,
        occurred_at: DateTime<FixedOffset>,
    },
    TransferFlow {
        amount: i64,
        from_flow_id: Uuid,
        to_flow_id: Uuid,
        note: Option<String>,
        occurred_at: DateTime<FixedOffset>,
    },
    /// Soft delete. Never blocked by caps or non-negativity.
    VoidTransaction {
        transaction_id: Uuid,
    },
}

impl Command {
    /// Tag stored in `commands.kind`.
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::CreateVault { .. } => "create_vault",
            Self::CreateWallet { .. } => "create_wallet",
            Self::CreateFlow { .. } => "create_flow",
            Self::CreateCategory { .. } => "create_category",
            Self::Income(_) => "income",
            Self::Expense(_) => "expense",
            Self::Refund(_) => "refund",
            Self::TransferWallet { .. } => "transfer_wallet",
            Self::TransferFlow { .. } => "transfer_flow",
            Self::VoidTransaction { .. } => "void_transaction",
        }
    }

    /// When the user says it happened, for transaction-like commands.
    #[must_use]
    pub fn occurred_at(&self) -> Option<DateTime<FixedOffset>> {
        match self {
            Self::CreateWallet { occurred_at, .. }
            | Self::CreateFlow { occurred_at, .. }
            | Self::TransferWallet { occurred_at, .. }
            | Self::TransferFlow { occurred_at, .. } => Some(*occurred_at),
            Self::Income(e) | Self::Expense(e) | Self::Refund(e) => Some(e.occurred_at),
            Self::CreateVault { .. }
            | Self::CreateCategory { .. }
            | Self::VoidTransaction { .. } => None,
        }
    }
}

/// A command addressed to a vault by an author.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandEnvelope {
    /// Client-generated UUID v7; idempotency key and id of what gets created.
    pub id: Uuid,
    pub vault_id: Uuid,
    pub author: String,
    pub command: Command,
}

impl CommandEnvelope {
    /// New envelope with a fresh v7 id.
    #[must_use]
    pub fn new(vault_id: Uuid, author: impl Into<String>, command: Command) -> Self {
        Self {
            id: Uuid::now_v7(),
            vault_id,
            author: author.into(),
            command,
        }
    }

    /// `CreateVault` envelope: the vault id equals the command id.
    #[must_use]
    pub fn create_vault(
        author: impl Into<String>,
        name: impl Into<String>,
        currency: Currency,
    ) -> Self {
        let id = Uuid::now_v7();
        Self {
            id,
            vault_id: id,
            author: author.into(),
            command: Command::CreateVault {
                name: name.into(),
                currency,
            },
        }
    }
}

/// Outcome of `execute`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub command_id: Uuid,
    /// Position in the vault log.
    pub seq: i64,
    /// Id of the entity created by the command, when any.
    pub result_id: Option<Uuid>,
    /// `true` when the command id was already in the log; nothing was applied.
    pub deduplicated: bool,
}

/// A command as stored in the log.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandRecord {
    pub envelope: CommandEnvelope,
    pub seq: i64,
    pub created_at: i64,
    pub result_id: Option<Uuid>,
}
