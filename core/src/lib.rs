//! Sparagne v2 core: domain rules, a per-vault command log and a SQLite
//! projection of it. Every write goes through [`Core::execute`] as a
//! [`Command`]; state tables are derived and can be rebuilt with [`replay`].
//! Amounts are `i64` minor units, one currency per vault; the command id is
//! the idempotency key and every entity a command creates gets an id derived
//! from it (equal to it, or a UUID v5 of it), so replaying the log is
//! deterministic; domain checks (flow caps, non-negativity) run on create and
//! update, never on void.

pub mod allocation;
mod analytics;
mod category;
mod command;
mod currency;
mod engine;
mod error;
mod ffi;
mod flow;
mod maintenance;
mod money;
mod query;
pub mod quick_add;
pub mod recurring;
pub mod statement;
mod store;
mod suggest;
pub mod sync;
mod usage;

pub use allocation::{
    AllocationBase, AllocationLine, AllocationMove, AllocationPlanPatch, AllocationPlanView,
    AllocationPreview, AllocationRule, AllocationRunView, LineStatus, PendingAllocation,
    PreviewLine, RunMove,
};
pub use analytics::{BucketCategoryTotals, TargetKind};
pub use command::{
    Command, CommandEnvelope, CommandRecord, Entry, Receipt, RecurringPatch, TransactionKind,
    TransactionPatch,
};
pub use currency::Currency;
pub use engine::entities::{AliasView, MergeConflictKind};
pub use error::DomainError;
pub use flow::{FlowMode, headroom};
pub use query::{
    CategoryView, FlowBalanceRange, FlowView, LegTarget, LegView, TransactionFilter,
    TransactionView, VaultSnapshot, WalletView, replay,
};
pub use recurring::{Frequency, RecurringView, RunOutcome, Schedule};
pub use statement::{
    AmountSign, StatementAction, StatementDateFormat, StatementDescriptionRule,
    StatementFormatGuess, StatementMapping, StatementMatchChoice, StatementOptions,
    StatementPreview, StatementReport, StatementRow, StatementRowOverride, StatementRowStatus,
    StatementTypeRule,
};
pub use store::Core;
pub use suggest::CategorySuggestion;
pub use sync::{SyncReport, SyncState};

// Imported through the crate root by the modules, never from outside.
pub(crate) use analytics::{
    BucketPersonTotals, BucketTargetTotals, CategoryTotals, FlowPersonTotals, FlowTransferTotals,
    TopExpense,
};
pub(crate) use category::normalize_category_key;
pub(crate) use engine::entities::MergePreview;
pub(crate) use flow::Flow;
pub(crate) use money::Money;
pub(crate) use query::Page;
pub(crate) use recurring::{PendingRecurring, RecurringRunView};
pub(crate) use statement::{StatementColumnValue, StatementDetection, StatementPreset};
pub(crate) use sync::RejectedCommand;
pub(crate) use usage::{PeriodTotals, RecentUsage, VaultView};

/// Result alias used across the crate.
pub type Result<T> = std::result::Result<T, DomainError>;

uniffi::setup_scaffolding!();
