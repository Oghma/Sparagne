//! One-shot import of a Sparagne v1 database into a v2 [`Core`].
//!
//! The v1 engine kept state in materialized tables (`wallets.balance`,
//! `cash_flows.balance`) written by an ORM; v2 keeps a command log and derives
//! state from it. The import therefore does not copy rows: it **replays** the
//! v1 history as v2 commands, in the order the v2 rules expect, and lets the
//! core rebuild the projection. What the core refuses is collected in
//! [`ImportReport::rejected`] instead of aborting the run.
//!
//! # Which vaults are imported
//!
//! - `options.vault = Some(name)`: only the v1 vault with that name. No match
//!   is a [`DomainError::NotFound`].
//! - otherwise, when the v1 `users` table holds exactly one user (the normal
//!   case for a personal instance), every vault owned by that user. The v1
//!   owner is the only vault selector v1 ever had, and with one user it is
//!   unambiguous.
//! - otherwise (several v1 users, no filter), every vault in the file, with a
//!   note in the report: the importer has no way to tell which v1 username
//!   corresponds to the v2 `author`, and silently dropping data would be
//!   worse than importing it all.
//!
//! Every imported vault is owned by `options.author` in v2, whatever its v1
//! owner was, because v2 has one local database per account.
//!
//! # Order of the commands
//!
//! 1. `CreateVault`
//! 2. `CreateWallet` for every wallet
//! 3. `CreateFlow` for every non-system cash flow (v1's `unallocated` maps
//!    onto the Unallocated flow v2 creates with the vault)
//! 4. `CreateCategory` and `AddAlias`
//! 5. the transactions, ordered by `occurred_at` then id
//! 6. `VoidTransaction` for the voided ones
//! 7. the recurring templates
//! 8. the archive commands
//!
//! Archiving comes last because v2 refuses legs on archived wallets and flows
//! and refuses archived categories on new transactions; recurring templates
//! come before the archives for the same reason.
//!
//! # Opening balances
//!
//! v1 stored a wallet balance next to the legs, so the two could drift (the
//! server wrote the opening transaction with a second, non-atomic call). The
//! importer sums the v1 legs of every non-voided transaction per wallet and
//! compares the total with `wallets.balance`; the difference, when any, is
//! passed as `CreateWallet.opening_balance`, which is exactly how v2 models an
//! opening: an entry on Unallocated with the system category `Opening`. The
//! same sum is computed for flows, but a flow difference is only reported
//! ([`ImportReport::flow_balance_mismatches`]), never invented as a command:
//! money in a flow always comes from somewhere.
//!
//! # What is dropped
//!
//! - `flow_references` and `flow_memberships` are not imported at all
//!   (cross-vault flow sharing is not a v2 concept).
//!   The report counts the rows it skipped.
//! - `legs.attributed_user_id`: v2 attributes a whole transaction to its
//!   author, never a single leg.
//! - `transactions.idempotency_key`: in v2 the command id is the idempotency
//!   key.
//! - `transactions.refunded_transaction_id`: v2 refunds are not linked to the
//!   expense they undo.
//! - `transactions.created_by` and `voided_by`: every imported command is
//!   authored by `options.author`, and names no person, so every imported row
//!   is the importer's.
//! - `transactions.voided_at`: `Command::VoidTransaction` carries no
//!   timestamp, so a voided row gets the import time as `voided_at`.
//! - `recurring_templates.created_by` and `created_at`, and the v1 `interval`
//!   that never existed (v2 schedules get `interval = 1`).
//! - category names that v2 refuses (empty after normalization) fall back to
//!   Uncategorized on their transactions.
//!
//! Every one of these has a counter in [`DroppedFields`].
//!
//! # Idempotency
//!
//! Command ids are UUID v5 over [`IMPORT_NAMESPACE`] from the v1 row id and a
//! role string, so the same v1 file always produces the same command ids, and
//! [`Core::execute`] returns `deduplicated` for an id already in the log
//! without applying anything. Running the import twice on the same v2 database
//! adds no command. A command the core *rejected* leaves no log row, so it is
//! retried (and normally rejected again) on the next run.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::{Connection, OpenFlags, Row, types::ValueRef};
use uuid::Uuid;

use crate::{
    Command, CommandEnvelope, Core, Currency, DomainError, Entry, FlowMode, Frequency,
    RecurringPatch, Schedule, TransactionKind, normalize_category_key, validate_category_name,
};

/// Namespace of every command id the importer mints. Generated once for this
/// importer and frozen: changing it would make a second import create a
/// duplicate of everything.
pub const IMPORT_NAMESPACE: Uuid = Uuid::from_u128(0x9c4b_3a17_5d28_4e6f_b0d1_2e7a_84f3_5c60);

/// Timezone used for the accounting day when the caller does not pick one.
pub const DEFAULT_TIMEZONE: &str = "Europe/Rome";

/// At most this many past periods of a recurring template are marked as
/// already handled. A daily template running for years would otherwise turn
/// into thousands of commands.
const MAX_SKIPPED_PERIODS: usize = 500;

// ---------------------------------------------------------------------------
// Options and report
// ---------------------------------------------------------------------------

/// How to run the import.
#[derive(Clone, Debug)]
pub struct ImportOptions {
    /// v2 author of every command: the username the app logs in with.
    pub author: String,
    /// Import only the v1 vault with this name; `None` imports all the
    /// selected ones (see the module docs).
    pub vault: Option<String>,
    /// IANA timezone used to give the v1 UTC instants the offset of the
    /// accounting day, e.g. `Europe/Rome`.
    pub timezone: String,
}

impl ImportOptions {
    /// Options for `author`, every vault, [`DEFAULT_TIMEZONE`].
    #[must_use]
    pub fn new(author: impl Into<String>) -> Self {
        Self {
            author: author.into(),
            vault: None,
            timezone: DEFAULT_TIMEZONE.to_string(),
        }
    }
}

/// Transactions imported, per v1 kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KindCounts {
    pub income: usize,
    pub expense: usize,
    pub refund: usize,
    pub transfer_wallet: usize,
    pub transfer_flow: usize,
}

impl KindCounts {
    /// Total across the five kinds.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.income + self.expense + self.refund + self.transfer_wallet + self.transfer_flow
    }

    fn bump(&mut self, kind: TransactionKind) {
        match kind {
            TransactionKind::Income => self.income += 1,
            TransactionKind::Expense => self.expense += 1,
            TransactionKind::Refund => self.refund += 1,
            TransactionKind::TransferWallet => self.transfer_wallet += 1,
            TransactionKind::TransferFlow => self.transfer_flow += 1,
        }
    }
}

/// v1 fields with no v2 counterpart, counted rather than forgotten.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DroppedFields {
    /// `flow_references` rows of the imported vaults.
    pub flow_references: usize,
    /// `flow_memberships` rows on flows of the imported vaults.
    pub flow_memberships: usize,
    /// Legs carrying an `attributed_user_id`.
    pub attributed_user_legs: usize,
    /// Transactions carrying an `idempotency_key`.
    pub idempotency_keys: usize,
    /// Refunds linked to the expense they undo.
    pub refund_links: usize,
    /// Transactions whose v1 `created_by` is not the v2 author.
    pub transaction_authors: usize,
    /// Voided transactions whose `voided_at` could not be preserved.
    pub void_timestamps: usize,
    /// v1 categories whose name v2 refuses; their transactions fall back to
    /// Uncategorized.
    pub unusable_category_names: usize,
    /// v1 categories that map onto a v2 system category instead of a new one.
    pub categories_mapped_to_system: usize,
    /// Recurring templates whose past periods exceeded
    /// [`MAX_SKIPPED_PERIODS`] and stay pending.
    pub recurring_history_truncated: usize,
}

/// One command the core refused. The import goes on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rejection {
    pub command_id: Uuid,
    /// `Command::kind_name`.
    pub kind: &'static str,
    /// `DomainError::code`.
    pub code: &'static str,
    pub message: String,
}

/// A flow whose v1 balance the v1 legs do not explain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowMismatch {
    pub flow: String,
    /// `balance` as v1 stored it.
    pub stored: i64,
    /// Sum of the v1 legs of the non-voided transactions.
    pub from_legs: i64,
}

/// What the import did.
#[derive(Clone, Debug, Default)]
pub struct ImportReport {
    pub vaults: usize,
    pub wallets: usize,
    /// Wallets that needed an opening entry to match their v1 balance.
    pub wallet_openings: usize,
    pub flows: usize,
    pub categories: usize,
    pub aliases: usize,
    pub transactions: KindCounts,
    pub voids: usize,
    pub recurring: usize,
    /// `SkipRecurring` commands standing in for `last_executed_date`.
    pub recurring_periods_skipped: usize,
    pub recurring_disabled: usize,
    pub recurring_archived: usize,
    pub archived_wallets: usize,
    pub archived_flows: usize,
    pub archived_categories: usize,
    pub dropped: DroppedFields,
    pub flow_balance_mismatches: Vec<FlowMismatch>,
    /// Commands the core applied for the first time.
    pub commands_executed: usize,
    /// Commands whose id was already in the log; nothing was applied.
    pub commands_deduplicated: usize,
    pub rejected: Vec<Rejection>,
    /// Free-text remarks: choices the importer made, workarounds, proposals.
    pub notes: Vec<String>,
}

impl fmt::Display for ImportReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "vaults                {}", self.vaults)?;
        writeln!(
            f,
            "wallets               {} ({} with an opening entry)",
            self.wallets, self.wallet_openings
        )?;
        writeln!(f, "flows                 {}", self.flows)?;
        writeln!(f, "categories            {}", self.categories)?;
        writeln!(f, "aliases               {}", self.aliases)?;
        writeln!(f, "transactions          {}", self.transactions.total())?;
        writeln!(f, "  income              {}", self.transactions.income)?;
        writeln!(f, "  expense             {}", self.transactions.expense)?;
        writeln!(f, "  refund              {}", self.transactions.refund)?;
        writeln!(
            f,
            "  transfer_wallet     {}",
            self.transactions.transfer_wallet
        )?;
        writeln!(
            f,
            "  transfer_flow       {}",
            self.transactions.transfer_flow
        )?;
        writeln!(f, "voids                 {}", self.voids)?;
        writeln!(
            f,
            "recurring             {} ({} disabled, {} archived, {} past periods marked handled)",
            self.recurring,
            self.recurring_disabled,
            self.recurring_archived,
            self.recurring_periods_skipped
        )?;
        writeln!(
            f,
            "archived              {} wallets, {} flows, {} categories",
            self.archived_wallets, self.archived_flows, self.archived_categories
        )?;
        writeln!(
            f,
            "commands              {} applied, {} already known",
            self.commands_executed, self.commands_deduplicated
        )?;
        writeln!(f, "not imported")?;
        writeln!(f, "  flow_references     {}", self.dropped.flow_references)?;
        writeln!(f, "  flow_memberships    {}", self.dropped.flow_memberships)?;
        writeln!(
            f,
            "  attributed legs     {}",
            self.dropped.attributed_user_legs
        )?;
        writeln!(f, "  idempotency keys    {}", self.dropped.idempotency_keys)?;
        writeln!(f, "  refund links        {}", self.dropped.refund_links)?;
        writeln!(
            f,
            "  foreign authors     {}",
            self.dropped.transaction_authors
        )?;
        writeln!(f, "  voided_at stamps    {}", self.dropped.void_timestamps)?;
        writeln!(
            f,
            "  system categories   {}",
            self.dropped.categories_mapped_to_system
        )?;
        writeln!(
            f,
            "  unusable names      {}",
            self.dropped.unusable_category_names
        )?;
        writeln!(
            f,
            "  truncated histories {}",
            self.dropped.recurring_history_truncated
        )?;
        if !self.flow_balance_mismatches.is_empty() {
            writeln!(f, "flow balances the legs do not explain")?;
            for m in &self.flow_balance_mismatches {
                writeln!(
                    f,
                    "  {}: stored {}, from legs {}",
                    m.flow, m.stored, m.from_legs
                )?;
            }
        }
        if self.rejected.is_empty() {
            writeln!(f, "rejected              0")?;
        } else {
            writeln!(f, "rejected              {}", self.rejected.len())?;
            for r in &self.rejected {
                writeln!(f, "  {} {} {}: {}", r.command_id, r.kind, r.code, r.message)?;
            }
        }
        for note in &self.notes {
            writeln!(f, "note: {note}")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Replay a v1 SQLite file into `core` as v2 commands.
///
/// `source` is opened read-only and never written. See the module docs for the
/// vault selection, the order of the commands and what is dropped.
///
/// # Errors
///
/// [`DomainError::Storage`] when the v1 file cannot be read or its rows cannot
/// be decoded, [`DomainError::NotFound`] when `options.vault` names a vault
/// that is not in the file, and [`DomainError::InvalidName`] when the timezone
/// is not an IANA name. Domain refusals of single commands are not errors:
/// they land in [`ImportReport::rejected`].
pub fn import_v1(
    core: &mut Core,
    source: &Path,
    options: &ImportOptions,
) -> Result<ImportReport, DomainError> {
    let tz: Tz = options.timezone.parse().map_err(|_| {
        DomainError::InvalidName(format!("unknown timezone '{}'", options.timezone))
    })?;
    let src = Connection::open_with_flags(
        source,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| DomainError::Storage(format!("opening '{}': {e}", source.display())))?;

    let mut importer = Importer {
        core,
        author: options.author.clone(),
        tz,
        report: ImportReport::default(),
    };
    let vaults = select_vaults(&src, options, &mut importer.report)?;
    for vault in &vaults {
        importer.import_vault(&src, vault)?;
    }
    Ok(importer.report)
}

// ---------------------------------------------------------------------------
// v1 rows
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct V1Vault {
    id: Uuid,
    name: String,
    currency: String,
}

#[derive(Clone, Debug)]
struct V1Wallet {
    id: Uuid,
    name: String,
    balance: i64,
    archived: bool,
}

#[derive(Clone, Debug)]
struct V1Flow {
    id: Uuid,
    name: String,
    system_kind: Option<String>,
    balance: i64,
    max_balance: Option<i64>,
    income_balance: Option<i64>,
    archived: bool,
    allow_negative: bool,
}

#[derive(Clone, Debug)]
struct V1Category {
    id: Uuid,
    name: String,
    archived: bool,
    is_system: bool,
}

#[derive(Clone, Debug)]
struct V1Alias {
    id: Uuid,
    category_id: Uuid,
    alias: String,
}

#[derive(Clone, Debug)]
struct V1Leg {
    target_kind: String,
    target_id: Uuid,
    amount: i64,
}

#[derive(Clone, Debug)]
struct V1Transaction {
    id: Uuid,
    kind: String,
    occurred_at: DateTime<Utc>,
    amount_minor: i64,
    category_id: Option<Uuid>,
    note: Option<String>,
    created_by: String,
    voided: bool,
    has_idempotency_key: bool,
    refund_link: bool,
    legs: Vec<V1Leg>,
}

#[derive(Clone, Debug)]
struct V1Recurring {
    id: Uuid,
    kind: String,
    amount_minor: i64,
    wallet_id: Option<Uuid>,
    flow_id: Option<Uuid>,
    category_id: Option<Uuid>,
    note: Option<String>,
    frequency: String,
    day_of_period: i64,
    start_date: NaiveDate,
    end_date: Option<NaiveDate>,
    enabled: bool,
    last_executed_date: Option<NaiveDate>,
    archived: bool,
}

/// How a v1 category reaches a v2 transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CategoryUse {
    /// Free text v2 resolves to an existing category.
    Name(String),
    /// Uncategorized.
    None,
}

// ---------------------------------------------------------------------------
// The importer
// ---------------------------------------------------------------------------

struct Importer<'a> {
    core: &'a mut Core,
    author: String,
    tz: Tz,
    report: ImportReport,
}

impl Importer<'_> {
    /// Execute one command. A domain refusal is recorded and swallowed; a
    /// storage failure aborts the import.
    fn run(&mut self, vault: Uuid, id: Uuid, command: Command) -> Result<bool, DomainError> {
        let kind = command.kind_name();
        let env = CommandEnvelope {
            id,
            vault_id: vault,
            author: self.author.clone(),
            command,
        };
        match self.core.execute(env) {
            Ok(receipt) => {
                if receipt.deduplicated {
                    self.report.commands_deduplicated += 1;
                } else {
                    self.report.commands_executed += 1;
                }
                Ok(true)
            }
            Err(DomainError::Storage(message)) => Err(DomainError::Storage(message)),
            Err(err) => {
                self.report.rejected.push(Rejection {
                    command_id: id,
                    kind,
                    code: err.code(),
                    message: err.to_string(),
                });
                Ok(false)
            }
        }
    }

    fn at(&self, instant: DateTime<Utc>) -> DateTime<FixedOffset> {
        instant.with_timezone(&self.tz).fixed_offset()
    }

    fn import_vault(&mut self, src: &Connection, vault: &V1Vault) -> Result<(), DomainError> {
        let currency = match Currency::try_from(vault.currency.as_str()) {
            Ok(currency) => currency,
            Err(err) => {
                self.report.rejected.push(Rejection {
                    command_id: role_id("vault", vault.id),
                    kind: "create_vault",
                    code: err.code(),
                    message: format!("vault '{}': {err}", vault.name),
                });
                return Ok(());
            }
        };
        let vault_id = role_id("vault", vault.id);
        if !self.run(
            vault_id,
            vault_id,
            Command::CreateVault {
                name: vault.name.clone(),
                currency,
            },
        )? {
            return Ok(());
        }
        self.report.vaults += 1;

        let wallets = read_wallets(src, vault.id)?;
        let flows = read_flows(src, vault.id)?;
        let categories = read_categories(src, vault.id)?;
        let aliases = read_aliases(src, vault.id)?;
        let (transactions, attributed_legs) = read_transactions(src, vault.id)?;
        let recurring = read_recurring(src, vault.id)?;
        self.report.dropped.attributed_user_legs += attributed_legs;
        self.report.dropped.flow_references += count_flow_references(src, vault.id)?;
        self.report.dropped.flow_memberships += count_flow_memberships(src, vault.id)?;

        let unallocated = self.core.snapshot(vault_id)?.unallocated_flow_id;
        let leg_totals = leg_totals(&transactions);
        let epoch = transactions
            .first()
            .map_or_else(Utc::now, |tx| tx.occurred_at);

        let targets = self.create_wallets(vault_id, &wallets, &leg_totals, epoch)?;
        let flow_targets = self.create_flows(vault_id, &flows, &leg_totals, unallocated, epoch)?;
        let targets: HashMap<Uuid, Uuid> = targets.into_iter().chain(flow_targets).collect();
        let category_uses = self.create_categories(vault_id, &categories, &aliases)?;

        self.import_transactions(vault_id, &transactions, &targets, &category_uses)?;
        self.import_recurring(vault_id, &recurring, &targets, &category_uses)?;
        self.archive(vault_id, &wallets, &flows, &categories)?;
        Ok(())
    }

    /// Wallets, with the part of their v1 balance the legs do not explain as
    /// an opening entry. Returns the v1 id -> v2 id map.
    fn create_wallets(
        &mut self,
        vault: Uuid,
        wallets: &[V1Wallet],
        leg_totals: &HashMap<Uuid, i64>,
        epoch: DateTime<Utc>,
    ) -> Result<HashMap<Uuid, Uuid>, DomainError> {
        let mut map = HashMap::new();
        for wallet in wallets {
            let from_legs = leg_totals.get(&wallet.id).copied().unwrap_or_default();
            let opening = wallet.balance - from_legs;
            let id = role_id("wallet", wallet.id);
            if self.run(
                vault,
                id,
                Command::CreateWallet {
                    name: wallet.name.clone(),
                    opening_balance: opening,
                    occurred_at: self.at(epoch),
                },
            )? {
                self.report.wallets += 1;
                if opening != 0 {
                    self.report.wallet_openings += 1;
                }
                map.insert(wallet.id, id);
            }
        }
        Ok(map)
    }

    /// Flows. The v1 `unallocated` system flow is not created: it maps onto
    /// the one v2 made with the vault.
    fn create_flows(
        &mut self,
        vault: Uuid,
        flows: &[V1Flow],
        leg_totals: &HashMap<Uuid, i64>,
        unallocated: Uuid,
        epoch: DateTime<Utc>,
    ) -> Result<HashMap<Uuid, Uuid>, DomainError> {
        let mut map = HashMap::new();
        for flow in flows {
            let from_legs = leg_totals.get(&flow.id).copied().unwrap_or_default();
            if flow.balance != from_legs {
                self.report.flow_balance_mismatches.push(FlowMismatch {
                    flow: flow.name.clone(),
                    stored: flow.balance,
                    from_legs,
                });
            }
            if flow.system_kind.as_deref() == Some("unallocated") {
                map.insert(flow.id, unallocated);
                continue;
            }
            let mode = match (flow.max_balance, flow.income_balance) {
                (None, _) => FlowMode::Unlimited,
                (Some(cap), None) => FlowMode::NetCapped { cap },
                (Some(cap), Some(_)) => FlowMode::IncomeCapped { cap },
            };
            let id = role_id("flow", flow.id);
            if self.run(
                vault,
                id,
                Command::CreateFlow {
                    name: flow.name.clone(),
                    mode,
                    allow_negative: flow.allow_negative,
                    opening_allocation: 0,
                    occurred_at: self.at(epoch),
                },
            )? {
                self.report.flows += 1;
                map.insert(flow.id, id);
            }
        }
        Ok(map)
    }

    /// Categories and aliases. A v1 category whose key is one of v2's system
    /// keys (`uncategorized`, `opening`) is not created: transactions name it
    /// and v2 resolves it to the system row.
    fn create_categories(
        &mut self,
        vault: Uuid,
        categories: &[V1Category],
        aliases: &[V1Alias],
    ) -> Result<HashMap<Uuid, CategoryUse>, DomainError> {
        let mut uses = HashMap::new();
        let mut created = HashSet::new();
        for category in categories {
            let key = normalize_category_key(&category.name).ok();
            match key.as_deref() {
                Some("uncategorized") => {
                    self.report.dropped.categories_mapped_to_system += 1;
                    uses.insert(category.id, CategoryUse::None);
                    continue;
                }
                Some("opening") => {
                    self.report.dropped.categories_mapped_to_system += 1;
                    uses.insert(category.id, CategoryUse::Name("opening".to_string()));
                    continue;
                }
                _ => {}
            }
            if category.is_system || validate_category_name(&category.name).is_err() {
                self.report.dropped.unusable_category_names += 1;
                uses.insert(category.id, CategoryUse::None);
                continue;
            }
            let id = role_id("category", category.id);
            if self.run(
                vault,
                id,
                Command::CreateCategory {
                    name: category.name.clone(),
                },
            )? {
                self.report.categories += 1;
                created.insert(category.id);
                uses.insert(category.id, CategoryUse::Name(category.name.clone()));
            } else {
                uses.insert(category.id, CategoryUse::None);
            }
        }
        for alias in aliases {
            if !created.contains(&alias.category_id) {
                continue;
            }
            let id = role_id("alias", alias.id);
            if self.run(
                vault,
                id,
                Command::AddAlias {
                    category_id: role_id("category", alias.category_id),
                    alias: alias.alias.clone(),
                },
            )? {
                self.report.aliases += 1;
            }
        }
        Ok(uses)
    }

    fn import_transactions(
        &mut self,
        vault: Uuid,
        transactions: &[V1Transaction],
        targets: &HashMap<Uuid, Uuid>,
        uses: &HashMap<Uuid, CategoryUse>,
    ) -> Result<(), DomainError> {
        for tx in transactions {
            if tx.has_idempotency_key {
                self.report.dropped.idempotency_keys += 1;
            }
            if tx.refund_link {
                self.report.dropped.refund_links += 1;
            }
            if tx.created_by != self.author {
                self.report.dropped.transaction_authors += 1;
            }
            let id = role_id("tx", tx.id);
            let kind = match TransactionKind::parse(&tx.kind) {
                Ok(kind) => kind,
                Err(err) => {
                    self.report.rejected.push(Rejection {
                        command_id: id,
                        kind: "income",
                        code: err.code(),
                        message: format!("transaction {}: {err}", tx.id),
                    });
                    continue;
                }
            };
            let command = match self.build_transaction(tx, kind, targets, uses) {
                Ok(command) => command,
                Err(rejection) => {
                    self.report.rejected.push(Rejection {
                        command_id: id,
                        ..rejection
                    });
                    continue;
                }
            };
            if self.run(vault, id, command)? {
                self.report.transactions.bump(kind);
            }
        }
        for tx in transactions.iter().filter(|tx| tx.voided) {
            self.report.dropped.void_timestamps += 1;
            let id = role_id("void", tx.id);
            if self.run(
                vault,
                id,
                Command::VoidTransaction {
                    transaction_id: role_id("tx", tx.id),
                },
            )? {
                self.report.voids += 1;
            }
        }
        Ok(())
    }

    /// The v2 command for one v1 transaction, or a rejection when its legs do
    /// not have the shape the kind requires (a cross-vault leg, for one: v1
    /// let a `flow_reference` put a leg on a flow of another vault).
    fn build_transaction(
        &self,
        tx: &V1Transaction,
        kind: TransactionKind,
        targets: &HashMap<Uuid, Uuid>,
        uses: &HashMap<Uuid, CategoryUse>,
    ) -> Result<Command, Rejection> {
        let bad = |message: String| Rejection {
            command_id: tx.id,
            kind: kind.as_str(),
            code: DomainError::InvalidCommand(String::new()).code(),
            message,
        };
        let missing = |target: Uuid| Rejection {
            command_id: tx.id,
            kind: kind.as_str(),
            code: DomainError::NotFound(String::new()).code(),
            message: format!(
                "transaction {}: leg on {target}, which is not a wallet or flow of this vault",
                tx.id
            ),
        };
        let resolve = |target: Uuid| targets.get(&target).copied().ok_or_else(|| missing(target));
        let occurred_at = self.at(tx.occurred_at);
        let note = tx.note.clone();

        match kind {
            TransactionKind::Income | TransactionKind::Expense | TransactionKind::Refund => {
                let wallet = tx.legs.iter().find(|l| l.target_kind == "wallet");
                let flow = tx.legs.iter().find(|l| l.target_kind == "flow");
                let (Some(wallet), Some(flow)) = (wallet, flow) else {
                    return Err(bad(format!(
                        "transaction {}: {} needs one wallet leg and one flow leg, found {}",
                        tx.id,
                        kind.as_str(),
                        tx.legs.len()
                    )));
                };
                let category = match tx.category_id.and_then(|id| uses.get(&id)) {
                    Some(CategoryUse::Name(name)) => Some(name.clone()),
                    _ => None,
                };
                // v1 authors are not v2 users: the row is the importer's,
                // like its author.
                let entry = Entry {
                    amount: tx.amount_minor,
                    wallet_id: Some(resolve(wallet.target_id)?),
                    flow_id: Some(resolve(flow.target_id)?),
                    category,
                    note,
                    occurred_at,
                    person: None,
                };
                Ok(match kind {
                    TransactionKind::Income => Command::Income(entry),
                    TransactionKind::Expense => Command::Expense(entry),
                    _ => Command::Refund(entry),
                })
            }
            TransactionKind::TransferWallet | TransactionKind::TransferFlow => {
                let want = if kind == TransactionKind::TransferWallet {
                    "wallet"
                } else {
                    "flow"
                };
                let from = tx
                    .legs
                    .iter()
                    .find(|l| l.target_kind == want && l.amount < 0);
                let to = tx
                    .legs
                    .iter()
                    .find(|l| l.target_kind == want && l.amount > 0);
                let (Some(from), Some(to)) = (from, to) else {
                    return Err(bad(format!(
                        "transaction {}: {} needs one negative and one positive {want} leg",
                        tx.id,
                        kind.as_str()
                    )));
                };
                let (from, to) = (resolve(from.target_id)?, resolve(to.target_id)?);
                Ok(if kind == TransactionKind::TransferWallet {
                    Command::TransferWallet {
                        amount: tx.amount_minor,
                        from_wallet_id: from,
                        to_wallet_id: to,
                        note,
                        occurred_at,
                    }
                } else {
                    Command::TransferFlow {
                        amount: tx.amount_minor,
                        from_flow_id: from,
                        to_flow_id: to,
                        note,
                        occurred_at,
                    }
                })
            }
        }
    }

    fn import_recurring(
        &mut self,
        vault: Uuid,
        templates: &[V1Recurring],
        targets: &HashMap<Uuid, Uuid>,
        uses: &HashMap<Uuid, CategoryUse>,
    ) -> Result<(), DomainError> {
        for template in templates {
            let id = role_id("recurring", template.id);
            let kind = match TransactionKind::parse(&template.kind) {
                Ok(kind) => kind,
                Err(err) => {
                    self.report.rejected.push(Rejection {
                        command_id: id,
                        kind: "create_recurring",
                        code: err.code(),
                        message: format!("recurring {}: {err}", template.id),
                    });
                    continue;
                }
            };
            let Some(schedule) = v1_schedule(template) else {
                self.report.rejected.push(Rejection {
                    command_id: id,
                    kind: "create_recurring",
                    code: DomainError::InvalidCommand(String::new()).code(),
                    message: format!(
                        "recurring {}: unknown frequency '{}'",
                        template.id, template.frequency
                    ),
                });
                continue;
            };
            let category = match template.category_id.and_then(|id| uses.get(&id)) {
                Some(CategoryUse::Name(name)) => Some(name.clone()),
                _ => None,
            };
            let created = self.run(
                vault,
                id,
                Command::CreateRecurring {
                    transaction_kind: kind,
                    amount: template.amount_minor,
                    wallet_id: template.wallet_id.and_then(|w| targets.get(&w).copied()),
                    flow_id: template.flow_id.and_then(|f| targets.get(&f).copied()),
                    category,
                    note: template.note.clone(),
                    schedule,
                    owner: None,
                },
            )?;
            if !created {
                continue;
            }
            self.report.recurring += 1;
            self.skip_past_periods(vault, template, schedule)?;
            if !template.enabled
                && self.run(
                    vault,
                    role_id("recurring_disable", template.id),
                    Command::UpdateRecurring {
                        recurring_id: id,
                        patch: RecurringPatch {
                            enabled: Some(false),
                            ..RecurringPatch::default()
                        },
                    },
                )?
            {
                self.report.recurring_disabled += 1;
            }
            if template.archived
                && self.run(
                    vault,
                    role_id("recurring_archive", template.id),
                    Command::ArchiveRecurring { recurring_id: id },
                )?
            {
                self.report.recurring_archived += 1;
            }
        }
        Ok(())
    }

    /// v1 tracked history as a single `last_executed_date`; v2 as one row per
    /// period. Every period up to that date becomes a `SkipRecurring`, so the
    /// app does not greet the user with years of pending periods.
    fn skip_past_periods(
        &mut self,
        vault: Uuid,
        template: &V1Recurring,
        schedule: Schedule,
    ) -> Result<(), DomainError> {
        let Some(last) = template.last_executed_date else {
            return Ok(());
        };
        let periods: Vec<NaiveDate> = schedule
            .occurrences()
            .take_while(|date| *date <= last)
            .take(MAX_SKIPPED_PERIODS + 1)
            .collect();
        if periods.len() > MAX_SKIPPED_PERIODS {
            self.report.dropped.recurring_history_truncated += 1;
        }
        for period in periods.into_iter().take(MAX_SKIPPED_PERIODS) {
            let id = command_id("recurring_skip", &format!("{}:{period}", template.id));
            if self.run(
                vault,
                id,
                Command::SkipRecurring {
                    recurring_id: role_id("recurring", template.id),
                    period_date: period,
                },
            )? {
                self.report.recurring_periods_skipped += 1;
            }
        }
        Ok(())
    }

    fn archive(
        &mut self,
        vault: Uuid,
        wallets: &[V1Wallet],
        flows: &[V1Flow],
        categories: &[V1Category],
    ) -> Result<(), DomainError> {
        for category in categories.iter().filter(|c| c.archived && !c.is_system) {
            if self.run(
                vault,
                role_id("category_archive", category.id),
                Command::ArchiveCategory {
                    category_id: role_id("category", category.id),
                },
            )? {
                self.report.archived_categories += 1;
            }
        }
        for flow in flows
            .iter()
            .filter(|f| f.archived && f.system_kind.is_none())
        {
            if self.run(
                vault,
                role_id("flow_archive", flow.id),
                Command::ArchiveFlow {
                    flow_id: role_id("flow", flow.id),
                },
            )? {
                self.report.archived_flows += 1;
            }
        }
        for wallet in wallets.iter().filter(|w| w.archived) {
            if self.run(
                vault,
                role_id("wallet_archive", wallet.id),
                Command::ArchiveWallet {
                    wallet_id: role_id("wallet", wallet.id),
                },
            )? {
                self.report.archived_wallets += 1;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Ids
// ---------------------------------------------------------------------------

/// Deterministic command id: v5 of `<role>:<key>` in [`IMPORT_NAMESPACE`].
#[must_use]
pub fn command_id(role: &str, key: &str) -> Uuid {
    Uuid::new_v5(&IMPORT_NAMESPACE, format!("{role}:{key}").as_bytes())
}

fn role_id(role: &str, v1_id: Uuid) -> Uuid {
    command_id(role, &v1_id.to_string())
}

// ---------------------------------------------------------------------------
// Reading the v1 file
// ---------------------------------------------------------------------------

fn select_vaults(
    src: &Connection,
    options: &ImportOptions,
    report: &mut ImportReport,
) -> Result<Vec<V1Vault>, DomainError> {
    let mut stmt = src.prepare("SELECT id, name, currency, user_id FROM vaults ORDER BY name")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            V1Vault {
                id: uuid_at(row, 0, "vaults.id")?,
                name: row.get(1)?,
                currency: row.get(2)?,
            },
            row.get::<_, String>(3)?,
        ))
    })?;
    let all: Vec<(V1Vault, String)> = rows.collect::<Result<_, _>>()?;

    if let Some(wanted) = &options.vault {
        let picked: Vec<V1Vault> = all
            .into_iter()
            .filter(|(v, _)| v.name.eq_ignore_ascii_case(wanted))
            .map(|(v, _)| v)
            .collect();
        if picked.is_empty() {
            return Err(DomainError::NotFound(format!("v1 vault '{wanted}'")));
        }
        return Ok(picked);
    }

    let users: Vec<String> = {
        let mut stmt = src.prepare("SELECT username FROM users ORDER BY username")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<_, _>>()?
    };
    match users.as_slice() {
        [only] => {
            let owner = only.clone();
            Ok(all
                .into_iter()
                .filter(|(_, o)| *o == owner)
                .map(|(v, _)| v)
                .collect())
        }
        [] => {
            report
                .notes
                .push("the v1 file has no user: every vault was imported".to_string());
            Ok(all.into_iter().map(|(v, _)| v).collect())
        }
        many => {
            report.notes.push(format!(
                "the v1 file has {} users ({}): every vault was imported, \
                 pass --vault to pick one",
                many.len(),
                many.join(", ")
            ));
            Ok(all.into_iter().map(|(v, _)| v).collect())
        }
    }
}

fn read_wallets(src: &Connection, vault: Uuid) -> Result<Vec<V1Wallet>, DomainError> {
    let mut stmt = src.prepare(
        "SELECT id, name, balance, archived FROM wallets WHERE vault_id = ?1 ORDER BY name",
    )?;
    let rows = stmt.query_map([vault.as_bytes().as_slice()], |row| {
        Ok(V1Wallet {
            id: uuid_at(row, 0, "wallets.id")?,
            name: row.get(1)?,
            balance: row.get(2)?,
            archived: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn read_flows(src: &Connection, vault: Uuid) -> Result<Vec<V1Flow>, DomainError> {
    let mut stmt = src.prepare(
        "SELECT id, name, system_kind, balance, max_balance, income_balance, archived, allow_negative
         FROM cash_flows WHERE vault_id = ?1 ORDER BY name",
    )?;
    let rows = stmt.query_map([vault.as_bytes().as_slice()], |row| {
        Ok(V1Flow {
            id: uuid_at(row, 0, "cash_flows.id")?,
            name: row.get(1)?,
            system_kind: row.get(2)?,
            balance: row.get(3)?,
            max_balance: row.get(4)?,
            income_balance: row.get(5)?,
            archived: row.get(6)?,
            allow_negative: row.get(7)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn read_categories(src: &Connection, vault: Uuid) -> Result<Vec<V1Category>, DomainError> {
    let mut stmt = src.prepare(
        "SELECT id, name, archived, is_system FROM categories WHERE vault_id = ?1 ORDER BY name",
    )?;
    let rows = stmt.query_map([vault.as_bytes().as_slice()], |row| {
        Ok(V1Category {
            id: uuid_at(row, 0, "categories.id")?,
            name: row.get(1)?,
            archived: row.get(2)?,
            is_system: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn read_aliases(src: &Connection, vault: Uuid) -> Result<Vec<V1Alias>, DomainError> {
    let mut stmt = src.prepare(
        "SELECT id, category_id, alias FROM category_aliases WHERE vault_id = ?1 ORDER BY alias",
    )?;
    let rows = stmt.query_map([vault.as_bytes().as_slice()], |row| {
        Ok(V1Alias {
            id: uuid_at(row, 0, "category_aliases.id")?,
            category_id: uuid_at(row, 1, "category_aliases.category_id")?,
            alias: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Transactions with their legs, ordered by `occurred_at` then id, which is
/// the order the v2 rules are checked in. The second value counts the legs
/// carrying an `attributed_user_id`, which v2 has nowhere to put.
fn read_transactions(
    src: &Connection,
    vault: Uuid,
) -> Result<(Vec<V1Transaction>, usize), DomainError> {
    let mut legs: HashMap<Uuid, Vec<V1Leg>> = HashMap::new();
    let mut attributed = 0;
    {
        let mut stmt = src.prepare(
            "SELECT l.transaction_id, l.target_kind, l.target_id, l.amount_minor,
                    l.attributed_user_id
             FROM legs l JOIN transactions t ON t.id = l.transaction_id
             WHERE t.vault_id = ?1 ORDER BY l.id",
        )?;
        let rows = stmt.query_map([vault.as_bytes().as_slice()], |row| {
            Ok((
                uuid_at(row, 0, "legs.transaction_id")?,
                V1Leg {
                    target_kind: row.get(1)?,
                    target_id: uuid_at(row, 2, "legs.target_id")?,
                    amount: row.get(3)?,
                },
                row.get::<_, Option<String>>(4)?.is_some(),
            ))
        })?;
        for row in rows {
            let (transaction_id, leg, is_attributed) = row?;
            if is_attributed {
                attributed += 1;
            }
            legs.entry(transaction_id).or_default().push(leg);
        }
    }

    let mut stmt = src.prepare(
        "SELECT id, kind, occurred_at, amount_minor, category_id, note, created_by,
                voided_at, idempotency_key, refunded_transaction_id
         FROM transactions WHERE vault_id = ?1 ORDER BY occurred_at, id",
    )?;
    let rows = stmt.query_map([vault.as_bytes().as_slice()], |row| {
        Ok(V1Transaction {
            id: uuid_at(row, 0, "transactions.id")?,
            kind: row.get(1)?,
            occurred_at: timestamp_at(row, 2, "transactions.occurred_at")?,
            amount_minor: row.get(3)?,
            category_id: optional_uuid_at(row, 4, "transactions.category_id")?,
            note: row.get(5)?,
            created_by: row.get(6)?,
            voided: !matches!(row.get_ref(7)?, ValueRef::Null),
            has_idempotency_key: row.get::<_, Option<String>>(8)?.is_some(),
            refund_link: !matches!(row.get_ref(9)?, ValueRef::Null),
            legs: Vec::new(),
        })
    })?;
    let mut out: Vec<V1Transaction> = rows.collect::<Result<_, _>>()?;
    for tx in &mut out {
        tx.legs = legs.remove(&tx.id).unwrap_or_default();
    }
    // SQLite sorted the text of `occurred_at`, which only matches the order of
    // the instants when every row uses the same format. Sort the decoded
    // values instead.
    out.sort_by(|left, right| {
        left.occurred_at
            .cmp(&right.occurred_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok((out, attributed))
}

fn read_recurring(src: &Connection, vault: Uuid) -> Result<Vec<V1Recurring>, DomainError> {
    if !table_exists(src, "recurring_templates")? {
        return Ok(Vec::new());
    }
    let mut stmt = src.prepare(
        "SELECT id, kind, amount_minor, wallet_id, flow_id, category_id, note, frequency,
                day_of_period, start_date, end_date, enabled, last_executed_date, archived_at
         FROM recurring_templates WHERE vault_id = ?1 ORDER BY start_date, id",
    )?;
    let rows = stmt.query_map([vault.as_bytes().as_slice()], |row| {
        Ok(V1Recurring {
            id: uuid_at(row, 0, "recurring_templates.id")?,
            kind: row.get(1)?,
            amount_minor: row.get(2)?,
            wallet_id: optional_uuid_at(row, 3, "recurring_templates.wallet_id")?,
            flow_id: optional_uuid_at(row, 4, "recurring_templates.flow_id")?,
            category_id: optional_uuid_at(row, 5, "recurring_templates.category_id")?,
            note: row.get(6)?,
            frequency: row.get(7)?,
            day_of_period: row.get(8)?,
            start_date: date_at(row, 9, "recurring_templates.start_date")?,
            end_date: optional_date_at(row, 10, "recurring_templates.end_date")?,
            enabled: row.get(11)?,
            last_executed_date: optional_date_at(
                row,
                12,
                "recurring_templates.last_executed_date",
            )?,
            archived: !matches!(row.get_ref(13)?, ValueRef::Null),
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn count_flow_references(src: &Connection, vault: Uuid) -> Result<usize, DomainError> {
    if !table_exists(src, "flow_references")? {
        return Ok(0);
    }
    let count: i64 = src.query_row(
        "SELECT COUNT(*) FROM flow_references WHERE vault_id = ?1",
        [vault.as_bytes().as_slice()],
        |row| row.get(0),
    )?;
    Ok(usize::try_from(count).unwrap_or_default())
}

fn count_flow_memberships(src: &Connection, vault: Uuid) -> Result<usize, DomainError> {
    if !table_exists(src, "flow_memberships")? {
        return Ok(0);
    }
    let count: i64 = src.query_row(
        "SELECT COUNT(*) FROM flow_memberships m
         JOIN cash_flows c ON c.id = m.flow_id
         WHERE c.vault_id = ?1",
        [vault.as_bytes().as_slice()],
        |row| row.get(0),
    )?;
    Ok(usize::try_from(count).unwrap_or_default())
}

fn table_exists(src: &Connection, name: &str) -> Result<bool, DomainError> {
    Ok(src.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [name],
        |row| row.get(0),
    )?)
}

/// Signed sum of the legs of the non-voided transactions, per target.
fn leg_totals(transactions: &[V1Transaction]) -> HashMap<Uuid, i64> {
    let mut totals: HashMap<Uuid, i64> = HashMap::new();
    for tx in transactions.iter().filter(|tx| !tx.voided) {
        for leg in &tx.legs {
            *totals.entry(leg.target_id).or_default() += leg.amount;
        }
    }
    totals
}

fn v1_schedule(template: &V1Recurring) -> Option<Schedule> {
    let day = u8::try_from(template.day_of_period).ok();
    let frequency = match template.frequency.as_str() {
        "daily" => Frequency::Daily,
        "weekly" => Frequency::Weekly { weekday: day? },
        "monthly" => Frequency::Monthly { day: day? },
        "yearly" => Frequency::Yearly {
            month: u8::try_from(template.day_of_period / 100).ok()?,
            day: u8::try_from(template.day_of_period % 100).ok()?,
        },
        _ => return None,
    };
    Some(Schedule {
        frequency,
        interval: 1,
        start_date: template.start_date,
        end_date: template.end_date,
    })
}

// ---------------------------------------------------------------------------
// Column decoding
// ---------------------------------------------------------------------------

/// A v1 column that does not hold what the v1 schema promised. Wrapped in a
/// `rusqlite::Error` so it travels through `query_map` and comes out of the
/// import as a [`DomainError::Storage`].
#[derive(Debug)]
struct DecodeError(String);

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DecodeError {}

fn decode_error(index: usize, message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        rusqlite::types::Type::Text,
        Box::new(DecodeError(message)),
    )
}

/// v1 wrote UUIDs as 16-byte blobs; text is accepted so a file dumped and
/// reloaded by hand still imports.
fn uuid_at(row: &Row<'_>, index: usize, what: &str) -> rusqlite::Result<Uuid> {
    let decoded = match row.get_ref(index)? {
        ValueRef::Blob(bytes) => Uuid::from_slice(bytes).ok(),
        ValueRef::Text(bytes) => std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| Uuid::parse_str(text).ok()),
        _ => None,
    };
    decoded.ok_or_else(|| decode_error(index, format!("{what} is not a uuid")))
}

fn optional_uuid_at(row: &Row<'_>, index: usize, what: &str) -> rusqlite::Result<Option<Uuid>> {
    if matches!(row.get_ref(index)?, ValueRef::Null) {
        return Ok(None);
    }
    uuid_at(row, index, what).map(Some)
}

/// v1 timestamps are SeaORM `DateTimeUtc`: text in practice, with or without
/// an offset depending on the driver version. Integers are read as Unix
/// seconds.
fn timestamp_at(row: &Row<'_>, index: usize, what: &str) -> rusqlite::Result<DateTime<Utc>> {
    let decoded = match row.get_ref(index)? {
        ValueRef::Integer(seconds) => Utc.timestamp_opt(seconds, 0).single(),
        ValueRef::Real(seconds) => Utc.timestamp_opt(seconds.trunc() as i64, 0).single(),
        ValueRef::Text(bytes) => std::str::from_utf8(bytes)
            .ok()
            .and_then(parse_timestamp_text),
        ValueRef::Null | ValueRef::Blob(_) => None,
    };
    decoded.ok_or_else(|| decode_error(index, format!("{what} is not a timestamp")))
}

fn date_at(row: &Row<'_>, index: usize, what: &str) -> rusqlite::Result<NaiveDate> {
    let text: String = row.get(index)?;
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d")
        .map_err(|e| decode_error(index, format!("{what}: '{text}' is not a date ({e})")))
}

fn optional_date_at(
    row: &Row<'_>,
    index: usize,
    what: &str,
) -> rusqlite::Result<Option<NaiveDate>> {
    if matches!(row.get_ref(index)?, ValueRef::Null) {
        return Ok(None);
    }
    date_at(row, index, what).map(Some)
}

fn parse_timestamp_text(text: &str) -> Option<DateTime<Utc>> {
    let text = text.trim();
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Some(parsed.with_timezone(&Utc));
    }
    for format in ["%Y-%m-%d %H:%M:%S%.f%:z", "%Y-%m-%dT%H:%M:%S%.f%:z"] {
        if let Ok(parsed) = DateTime::parse_from_str(text, format) {
            return Some(parsed.with_timezone(&Utc));
        }
    }
    for format in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(text, format) {
            return Some(Utc.from_utc_datetime(&naive));
        }
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .map(|date| Utc.from_utc_datetime(&date.and_time(NaiveTime::MIN)))
}
