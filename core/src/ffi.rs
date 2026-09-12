//! UniFFI surface: custom scalar mappings, the handle object the app holds and
//! the free functions that mint envelopes and parse text.
//!
//! The domain types are exported as they are, so there are no mirror DTOs: the
//! derives live on the real `Command`, `Entry`, `TransactionView` and friends.
//! Only three things are added here.
//!
//! 1. **Scalars.** `Uuid`, the two `DateTime` flavours and `NaiveDate` are not
//!    UniFFI builtins, so they travel as strings. A malformed string surfaces
//!    as a [`DomainError`] rather than a panic (see [`lift_failed`]).
//! 2. **[`CoreHandle`].** [`Core::execute`] takes `&mut self`; UniFFI objects
//!    are shared and only ever hand out `&self`, so the mutability lives behind
//!    a `Mutex` here instead of changing the core.
//! 3. **Free functions.** Ids are minted in Rust only: Swift never builds a
//!    [`CommandEnvelope`] by hand.

use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, FixedOffset, NaiveDate, SecondsFormat, Utc};
use uuid::Uuid;

use crate::{
    AliasView, BucketPersonTotals, CategoryTotals, CategoryView, Command, CommandEnvelope,
    CommandRecord, Core, Currency, DomainError, FlowPersonTotals, MergePreview, Money, Page,
    PendingRecurring, PeriodTotals, Receipt, RecentUsage, RecurringRunView, RecurringView,
    RejectedCommand, SyncReport, SyncState, TopExpense, TransactionFilter, TransactionView,
    VaultSnapshot, VaultView,
    quick_add::{self, QuickAdd, QuickAddDefaults, QuickAddError, ResolvedQuickAdd},
};

// ---------------------------------------------------------------------------
// Custom scalar types
// ---------------------------------------------------------------------------

/// `chrono::DateTime<FixedOffset>` as it crosses the FFI. Aliases are required:
/// `uniffi::custom_type!` only accepts a single identifier.
type OffsetDateTime = DateTime<FixedOffset>;
/// `chrono::DateTime<Utc>` as it crosses the FFI.
type UtcDateTime = DateTime<Utc>;

/// Turns a malformed scalar into a domain error.
///
/// UniFFI downcasts the `anyhow::Error` a failed lift produces to the declared
/// error type of the call; when that type is [`DomainError`] the app sees
/// `invalid_command` instead of an internal error.
fn lift_failed(kind: &str, value: &str) -> DomainError {
    DomainError::InvalidCommand(format!("invalid {kind}: '{value}'"))
}

uniffi::custom_type!(Uuid, String, {
    remote,
    lower: |value| value.hyphenated().to_string(),
    try_lift: |value| Ok(Uuid::parse_str(&value).map_err(|_| lift_failed("uuid", &value))?),
});

uniffi::custom_type!(OffsetDateTime, String, {
    remote,
    lower: |value| value.to_rfc3339_opts(SecondsFormat::Secs, false),
    try_lift: |value| Ok(DateTime::parse_from_rfc3339(&value)
        .map_err(|_| lift_failed("date-time", &value))?),
});

uniffi::custom_type!(UtcDateTime, String, {
    remote,
    lower: |value| value.to_rfc3339_opts(SecondsFormat::Secs, true),
    try_lift: |value| Ok(DateTime::parse_from_rfc3339(&value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|_| lift_failed("date-time", &value))?),
});

uniffi::custom_type!(NaiveDate, String, {
    remote,
    lower: |value| value.format(DATE_FORMAT).to_string(),
    try_lift: |value| Ok(NaiveDate::parse_from_str(&value, DATE_FORMAT)
        .map_err(|_| lift_failed("date", &value))?),
});

/// Wire format of a bare calendar date.
const DATE_FORMAT: &str = "%Y-%m-%d";

// ---------------------------------------------------------------------------
// Handle
// ---------------------------------------------------------------------------

/// The app's handle on one database file.
///
/// Every method serializes on an internal lock, so the object is safe to share
/// between Swift tasks. Calls are synchronous and short; long scans belong in a
/// background queue on the Swift side.
#[derive(Debug, uniffi::Object)]
pub struct CoreHandle {
    inner: Mutex<Core>,
}

impl CoreHandle {
    /// A poisoned lock means an earlier call panicked; report it as storage
    /// trouble rather than panicking again.
    fn lock(&self) -> Result<MutexGuard<'_, Core>, DomainError> {
        self.inner
            .lock()
            .map_err(|_| DomainError::Storage("core lock poisoned".to_string()))
    }
}

#[uniffi::export]
impl CoreHandle {
    /// Opens (or creates and migrates) a database file.
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<Arc<Self>, DomainError> {
        Ok(Arc::new(Self {
            inner: Mutex::new(Core::open(path)?),
        }))
    }

    /// Fresh in-memory database. Nothing is persisted.
    #[uniffi::constructor]
    pub fn open_in_memory() -> Result<Arc<Self>, DomainError> {
        Ok(Arc::new(Self {
            inner: Mutex::new(Core::open_in_memory()?),
        }))
    }

    /// Applies one command: the only way to change a vault.
    pub fn execute(&self, envelope: CommandEnvelope) -> Result<Receipt, DomainError> {
        self.lock()?.execute(envelope)
    }

    /// Every vault in the database, ordered by name.
    pub fn vaults(&self) -> Result<Vec<VaultView>, DomainError> {
        self.lock()?.vaults()
    }

    /// One vault by id, `nil` when the database does not hold it.
    pub fn vault(&self, vault_id: Uuid) -> Result<Option<VaultView>, DomainError> {
        self.lock()?.vault(vault_id)
    }

    /// Wallets and flows of a vault with their balances.
    pub fn snapshot(&self, vault_id: Uuid) -> Result<VaultSnapshot, DomainError> {
        self.lock()?.snapshot(vault_id)
    }

    pub fn categories(
        &self,
        vault_id: Uuid,
        include_archived: bool,
    ) -> Result<Vec<CategoryView>, DomainError> {
        self.lock()?.categories(vault_id, include_archived)
    }

    pub fn aliases(&self, vault_id: Uuid) -> Result<Vec<AliasView>, DomainError> {
        self.lock()?.aliases(vault_id)
    }

    /// What `MergeCategory` would refuse, without changing anything.
    pub fn preview_merge(
        &self,
        vault_id: Uuid,
        source_id: Uuid,
        target_id: Uuid,
    ) -> Result<MergePreview, DomainError> {
        self.lock()?.preview_merge(vault_id, source_id, target_id)
    }

    /// Active categories whose name is close to `name`, nearest first.
    pub fn similar_categories(
        &self,
        vault_id: Uuid,
        name: String,
    ) -> Result<Vec<CategoryView>, DomainError> {
        self.lock()?.similar_categories(vault_id, &name)
    }

    /// One page of transactions, newest first. Pass `next_cursor` back for the
    /// next (older) page.
    pub fn list_transactions(
        &self,
        vault_id: Uuid,
        filter: TransactionFilter,
        limit: u32,
        cursor: Option<String>,
    ) -> Result<Page, DomainError> {
        self.lock()?
            .list_transactions(vault_id, &filter, to_usize(limit), cursor.as_deref())
    }

    /// One transaction with its legs, voided included.
    pub fn transaction(
        &self,
        vault_id: Uuid,
        transaction_id: Uuid,
    ) -> Result<TransactionView, DomainError> {
        self.lock()?.transaction(vault_id, transaction_id)
    }

    /// Ids of the entities used most recently, for picker ordering.
    pub fn recent_usage(
        &self,
        vault_id: Uuid,
        since: UtcDateTime,
        limit: u32,
    ) -> Result<RecentUsage, DomainError> {
        self.lock()?.recent_usage(vault_id, since, to_usize(limit))
    }

    /// Sums by kind over `[from, to)`. Either bound may be `nil`; both `nil`
    /// is all time.
    #[uniffi::method(default(from = None, to = None))]
    pub fn period_totals(
        &self,
        vault_id: Uuid,
        from: Option<UtcDateTime>,
        to: Option<UtcDateTime>,
    ) -> Result<PeriodTotals, DomainError> {
        self.lock()?.period_totals(vault_id, from, to)
    }

    // -- analytics (docs/v2/UI.md §4) ------------------------------------

    /// Distinct authors of live transactions: the PERSONA segmented control.
    pub fn authors(&self, vault_id: Uuid) -> Result<Vec<String>, DomainError> {
        self.lock()?.authors(vault_id)
    }

    /// Envelope x person matrix over `[from, to)`.
    pub fn flow_person_totals(
        &self,
        vault_id: Uuid,
        from: UtcDateTime,
        to: UtcDateTime,
    ) -> Result<Vec<FlowPersonTotals>, DomainError> {
        self.lock()?.flow_person_totals(vault_id, from, to)
    }

    /// Category breakdown over `[from, to)`, heaviest net expense first.
    #[uniffi::method(default(person = None))]
    pub fn category_totals(
        &self,
        vault_id: Uuid,
        from: UtcDateTime,
        to: UtcDateTime,
        person: Option<String>,
    ) -> Result<Vec<CategoryTotals>, DomainError> {
        self.lock()?.category_totals(vault_id, from, to, person)
    }

    /// Totals of the ranges between consecutive `bounds`: 13 month starts give
    /// the twelve bars of a year.
    #[uniffi::method(default(person = None))]
    pub fn bucket_totals(
        &self,
        vault_id: Uuid,
        bounds: Vec<UtcDateTime>,
        person: Option<String>,
    ) -> Result<Vec<PeriodTotals>, DomainError> {
        self.lock()?.bucket_totals(vault_id, bounds, person)
    }

    /// Bucket x person breakdown behind the RIEPILOGO: the app passes the
    /// epoch plus thirteen month starts and reads thirteen buckets.
    pub fn year_breakdown(
        &self,
        vault_id: Uuid,
        bounds: Vec<UtcDateTime>,
    ) -> Result<Vec<BucketPersonTotals>, DomainError> {
        self.lock()?.year_breakdown(vault_id, bounds)
    }

    /// The heaviest expenses of `[from, to)`, largest first.
    #[uniffi::method(default(person = None))]
    pub fn top_expenses(
        &self,
        vault_id: Uuid,
        from: UtcDateTime,
        to: UtcDateTime,
        person: Option<String>,
        limit: u32,
    ) -> Result<Vec<TopExpense>, DomainError> {
        self.lock()?.top_expenses(vault_id, from, to, person, limit)
    }

    pub fn list_recurring(
        &self,
        vault_id: Uuid,
        include_archived: bool,
    ) -> Result<Vec<RecurringView>, DomainError> {
        self.lock()?.list_recurring(vault_id, include_archived)
    }

    /// Templates with periods still waiting for a decision. `today` comes from
    /// the app, in the system timezone: the core never guesses one.
    pub fn pending_recurring(
        &self,
        vault_id: Uuid,
        today: NaiveDate,
    ) -> Result<Vec<PendingRecurring>, DomainError> {
        self.lock()?.pending_recurring(vault_id, today)
    }

    pub fn recurring_runs(
        &self,
        vault_id: Uuid,
        recurring_id: Uuid,
    ) -> Result<Vec<RecurringRunView>, DomainError> {
        self.lock()?.recurring_runs(vault_id, recurring_id)
    }

    /// Log entries of a vault with `seq > since_seq`, in order.
    pub fn commands_since(
        &self,
        vault_id: Uuid,
        since_seq: i64,
    ) -> Result<Vec<CommandRecord>, DomainError> {
        self.lock()?.commands_since(vault_id, since_seq)
    }

    // -- Sync ---------------------------------------------------------------

    /// Where the vault stands with the server: last known server seq, outbox
    /// size, pending rejections.
    pub fn sync_state(&self, vault_id: Uuid) -> Result<SyncState, DomainError> {
        self.lock()?.sync_state(vault_id)
    }

    /// Highest applied seq in the vault's local log; 0 for an unknown vault.
    pub fn last_seq(&self, vault_id: Uuid) -> Result<i64, DomainError> {
        self.lock()?.last_seq(vault_id)
    }

    /// JSON body for `POST /vaults/{id}/push`: at most `limit` commands of
    /// the outbox, oldest first. Swift only does the HTTP, and pushes again
    /// while `sync_state` still reports an outbox.
    pub fn push_request_json(&self, vault_id: Uuid, limit: u32) -> Result<String, DomainError> {
        self.lock()?.push_request_json(vault_id, to_usize(limit))
    }

    /// Folds the server's push response back into the log.
    pub fn apply_push_response_json(
        &self,
        vault_id: Uuid,
        json: String,
    ) -> Result<SyncReport, DomainError> {
        self.lock()?.apply_push_response_json(vault_id, &json)
    }

    /// Folds a pull response into the log, rebasing when it has to.
    pub fn integrate_pull_json(
        &self,
        vault_id: Uuid,
        json: String,
    ) -> Result<SyncReport, DomainError> {
        self.lock()?.integrate_pull_json(vault_id, &json)
    }

    /// Server side of a push, for a core acting as the server (tests, local
    /// fake server).
    pub fn serve_push_json(&self, vault_id: Uuid, json: String) -> Result<String, DomainError> {
        self.lock()?.serve_push_json(vault_id, &json)
    }

    /// Server side of a pull.
    pub fn serve_pull_json(
        &self,
        vault_id: Uuid,
        since: i64,
        limit: u32,
    ) -> Result<String, DomainError> {
        self.lock()?
            .serve_pull_json(vault_id, since, limit as usize)
    }

    /// Rewrites the author of the outbox after a login and rebuilds the
    /// projection, so `created_by` and the vault owner follow the account.
    pub fn relabel_outbox(&self, vault_id: Uuid, author: String) -> Result<(), DomainError> {
        self.lock()?.relabel_outbox(vault_id, &author)
    }

    /// Commands the server (or a rebase) refused, oldest first.
    pub fn rejected_commands(&self, vault_id: Uuid) -> Result<Vec<RejectedCommand>, DomainError> {
        self.lock()?.rejected_commands(vault_id)
    }

    /// Forgets one rejected command.
    pub fn dismiss_rejected(&self, vault_id: Uuid, command_id: Uuid) -> Result<(), DomainError> {
        self.lock()?.dismiss_rejected(vault_id, command_id)
    }

    /// Resolves the wallet and flow names of a parsed quick-add line against
    /// the vault and returns the command to execute plus the ids the names
    /// resolved to.
    pub fn resolve_quick_add(
        &self,
        vault_id: Uuid,
        parsed: QuickAdd,
        now: OffsetDateTime,
        defaults: QuickAddDefaults,
    ) -> Result<ResolvedQuickAdd, QuickAddError> {
        self.lock()?
            .resolve_quick_add(vault_id, &parsed, now, &defaults)
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// A command addressed to a vault, with a fresh UUID v7.
///
/// Ids are minted here and nowhere else: the id is the idempotency key and the
/// id of whatever the command creates, so Swift must never invent one.
#[must_use]
#[uniffi::export]
pub fn new_envelope(vault_id: Uuid, author: String, command: Command) -> CommandEnvelope {
    CommandEnvelope::new(vault_id, author, command)
}

/// `CreateVault` envelope: the new vault's id equals the command id.
#[must_use]
#[uniffi::export]
pub fn create_vault_envelope(author: String, name: String, currency: Currency) -> CommandEnvelope {
    CommandEnvelope::create_vault(author, name, currency)
}

/// Parses one quick-add line. Pure: no database access, no name resolution.
#[uniffi::export]
pub fn parse_quick_add(input: String, currency: Currency) -> Result<QuickAdd, QuickAddError> {
    quick_add::parse(&input, currency)
}

/// `<sign><major>.<minor> <CODE>`, e.g. `-12.50 EUR`. Debug and test output;
/// the app formats for display with the system formatter.
#[must_use]
#[uniffi::export]
pub fn format_money(minor: i64, currency: Currency) -> String {
    Money::new(minor).format(currency)
}

/// Parses a major-unit amount into minor units. No grouping separators.
#[uniffi::export]
pub fn parse_money(text: String, currency: Currency) -> Result<i64, DomainError> {
    Money::parse_major(&text, currency).map(Money::minor)
}

/// Resolves a relative date token from a quick-add line against `today`.
#[uniffi::export]
pub fn resolve_date_spec(
    spec: quick_add::DateSpec,
    today: NaiveDate,
) -> Result<NaiveDate, QuickAddError> {
    spec.resolve(today)
}

/// Saturating `u32` -> `usize`; the core takes `usize` limits.
fn to_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}
