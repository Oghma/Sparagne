//! UniFFI surface. The domain types carry the derives themselves, so there are
//! no mirror DTOs; this module adds the scalars that travel as strings (a
//! malformed one is a [`DomainError`], not a panic: [`lift_failed`]), the
//! [`CoreHandle`] that keeps the `&mut self` core behind a `Mutex` because
//! UniFFI objects only hand out `&self`, and the free functions, which mint
//! every id so that Swift never builds a [`CommandEnvelope`] by hand.

use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, FixedOffset, NaiveDate, SecondsFormat, Utc};
use uuid::Uuid;

use crate::{
    AliasView, AllocationBase, AllocationLine, AllocationPlanView, AllocationPreview,
    AllocationRunView, BucketCategoryTotals, BucketPersonTotals, BucketTargetTotals,
    CategorySuggestion, CategoryTotals, CategoryView, Command, CommandEnvelope, Core, Currency,
    DomainError, FlowBalanceRange, FlowPersonTotals, FlowTransferTotals, MergePreview, Money, Page,
    PendingAllocation, PendingRecurring, PeriodTotals, Receipt, RecentUsage, RecurringRunView,
    RecurringView, RejectedCommand, Schedule, StatementColumnValue, StatementDetection,
    StatementFormatGuess, StatementMapping, StatementOptions, StatementPreset, StatementPreview,
    StatementReport, StatementRowOverride, SyncReport, SyncState, TargetKind, TopExpense,
    TransactionFilter, VaultSnapshot, VaultView,
    quick_add::{self, QuickAdd, QuickAddDefaults, QuickAddError, ResolvedQuickAdd},
    statement,
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

    /// Applies several commands in one local transaction, all or none. Each
    /// keeps its own log row and syncs on its own.
    pub fn execute_batch(
        &self,
        envelopes: Vec<CommandEnvelope>,
    ) -> Result<Vec<Receipt>, DomainError> {
        self.lock()?.execute_batch(envelopes)
    }

    /// Every vault in the database, ordered by name.
    pub fn vaults(&self) -> Result<Vec<VaultView>, DomainError> {
        self.lock()?.vaults()
    }

    /// One vault by id, `nil` when the database does not hold it.
    pub fn vault(&self, vault_id: Uuid) -> Result<Option<VaultView>, DomainError> {
        self.lock()?.vault(vault_id)
    }

    /// Vaults a `DeleteVault` removed whose log is still here: their outbox
    /// has to be pushed like any other, the deletion first of all.
    pub fn deleted_vaults(&self) -> Result<Vec<Uuid>, DomainError> {
        self.lock()?.deleted_vaults()
    }

    /// Wallets and flows of a vault with their balances.
    pub fn snapshot(&self, vault_id: Uuid) -> Result<VaultSnapshot, DomainError> {
        self.lock()?.snapshot(vault_id)
    }

    /// Each flow's balance at `from` and the lowest and highest it reaches
    /// afterwards: what a transfer dated `from` can move without a flow
    /// crossing zero or its cap later on.
    pub fn flow_balance_ranges(
        &self,
        vault_id: Uuid,
        from: UtcDateTime,
    ) -> Result<Vec<FlowBalanceRange>, DomainError> {
        self.lock()?.flow_balance_ranges(vault_id, from)
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

    /// Ids of the entities used most recently, for picker ordering.
    pub fn recent_usage(
        &self,
        vault_id: Uuid,
        since: UtcDateTime,
        limit: u32,
    ) -> Result<RecentUsage, DomainError> {
        self.lock()?.recent_usage(vault_id, since, to_usize(limit))
    }

    /// The category each note was filed under before, one entry per note
    /// (`nil` when the history says nothing), looking back to `since`.
    pub fn suggest_categories(
        &self,
        vault_id: Uuid,
        notes: Vec<String>,
        since: UtcDateTime,
    ) -> Result<Vec<Option<CategorySuggestion>>, DomainError> {
        self.lock()?.suggest_categories(vault_id, &notes, since)
    }

    // -- analytics -------------------------------------------------------

    /// Distinct persons of live transactions: the PERSONA segmented control.
    pub fn people(&self, vault_id: Uuid) -> Result<Vec<String>, DomainError> {
        self.lock()?.people(vault_id)
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

    /// Envelope x person moves between envelopes over `[from, to)`.
    pub fn flow_transfer_totals(
        &self,
        vault_id: Uuid,
        from: UtcDateTime,
        to: UtcDateTime,
    ) -> Result<Vec<FlowTransferTotals>, DomainError> {
        self.lock()?.flow_transfer_totals(vault_id, from, to)
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

    /// Bucket x category net expense over the ranges between consecutive
    /// `bounds`: the RIEPILOGO's months x categories grid.
    #[uniffi::method(default(person = None))]
    pub fn category_bucket_totals(
        &self,
        vault_id: Uuid,
        bounds: Vec<UtcDateTime>,
        person: Option<String>,
    ) -> Result<Vec<BucketCategoryTotals>, DomainError> {
        self.lock()?
            .category_bucket_totals(vault_id, bounds, person)
    }

    /// Bucket x wallet, or x envelope, signed movement over the ranges
    /// between consecutive `bounds`, transfers included: the balance of each
    /// at the end of every month, once the buckets are summed up.
    pub fn balance_breakdown(
        &self,
        vault_id: Uuid,
        bounds: Vec<UtcDateTime>,
        target: TargetKind,
    ) -> Result<Vec<BucketTargetTotals>, DomainError> {
        self.lock()?.balance_breakdown(vault_id, bounds, target)
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

    // -- Recurring and allocation -----------------------------------------

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

    /// The vault's allocation plan, if it has one.
    pub fn allocation_plan(
        &self,
        vault_id: Uuid,
    ) -> Result<Option<AllocationPlanView>, DomainError> {
        self.lock()?.allocation_plan(vault_id)
    }

    /// The period of the allocation plan waiting for a decision on `today`,
    /// if any. `today` comes from the app, in the system timezone.
    pub fn pending_allocation(
        &self,
        vault_id: Uuid,
        today: NaiveDate,
    ) -> Result<Option<PendingAllocation>, DomainError> {
        self.lock()?.pending_allocation(vault_id, today)
    }

    /// The incomes the next allocation shares out, and their total.
    pub fn allocation_base(&self, vault_id: Uuid) -> Result<AllocationBase, DomainError> {
        self.lock()?.allocation_base(vault_id)
    }

    /// `lines` worked out on `total` against the vault's envelopes as they
    /// are now; the lines may be an unsaved draft.
    pub fn preview_allocation(
        &self,
        vault_id: Uuid,
        lines: Vec<AllocationLine>,
        total: i64,
    ) -> Result<AllocationPreview, DomainError> {
        self.lock()?.preview_allocation(vault_id, &lines, total)
    }

    /// The allocation plan's decided periods, the most recent first.
    pub fn allocation_runs(
        &self,
        vault_id: Uuid,
        limit: u32,
    ) -> Result<Vec<AllocationRunView>, DomainError> {
        self.lock()?.allocation_runs(vault_id, limit)
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
            .serve_pull_json(vault_id, since, to_usize(limit))
    }

    /// Rewrites the author of the outbox after a login, and every person or
    /// owner it names by an old author name, then rebuilds the projection: so
    /// `created_by`, the persons and the vault owner follow the account.
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

    /// Refuses the whole outbox locally, as if the server had: used when a
    /// push comes back `403`, so the vault keeps pulling.
    pub fn reject_outbox(
        &self,
        vault_id: Uuid,
        code: String,
        message: String,
    ) -> Result<SyncReport, DomainError> {
        self.lock()?.reject_outbox(vault_id, &code, &message)
    }

    // -- Maintenance ----------------------------------------------------------

    /// Writes a consistent copy of the database to `path`, which must not
    /// exist yet.
    pub fn backup_to(&self, path: String) -> Result<(), DomainError> {
        self.lock()?.backup_to(std::path::Path::new(&path))
    }

    /// Drops a vault from this device without telling the server. Returns
    /// how many outbox commands were thrown away.
    pub fn forget_vault(&self, vault_id: Uuid) -> Result<u32, DomainError> {
        self.lock()?.forget_vault(vault_id)
    }

    // -- Statement import -------------------------------------------------

    /// What importing a statement would do, row by row, with the user's
    /// overrides. Writes nothing.
    #[uniffi::method(default(overrides = []))]
    pub fn preview_statement(
        &self,
        vault_id: Uuid,
        text: String,
        mapping: StatementMapping,
        options: StatementOptions,
        overrides: Vec<StatementRowOverride>,
    ) -> Result<StatementPreview, DomainError> {
        self.lock()?
            .preview_statement(vault_id, &text, &mapping, &options, &overrides)
    }

    /// Imports a statement, one command per row, with the user's overrides.
    pub fn import_statement(
        &self,
        vault_id: Uuid,
        author: String,
        text: String,
        mapping: StatementMapping,
        options: StatementOptions,
        overrides: Vec<StatementRowOverride>,
    ) -> Result<StatementReport, DomainError> {
        self.lock()?
            .import_statement(vault_id, &author, &text, &mapping, &options, &overrides)
    }

    // -- Quick add ----------------------------------------------------------

    /// Resolves the wallet, flow and person names of a parsed quick-add line
    /// against the vault and `people` (who the app lets a row be for), and
    /// returns the command to execute plus the ids the names resolved to.
    pub fn resolve_quick_add(
        &self,
        vault_id: Uuid,
        parsed: QuickAdd,
        now: OffsetDateTime,
        defaults: QuickAddDefaults,
        people: Vec<String>,
    ) -> Result<ResolvedQuickAdd, QuickAddError> {
        self.lock()?
            .resolve_quick_add(vault_id, &parsed, now, &defaults, &people)
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

/// The built-in statement presets.
#[must_use]
#[uniffi::export]
pub fn statement_presets() -> Vec<StatementPreset> {
    statement::presets()
}

/// Delimiter, header, matching preset and a sample of a statement file.
#[uniffi::export]
pub fn detect_statement(text: String) -> Result<StatementDetection, DomainError> {
    statement::detect(&text)
}

/// The distinct values of a statement column with how many rows hold each,
/// most rows first: the accounts or the types of a file, for the mapping.
#[must_use]
#[uniffi::export]
pub fn statement_column_values(
    text: String,
    delimiter: String,
    column: String,
) -> Vec<StatementColumnValue> {
    statement::column_values(&text, &delimiter, &column)
}

/// The date format and decimal mark the values of a statement's date and
/// amount columns are written in, for a mapping that starts blank.
#[must_use]
#[uniffi::export]
pub fn guess_statement_formats(
    text: String,
    delimiter: String,
    date_column: Option<String>,
    amount_column: Option<String>,
) -> StatementFormatGuess {
    statement::guess_formats(
        &text,
        &delimiter,
        date_column.as_deref(),
        amount_column.as_deref(),
    )
}

/// A statement mapping as JSON, for the app to remember.
#[must_use]
#[uniffi::export]
pub fn encode_statement_mapping(mapping: StatementMapping) -> String {
    statement::encode_mapping(&mapping)
}

/// A statement mapping back from its JSON.
#[uniffi::export]
pub fn decode_statement_mapping(json: String) -> Result<StatementMapping, DomainError> {
    statement::decode_mapping(&json)
}

/// The next `limit` dates of a schedule on or after `from`, for previewing a
/// saved or unsaved template (agenda, editor inspector). No database access.
///
/// `limit` is capped at [`MAX_OCCURRENCES_PREVIEW`]: a schedule without an
/// `end_date` never runs out, so an unbounded `limit` (`u32::MAX`) would
/// allocate until memory does. A caller that asks for more gets that many.
#[uniffi::export]
pub fn schedule_occurrences(
    schedule: Schedule,
    from: NaiveDate,
    limit: u32,
) -> Result<Vec<NaiveDate>, DomainError> {
    schedule.occurrences_from(from, to_usize(limit.min(MAX_OCCURRENCES_PREVIEW)))
}

/// The most dates `schedule_occurrences` returns: far more than any preview
/// shows, far less than a memory problem.
const MAX_OCCURRENCES_PREVIEW: u32 = 1_000;

/// Saturating `u32` -> `usize`; the core takes `usize` limits.
fn to_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;
    use crate::recurring::Frequency;

    fn open_ended_daily() -> Schedule {
        Schedule {
            frequency: Frequency::Daily,
            interval: 1,
            start_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            end_date: None,
        }
    }

    #[test]
    fn occurrences_preview_is_capped() {
        let from = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let all = schedule_occurrences(open_ended_daily(), from, u32::MAX).unwrap();
        assert_eq!(all.len(), to_usize(MAX_OCCURRENCES_PREVIEW));
        let few = schedule_occurrences(open_ended_daily(), from, 3).unwrap();
        assert_eq!(few.len(), 3);
    }
}
