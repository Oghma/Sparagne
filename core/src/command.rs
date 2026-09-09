//! Commands: the only way to change a vault.

use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Currency, DomainError, FlowMode, recurring::Schedule};

/// Kind of a transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, uniffi::Enum)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, uniffi::Record)]
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
///
/// Conventions shared by every `Update*` command: an `Option` field left as
/// `None` is untouched; a text field set to a blank string clears it (note)
/// or falls back to the system default (category = Uncategorized).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Command {
    // -- Vault --------------------------------------------------------------
    /// The vault id is the command id.
    CreateVault {
        name: String,
        currency: Currency,
    },

    // -- Wallet -------------------------------------------------------------
    /// Non-zero `opening_balance` creates an opening transaction on
    /// Unallocated with the system category `opening`.
    CreateWallet {
        name: String,
        opening_balance: i64,
        occurred_at: DateTime<FixedOffset>,
    },
    RenameWallet {
        wallet_id: Uuid,
        name: String,
    },
    /// Requires a zero balance. Archived wallets refuse new legs.
    ArchiveWallet {
        wallet_id: Uuid,
    },
    RestoreWallet {
        wallet_id: Uuid,
    },

    // -- Flow ---------------------------------------------------------------
    /// Positive `opening_allocation` moves money from Unallocated into the new
    /// flow, subject to its cap.
    CreateFlow {
        name: String,
        mode: FlowMode,
        allow_negative: bool,
        opening_allocation: i64,
        occurred_at: DateTime<FixedOffset>,
    },
    /// Only the given fields change. A new cap must already hold for the
    /// current balance (net) or the cumulative income (income-capped);
    /// `allow_negative = false` requires a non-negative balance. Unallocated
    /// cannot be updated.
    UpdateFlow {
        flow_id: Uuid,
        name: Option<String>,
        mode: Option<FlowMode>,
        allow_negative: Option<bool>,
    },
    /// Requires a zero balance. Archived flows refuse new legs.
    ArchiveFlow {
        flow_id: Uuid,
    },
    RestoreFlow {
        flow_id: Uuid,
    },

    // -- Category -----------------------------------------------------------
    CreateCategory {
        name: String,
    },
    /// System categories cannot be renamed. Transactions follow the category
    /// by id, so nothing else changes.
    RenameCategory {
        category_id: Uuid,
        name: String,
    },
    /// Archived categories are refused on new and updated transactions;
    /// existing ones keep pointing at them. System categories cannot be
    /// archived.
    ArchiveCategory {
        category_id: Uuid,
    },
    RestoreCategory {
        category_id: Uuid,
    },
    /// `alias` is normalized like a category name and must be unique across
    /// category keys and aliases of the vault. Not allowed on system
    /// categories.
    AddAlias {
        category_id: Uuid,
        alias: String,
    },
    /// Matched by normalized key.
    RemoveAlias {
        category_id: Uuid,
        alias: String,
    },
    /// Repoints every transaction of `source` to `target`, moves the aliases,
    /// adds the source name as an alias of the target (unless the target is
    /// a system category) and deletes the source, so its key is free for the
    /// alias. Refused when `Core::preview_merge` reports conflicts.
    MergeCategory {
        source_id: Uuid,
        target_id: Uuid,
    },

    // -- Transaction --------------------------------------------------------
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
    /// Partial update; the patch must carry at least one field. The kind
    /// never changes (void and recreate instead). Caps and non-negativity are
    /// re-checked on the resulting legs; nothing changes on failure.
    UpdateTransaction {
        transaction_id: Uuid,
        patch: TransactionPatch,
    },
    /// Soft delete. Never blocked by caps or non-negativity.
    VoidTransaction {
        transaction_id: Uuid,
    },

    // -- Recurring ----------------------------------------------------------
    /// A template that the user materializes period by period. Only
    /// `Income` and `Expense` kinds. The template id is the command id.
    CreateRecurring {
        transaction_kind: TransactionKind,
        /// Absolute, `> 0`.
        amount: i64,
        /// `None` = the only active wallet at execution time.
        wallet_id: Option<Uuid>,
        /// `None` = Unallocated.
        flow_id: Option<Uuid>,
        /// Free text, resolved at execution time; blank = Uncategorized.
        category: Option<String>,
        note: Option<String>,
        schedule: Schedule,
    },
    /// Partial update; the patch must carry at least one field. Past runs are
    /// not touched.
    UpdateRecurring {
        recurring_id: Uuid,
        patch: RecurringPatch,
    },
    ArchiveRecurring {
        recurring_id: Uuid,
    },
    RestoreRecurring {
        recurring_id: Uuid,
    },
    /// Materializes the period `period_date` of a template as a transaction
    /// whose id is the command id. `period_date` must be a due date of the
    /// schedule that has not been executed or skipped yet.
    ExecuteRecurring {
        recurring_id: Uuid,
        period_date: NaiveDate,
        occurred_at: DateTime<FixedOffset>,
    },
    /// Marks a due period as handled without a transaction.
    SkipRecurring {
        recurring_id: Uuid,
        period_date: NaiveDate,
    },
}

/// The fields [`Command::UpdateTransaction`] can change.
///
/// Every field is optional and `None` means "leave as is"; a blank string
/// clears the note or puts the category back to Uncategorized. Which fields
/// apply depends on the kind of the transaction being patched: `wallet_id`,
/// `flow_id` and `category` belong to entries, `from_id` and `to_id` to
/// transfers, and mixing the two is refused.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, uniffi::Record)]
#[serde(default)]
pub struct TransactionPatch {
    /// Absolute, `> 0`.
    #[uniffi(default = None)]
    pub amount: Option<i64>,
    #[uniffi(default = None)]
    pub occurred_at: Option<DateTime<FixedOffset>>,
    /// Entries only; blank = Uncategorized.
    #[uniffi(default = None)]
    pub category: Option<String>,
    /// Blank = clear.
    #[uniffi(default = None)]
    pub note: Option<String>,
    /// Entries only.
    #[uniffi(default = None)]
    pub wallet_id: Option<Uuid>,
    /// Entries only.
    #[uniffi(default = None)]
    pub flow_id: Option<Uuid>,
    /// Transfers only; a wallet id or a flow id matching the kind.
    #[uniffi(default = None)]
    pub from_id: Option<Uuid>,
    /// Transfers only.
    #[uniffi(default = None)]
    pub to_id: Option<Uuid>,
}

impl TransactionPatch {
    /// A patch that carries no field at all changes nothing.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.amount.is_none()
            && self.occurred_at.is_none()
            && self.category.is_none()
            && self.note.is_none()
            && self.wallet_id.is_none()
            && self.flow_id.is_none()
            && self.from_id.is_none()
            && self.to_id.is_none()
    }
}

/// The fields [`Command::UpdateRecurring`] can change. `None` leaves the
/// field as it is.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, uniffi::Record)]
#[serde(default)]
pub struct RecurringPatch {
    /// Absolute, `> 0`.
    #[uniffi(default = None)]
    pub amount: Option<i64>,
    #[uniffi(default = None)]
    pub wallet_id: Option<Uuid>,
    #[uniffi(default = None)]
    pub flow_id: Option<Uuid>,
    /// Blank = Uncategorized.
    #[uniffi(default = None)]
    pub category: Option<String>,
    /// Blank = clear.
    #[uniffi(default = None)]
    pub note: Option<String>,
    #[uniffi(default = None)]
    pub schedule: Option<Schedule>,
    /// Disabled templates are never pending.
    #[uniffi(default = None)]
    pub enabled: Option<bool>,
}

impl RecurringPatch {
    /// A patch that carries no field at all changes nothing.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.amount.is_none()
            && self.wallet_id.is_none()
            && self.flow_id.is_none()
            && self.category.is_none()
            && self.note.is_none()
            && self.schedule.is_none()
            && self.enabled.is_none()
    }
}

impl Command {
    /// Tag stored in `commands.kind`.
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::CreateVault { .. } => "create_vault",
            Self::CreateWallet { .. } => "create_wallet",
            Self::RenameWallet { .. } => "rename_wallet",
            Self::ArchiveWallet { .. } => "archive_wallet",
            Self::RestoreWallet { .. } => "restore_wallet",
            Self::CreateFlow { .. } => "create_flow",
            Self::UpdateFlow { .. } => "update_flow",
            Self::ArchiveFlow { .. } => "archive_flow",
            Self::RestoreFlow { .. } => "restore_flow",
            Self::CreateCategory { .. } => "create_category",
            Self::RenameCategory { .. } => "rename_category",
            Self::ArchiveCategory { .. } => "archive_category",
            Self::RestoreCategory { .. } => "restore_category",
            Self::AddAlias { .. } => "add_alias",
            Self::RemoveAlias { .. } => "remove_alias",
            Self::MergeCategory { .. } => "merge_category",
            Self::Income(_) => "income",
            Self::Expense(_) => "expense",
            Self::Refund(_) => "refund",
            Self::TransferWallet { .. } => "transfer_wallet",
            Self::TransferFlow { .. } => "transfer_flow",
            Self::UpdateTransaction { .. } => "update_transaction",
            Self::VoidTransaction { .. } => "void_transaction",
            Self::CreateRecurring { .. } => "create_recurring",
            Self::UpdateRecurring { .. } => "update_recurring",
            Self::ArchiveRecurring { .. } => "archive_recurring",
            Self::RestoreRecurring { .. } => "restore_recurring",
            Self::ExecuteRecurring { .. } => "execute_recurring",
            Self::SkipRecurring { .. } => "skip_recurring",
        }
    }

    /// When the user says it happened, for transaction-like commands.
    #[must_use]
    pub fn occurred_at(&self) -> Option<DateTime<FixedOffset>> {
        match self {
            Self::CreateWallet { occurred_at, .. }
            | Self::CreateFlow { occurred_at, .. }
            | Self::TransferWallet { occurred_at, .. }
            | Self::TransferFlow { occurred_at, .. }
            | Self::ExecuteRecurring { occurred_at, .. } => Some(*occurred_at),
            Self::Income(e) | Self::Expense(e) | Self::Refund(e) => Some(e.occurred_at),
            Self::UpdateTransaction { patch, .. } => patch.occurred_at,
            Self::CreateVault { .. }
            | Self::RenameWallet { .. }
            | Self::ArchiveWallet { .. }
            | Self::RestoreWallet { .. }
            | Self::UpdateFlow { .. }
            | Self::ArchiveFlow { .. }
            | Self::RestoreFlow { .. }
            | Self::CreateCategory { .. }
            | Self::RenameCategory { .. }
            | Self::ArchiveCategory { .. }
            | Self::RestoreCategory { .. }
            | Self::AddAlias { .. }
            | Self::RemoveAlias { .. }
            | Self::MergeCategory { .. }
            | Self::VoidTransaction { .. }
            | Self::CreateRecurring { .. }
            | Self::UpdateRecurring { .. }
            | Self::ArchiveRecurring { .. }
            | Self::RestoreRecurring { .. }
            | Self::SkipRecurring { .. } => None,
        }
    }
}

/// A command addressed to a vault by an author.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, uniffi::Record)]
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
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
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
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct CommandRecord {
    pub envelope: CommandEnvelope,
    pub seq: i64,
    pub created_at: i64,
    pub result_id: Option<Uuid>,
}
