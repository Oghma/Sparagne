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
    AllocationRunView, BalanceRange, BucketCategoryTotals, BucketPersonTotals, BucketTargetTotals,
    CategorySuggestion, CategoryTotals, CategoryView, Command, CommandEnvelope, Core, Currency,
    DomainError, FlowCategoryTotals, FlowPersonTotals, FlowTransferTotals, MergePreview, Page,
    PendingAllocation, PendingRecurring, PeriodTotals, Receipt, RecentUsage, RecurringRunView,
    RecurringView, RejectedCommand, Schedule, StatementColumnValue, StatementDetection,
    StatementFormatGuess, StatementMapping, StatementOptions, StatementPreset, StatementPreview,
    StatementReport, StatementRowOverride, SyncReport, SyncState, TargetKind, TopExpense,
    TransactionFilter, VaultSnapshot, VaultView,
    loan::{
        LinkedInstallment, LoanEventView, LoanOverview, LoanSchedule, LoanTerms, LoanView, math,
    },
    parse_major,
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

    /// Applies one command: the only way to change a vault. Its note is
    /// capitalized first ([`Command::capitalize_note`]); the server and a
    /// replay go through [`Core::execute`] and take the note as logged.
    pub fn execute(&self, mut envelope: CommandEnvelope) -> Result<Receipt, DomainError> {
        envelope.command.capitalize_note();
        self.lock()?.execute(envelope)
    }

    /// Applies several commands in one local transaction, all or none. Each
    /// keeps its own log row and syncs on its own. Notes are capitalized as
    /// in [`Self::execute`].
    pub fn execute_batch(
        &self,
        mut envelopes: Vec<CommandEnvelope>,
    ) -> Result<Vec<Receipt>, DomainError> {
        for envelope in &mut envelopes {
            envelope.command.capitalize_note();
        }
        self.lock()?.execute_batch(envelopes)
    }

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

/// The methods that only lock the core and call its method of the same name,
/// as one more `#[uniffi::export] impl CoreHandle` block. Each is written
/// `fn name(args) -> T;` and returns `Result<T, DomainError>`; the arguments
/// are passed on as they are, unless `=> (...)` spells out the call's, for a
/// `String` the core borrows as `&str` or a `u32` limit it takes as `usize`.
/// Attributes pass through, among them the doc comments UniFFI copies into
/// the Swift bindings.
macro_rules! forward {
    (@call $core:expr, $name:ident, $args:tt) => {
        $core.$name $args
    };
    (@call $core:expr, $name:ident, $args:tt, $call:tt) => {
        $core.$name $call
    };
    ($(
        $(#[$meta:meta])*
        fn $name:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty
            $(=> ($($call:expr),* $(,)?))?;
    )*) => {
        #[uniffi::export]
        impl CoreHandle {
            $(
                $(#[$meta])*
                pub fn $name(&self, $($arg: $ty),*) -> Result<$ret, DomainError> {
                    forward!(@call self.lock()?, $name, ($($arg),*) $(, ($($call),*))?)
                }
            )*
        }
    };
}

forward! {
    /// Every vault in the database, ordered by name.
    fn vaults() -> Vec<VaultView>;

    /// One vault by id, `nil` when the database does not hold it.
    fn vault(vault_id: Uuid) -> Option<VaultView>;

    /// Vaults a `DeleteVault` removed whose log is still here: their outbox
    /// has to be pushed like any other, the deletion first of all.
    fn deleted_vaults() -> Vec<Uuid>;

    /// Wallets and flows of a vault with their balances.
    fn snapshot(vault_id: Uuid) -> VaultSnapshot;

    /// Each flow's balance at `from`, or each wallet's by `target`, and the
    /// lowest and highest it reaches afterwards: what a transfer dated `from`
    /// can move without a flow crossing zero or its cap later on.
    fn balance_ranges(vault_id: Uuid, from: UtcDateTime, target: TargetKind) -> Vec<BalanceRange>;

    fn categories(vault_id: Uuid, include_archived: bool) -> Vec<CategoryView>;

    fn aliases(vault_id: Uuid) -> Vec<AliasView>;

    /// What `MergeCategory` would refuse, without changing anything.
    fn preview_merge(vault_id: Uuid, source_id: Uuid, target_id: Uuid) -> MergePreview;

    /// Active categories whose name is close to `name`, nearest first.
    fn similar_categories(vault_id: Uuid, name: String) -> Vec<CategoryView>
        => (vault_id, &name);

    /// One page of transactions, newest first. Pass `next_cursor` back for the
    /// next (older) page.
    fn list_transactions(
        vault_id: Uuid,
        filter: TransactionFilter,
        limit: u32,
        cursor: Option<String>,
    ) -> Page => (vault_id, &filter, to_usize(limit), cursor.as_deref());

    /// Ids of the entities used most recently, for picker ordering.
    fn recent_usage(vault_id: Uuid, since: UtcDateTime, limit: u32) -> RecentUsage
        => (vault_id, since, to_usize(limit));

    /// The category each note was filed under before, one entry per note
    /// (`nil` when the history says nothing), looking back to `since`.
    fn suggest_categories(
        vault_id: Uuid,
        notes: Vec<String>,
        since: UtcDateTime,
    ) -> Vec<Option<CategorySuggestion>> => (vault_id, &notes, since);

    // -- analytics -------------------------------------------------------

    /// Distinct persons of live transactions: the PERSONA segmented control.
    fn people(vault_id: Uuid) -> Vec<String>;

    /// Envelope x person matrix over `[from, to)`.
    fn flow_person_totals(vault_id: Uuid, from: UtcDateTime, to: UtcDateTime)
        -> Vec<FlowPersonTotals>;

    /// Envelope x person moves between envelopes over `[from, to)`.
    fn flow_transfer_totals(vault_id: Uuid, from: UtcDateTime, to: UtcDateTime)
        -> Vec<FlowTransferTotals>;

    /// Category breakdown over `[from, to)`, heaviest net expense first.
    #[uniffi::method(default(person = None))]
    fn category_totals(
        vault_id: Uuid,
        from: UtcDateTime,
        to: UtcDateTime,
        person: Option<String>,
    ) -> Vec<CategoryTotals>;

    /// Envelope x category net expense over `[from, to)`: the ribbons from the
    /// flows to the categories of the cash-flow Sankey.
    #[uniffi::method(default(person = None))]
    fn flow_category_totals(
        vault_id: Uuid,
        from: UtcDateTime,
        to: UtcDateTime,
        person: Option<String>,
    ) -> Vec<FlowCategoryTotals>;

    /// Totals of the ranges between consecutive `bounds`: 13 month starts give
    /// the twelve bars of a year.
    #[uniffi::method(default(person = None))]
    fn bucket_totals(vault_id: Uuid, bounds: Vec<UtcDateTime>, person: Option<String>)
        -> Vec<PeriodTotals>;

    /// Bucket x person breakdown behind the RIEPILOGO: the app passes the
    /// epoch plus thirteen month starts and reads thirteen buckets.
    fn year_breakdown(vault_id: Uuid, bounds: Vec<UtcDateTime>) -> Vec<BucketPersonTotals>;

    /// Bucket x category net expense over the ranges between consecutive
    /// `bounds`: the RIEPILOGO's months x categories grid.
    #[uniffi::method(default(person = None))]
    fn category_bucket_totals(vault_id: Uuid, bounds: Vec<UtcDateTime>, person: Option<String>)
        -> Vec<BucketCategoryTotals>;

    /// Bucket x wallet, or x envelope, signed movement over the ranges
    /// between consecutive `bounds`, transfers included: the balance of each
    /// at the end of every month, once the buckets are summed up.
    fn balance_breakdown(vault_id: Uuid, bounds: Vec<UtcDateTime>, target: TargetKind)
        -> Vec<BucketTargetTotals>;

    /// The heaviest expenses of `[from, to)`, largest first.
    #[uniffi::method(default(person = None))]
    fn top_expenses(
        vault_id: Uuid,
        from: UtcDateTime,
        to: UtcDateTime,
        person: Option<String>,
        limit: u32,
    ) -> Vec<TopExpense>;

    // -- Recurring and allocation -----------------------------------------

    fn list_recurring(vault_id: Uuid, include_archived: bool) -> Vec<RecurringView>;

    /// Templates with periods still waiting for a decision. `today` comes from
    /// the app, in the system timezone: the core never guesses one.
    fn pending_recurring(vault_id: Uuid, today: NaiveDate) -> Vec<PendingRecurring>;

    fn recurring_runs(vault_id: Uuid, recurring_id: Uuid) -> Vec<RecurringRunView>;

    /// The vault's allocation plan, if it has one.
    fn allocation_plan(vault_id: Uuid) -> Option<AllocationPlanView>;

    /// The period of the allocation plan waiting for a decision on `today`,
    /// if any. `today` comes from the app, in the system timezone.
    fn pending_allocation(vault_id: Uuid, today: NaiveDate) -> Option<PendingAllocation>;

    /// The incomes the next allocation shares out, and their total.
    fn allocation_base(vault_id: Uuid) -> AllocationBase;

    /// `lines` worked out on `total` against the vault's envelopes as they
    /// are now; the lines may be an unsaved draft.
    fn preview_allocation(vault_id: Uuid, lines: Vec<AllocationLine>, total: i64)
        -> AllocationPreview => (vault_id, &lines, total);

    /// The allocation plan's decided periods, the most recent first.
    fn allocation_runs(vault_id: Uuid, limit: u32) -> Vec<AllocationRunView>;

    // -- Loans -------------------------------------------------------------

    /// Every loan of the vault, archived included, by name.
    fn loans(vault_id: Uuid) -> Vec<LoanView>;

    /// One loan's schedule, its events applied.
    fn loan_schedule(vault_id: Uuid, loan_id: Uuid) -> LoanSchedule;

    /// Every active loan on `today`: residual, progress, next installment.
    fn loan_overview(vault_id: Uuid, today: NaiveDate) -> Vec<LoanOverview>;

    /// For every active loan with a linked template, the amount each due
    /// date's installment should be recorded with.
    fn linked_installments(vault_id: Uuid) -> Vec<LinkedInstallment>;

    /// The schedule of terms and events not saved yet, for the forms.
    fn preview_loan(terms: LoanTerms, events: Vec<LoanEventView>) -> LoanSchedule
        => (&terms, &events);

    // -- Sync ---------------------------------------------------------------

    /// Where the vault stands with the server: last known server seq, outbox
    /// size, pending rejections.
    fn sync_state(vault_id: Uuid) -> SyncState;

    /// Highest applied seq in the vault's local log; 0 for an unknown vault.
    fn last_seq(vault_id: Uuid) -> i64;

    /// JSON body for `POST /vaults/{id}/push`: at most `limit` commands of
    /// the outbox, oldest first. Swift only does the HTTP, and pushes again
    /// while `sync_state` still reports an outbox.
    fn push_request_json(vault_id: Uuid, limit: u32) -> String => (vault_id, to_usize(limit));

    /// Folds the server's push response back into the log.
    fn apply_push_response_json(vault_id: Uuid, json: String) -> SyncReport => (vault_id, &json);

    /// Folds a pull response into the log, rebasing when it has to.
    fn integrate_pull_json(vault_id: Uuid, json: String) -> SyncReport => (vault_id, &json);

    /// Server side of a push, for a core acting as the server (tests, local
    /// fake server).
    fn serve_push_json(vault_id: Uuid, json: String) -> String => (vault_id, &json);

    /// Server side of a pull.
    fn serve_pull_json(vault_id: Uuid, since: i64, limit: u32) -> String
        => (vault_id, since, to_usize(limit));

    /// Rewrites the author of the outbox after a login, and every person or
    /// owner it names by an old author name, then rebuilds the projection: so
    /// `created_by`, the persons and the vault owner follow the account.
    fn relabel_outbox(vault_id: Uuid, author: String) -> () => (vault_id, &author);

    /// Commands the server (or a rebase) refused, oldest first.
    fn rejected_commands(vault_id: Uuid) -> Vec<RejectedCommand>;

    /// Forgets one rejected command.
    fn dismiss_rejected(vault_id: Uuid, command_id: Uuid) -> ();

    /// Refuses the whole outbox locally, as if the server had: used when a
    /// push comes back `403`, so the vault keeps pulling.
    fn reject_outbox(vault_id: Uuid, code: String, message: String) -> SyncReport
        => (vault_id, &code, &message);

    // -- Maintenance ----------------------------------------------------------

    /// Writes a consistent copy of the database to `path`, which must not
    /// exist yet.
    fn backup_to(path: String) -> () => (std::path::Path::new(&path));

    /// Drops a vault from this device without telling the server. Returns
    /// how many outbox commands were thrown away.
    fn forget_vault(vault_id: Uuid) -> u32;

    // -- Statement import -------------------------------------------------

    /// What importing a statement would do, row by row, with the user's
    /// overrides. Writes nothing.
    #[uniffi::method(default(overrides = []))]
    fn preview_statement(
        vault_id: Uuid,
        text: String,
        mapping: StatementMapping,
        options: StatementOptions,
        overrides: Vec<StatementRowOverride>,
    ) -> StatementPreview => (vault_id, &text, &mapping, &options, &overrides);

    /// Imports a statement, one command per row, with the user's overrides.
    fn import_statement(
        vault_id: Uuid,
        author: String,
        text: String,
        mapping: StatementMapping,
        options: StatementOptions,
        overrides: Vec<StatementRowOverride>,
    ) -> StatementReport => (vault_id, &author, &text, &mapping, &options, &overrides);
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
    parse_major(&text, currency)
}

/// The French installment that repays `principal` minor units in `months`
/// monthly installments at the yearly TAN `annual_rate_ppm` (millionths).
#[uniffi::export]
pub fn loan_installment(
    principal: i64,
    annual_rate_ppm: u32,
    months: u32,
) -> Result<i64, DomainError> {
    math::installment(principal, annual_rate_ppm, months)
}

/// The yearly TAN, in millionths, at which `installment` repays `principal`
/// in `months`: what the loan form shows when the user knows the amount, the
/// installment and the duration but not the rate.
#[uniffi::export]
pub fn solve_loan_rate(principal: i64, installment: i64, months: u32) -> Result<u32, DomainError> {
    math::solve_rate(principal, installment, months)
}

/// The TAN, in millionths, of a loan with no fees whose TAEG is `taeg_ppm`:
/// an estimate when the user only knows the TAEG.
#[uniffi::export]
pub fn tan_from_taeg(taeg_ppm: u32) -> Result<u32, DomainError> {
    math::tan_from_taeg(taeg_ppm)
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
    use crate::{Entry, recurring::Frequency};

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

    fn expense(note: &str) -> Command {
        Command::Expense(Entry {
            amount: 500,
            wallet_id: None,
            flow_id: None,
            category: None,
            note: Some(note.to_string()),
            occurred_at: DateTime::parse_from_rfc3339("2026-10-23T12:00:00+02:00").unwrap(),
            person: None,
        })
    }

    #[test]
    fn the_app_logs_its_notes_capitalized_and_a_replay_keeps_them_as_logged() {
        let handle = CoreHandle::open_in_memory().unwrap();
        let envelope =
            create_vault_envelope("alice".to_string(), "Main".to_string(), Currency::Eur);
        let vault = handle.execute(envelope).unwrap().result_id.unwrap();
        let wallet = Command::CreateWallet {
            name: "Cash".to_string(),
            opening_balance: 10_000,
            occurred_at: DateTime::parse_from_rfc3339("2026-10-01T00:00:00+02:00").unwrap(),
        };
        let envelope = |command| new_envelope(vault, "alice".to_string(), command);
        handle.execute(envelope(wallet)).unwrap();

        let pharmacy = handle.execute(envelope(expense("farmacia"))).unwrap();
        let batch = handle
            .execute_batch(vec![
                envelope(expense("hotel Roma")),
                envelope(expense("iPhone")),
            ])
            .unwrap();
        // A command logged before the rule, as the server or a replay sees it.
        let older = handle
            .lock()
            .unwrap()
            .execute(envelope(expense("pizza")))
            .unwrap();

        let core = handle.lock().unwrap();
        let note = |receipt: &Receipt| {
            core.transaction(vault, receipt.result_id.unwrap())
                .unwrap()
                .note
        };
        assert_eq!(note(&pharmacy).as_deref(), Some("Farmacia"));
        assert_eq!(note(&batch[0]).as_deref(), Some("Hotel Roma"));
        assert_eq!(note(&batch[1]).as_deref(), Some("iPhone"));
        assert_eq!(note(&older).as_deref(), Some("pizza"));

        let log = core.commands_since(vault, 0).unwrap();
        let mut fresh = Core::open_in_memory().unwrap();
        crate::replay(&log, &mut fresh).unwrap();
        let replayed = |receipt: &Receipt| {
            fresh
                .transaction(vault, receipt.result_id.unwrap())
                .unwrap()
                .note
        };
        assert_eq!(replayed(&pharmacy).as_deref(), Some("Farmacia"));
        assert_eq!(replayed(&older).as_deref(), Some("pizza"));
    }
}
