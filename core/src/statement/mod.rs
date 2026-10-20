//! Importing a bank or card statement from CSV: [`detect`] reads the header
//! and recognises a preset, [`Core::preview_statement`] says what becomes of
//! each row under a [`StatementMapping`] and the user's
//! [`StatementRowOverride`]s, and [`Core::import_statement`], fed the same
//! overrides, writes exactly that under ids derived from each row, so a
//! statement imported again adds nothing. `plan` reads and decides the rows,
//! `matching` pairs them with what the vault holds; the rules, with their
//! thresholds and known limits, are in `docs/STATEMENTS.md`.

use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    Command, CommandEnvelope, Core, DomainError, Result, TransactionKind, TransactionView,
};

mod amount;
mod csv;
mod dates;
mod guess;
mod matching;
mod plan;
mod presets;

/// Namespace of every command id a statement row gets. Generated once for the
/// statement import and frozen: changing it would make the next import of a
/// statement duplicate every row.
pub const STATEMENT_NAMESPACE: Uuid = Uuid::from_u128(0xe21a_7afd_0f46_4de7_8a8f_5524_1a97_b9f5);

/// Data rows [`detect`] returns in [`StatementDetection::sample`].
const SAMPLE_ROWS: usize = 5;

/// How the columns of a statement map to a transaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct StatementMapping {
    /// One character: `,`, `;` or a tab.
    pub delimiter: String,
    pub date_column: String,
    pub date_format: StatementDateFormat,
    pub amount_column: String,
    pub amount_sign: AmountSign,
    /// `true` when the decimal separator is a comma (`12,50`).
    pub decimal_comma: bool,
    /// Joined with a space to make the note.
    pub description_columns: Vec<String>,
    pub type_column: Option<String>,
    /// What to do with a row whose type column holds `value`.
    pub type_rules: Vec<StatementTypeRule>,
    /// What to do with a row no type rule matched (or with no type column).
    pub default_action: StatementAction,
    pub status_column: Option<String>,
    /// Rows whose status is one of these are skipped (case-insensitive).
    pub skip_statuses: Vec<String>,
    pub currency_column: Option<String>,
    /// The bank's own category; only ever matched against existing ones.
    pub category_column: Option<String>,
    pub original_amount_column: Option<String>,
    pub original_currency_column: Option<String>,
    /// Tried in order before the type rules. Left out of the JSON when empty,
    /// so a mapping saved before rules existed reads back, and one without
    /// rules is written as it always was.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[uniffi(default = [])]
    pub description_rules: Vec<StatementDescriptionRule>,
    /// The column naming the account a row belongs to, in an export holding
    /// several (the BudgetBakers Wallet app's): each account goes to the
    /// wallet [`StatementOptions::account_wallets`] gives it. Left out of the
    /// JSON when `None`, like the next one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[uniffi(default = None)]
    pub account_column: Option<String>,
    /// The column saying a row is one half of a move between two of the
    /// user's accounts (`true`, `1`, `yes`, `y`, `si`, `sì` or `x`, whatever
    /// the case).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[uniffi(default = None)]
    pub transfer_column: Option<String>,
}

/// Where the rows of one account of the file go.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StatementAccountWallet {
    /// As the account column writes it; compared trimmed, whatever the case.
    pub account: String,
    /// `None` leaves the account's rows out (`skipped_account`).
    pub wallet_id: Option<Uuid>,
}

/// One entry of [`StatementMapping::description_rules`]: what a row whose
/// description holds `contains` is. Matched case-insensitively against the
/// joined description columns, inner runs of spaces collapsed on both sides;
/// the first rule that matches decides on its own, and a blank `contains`
/// matches nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct StatementDescriptionRule {
    pub contains: String,
    /// `None` keeps what the type rules or the default action say, so a rule
    /// may only clean the note or pick the category.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[uniffi(default = None)]
    pub action: Option<StatementAction>,
    /// Typed by the user, so it may create a category.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[uniffi(default = None)]
    pub category: Option<String>,
    /// Replaces the payee as the note; an original amount is still appended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[uniffi(default = None)]
    pub note: Option<String>,
}

/// One entry of [`StatementMapping::type_rules`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct StatementTypeRule {
    pub value: String,
    pub action: StatementAction,
}

/// How the date column is written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StatementDateFormat {
    /// `2026-09-16 08:54:40 UTC`.
    DateTimeUtc,
    /// `2026-09-16`, placed at local noon.
    IsoDate,
    /// `2026-09-16T08:54:40` with or without an offset; local time without.
    IsoDateTime,
    /// `16/09/2026`, `16.09.2026` or `16-09-2026`, placed at local noon.
    DayMonthYear,
    /// `09/16/2026`, placed at local noon.
    MonthDayYear,
    /// A chrono `strftime` pattern.
    Custom { pattern: String },
}

/// Which sign the amount column gives money going out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum AmountSign {
    /// Spending is positive (card exports).
    OutflowPositive,
    /// Spending is negative (bank exports).
    OutflowNegative,
}

/// What a statement row becomes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StatementAction {
    /// Expense when money goes out, income when it comes in.
    #[default]
    BySign,
    Expense,
    Income,
    Refund,
    /// Money moved in from another wallet of the vault; skipped until the
    /// wallet is chosen.
    TransferIn {
        from_wallet_id: Option<Uuid>,
    },
    /// Money moved out to another wallet of the vault; skipped until the
    /// wallet is chosen.
    TransferOut {
        to_wallet_id: Option<Uuid>,
    },
    Skip,
}

/// Where the imported rows go.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StatementOptions {
    pub wallet_id: Uuid,
    /// `None` is Unallocated.
    pub flow_id: Option<Uuid>,
    /// IANA name used for rows without an offset, e.g. `Europe/Rome`.
    pub timezone: String,
    /// Wallets other than `wallet_id` where a hand-entered row may be the
    /// duplicate of a statement row: a duplicate found there is moved into
    /// `wallet_id` by default ([`StatementMatchChoice::MoveExisting`]), and an
    /// entry there standing for a transfer row may be replaced by it
    /// ([`StatementMatchChoice::ReplaceExisting`]). Never a wallet that is
    /// itself a statement's.
    #[uniffi(default = [])]
    pub also_check_wallets: Vec<Uuid>,
    /// Rows dated before this day (in their own offset) are skipped with
    /// `before_start`; transfers with the wallets also checked dated before
    /// it, and entries of those wallets dated more than 3 days before it,
    /// match no row.
    #[uniffi(default = None)]
    pub since: Option<NaiveDate>,
    /// The other wallets the app knows receive bank statements (from the
    /// imports it remembers). With the wallets a claim or a move names, they
    /// are the statements' wallets: never advised as
    /// [`StatementPreview::lookalikes`], and their imported rows standing for
    /// a transfer row's other side are replaced by default up to 4 days
    /// away. A wallet not in the vault is no statement's.
    #[uniffi(default = [])]
    pub statement_wallets: Vec<Uuid>,
    /// Where each account of [`StatementMapping::account_column`] goes; an
    /// account not listed goes to `wallet_id`.
    #[uniffi(default = [])]
    pub account_wallets: Vec<StatementAccountWallet>,
}

/// What becomes of a row flagged [`StatementRowStatus::PossibleDuplicate`]
/// or [`StatementRowStatus::MatchedTransfer`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum StatementMatchChoice {
    /// Leave the row out: the transaction already there stands for it, and
    /// the import records it. On a duplicate found in one of
    /// [`StatementOptions::also_check_wallets`] it reads as `MoveExisting`,
    /// on an entry standing for the row's transfer as `ImportAnyway`.
    Skip,
    /// Import the row as if nothing matched it.
    ImportAnyway,
    /// Leave the row out and move the transaction it matched into the
    /// statement's wallet (`UpdateTransaction` with its `wallet_id`). Only for
    /// a possible duplicate found in one of
    /// [`StatementOptions::also_check_wallets`]; on an entry standing for the
    /// row's transfer it reads as `ImportAnyway`, anywhere else as `Skip`.
    MoveExisting,
    /// Void the entry the row matched and import the row as the transfer
    /// between wallets it is, all at once: for a transfer row whose other
    /// side is already in the vault as an income, expense or refund (the
    /// other bank's row imported without a rule, or typed by hand). The
    /// default only when the entry very likely is that side: the other
    /// bank's statement row up to 4 days away, or an entry dated the row's
    /// day or a day off in no wallet also checked; never one dated before
    /// the start day. Only for such a match; anywhere else it reads as the
    /// default.
    ReplaceExisting,
}

/// A user's change to one previewed row.
#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct StatementRowOverride {
    /// 1-based line of the row in the file, header included.
    pub line: u32,
    /// Typed by the user, so it may create a category.
    pub category: Option<String>,
    pub note: Option<String>,
    /// Leaves the row out (`skipped_by_you`), matched or not: nothing is
    /// written or claimed for it, so the next import shows it again.
    pub skip: bool,
    /// What the row is, over the description rules, the type rules and the
    /// default action. Never part of the row's id.
    #[uniffi(default = None)]
    pub action: Option<StatementAction>,
    /// The envelope of an entry, over [`StatementOptions::flow_id`]; the
    /// vault's Unallocated flow id names Unallocated, and an archived or
    /// unknown envelope is ignored.
    #[uniffi(default = None)]
    pub flow_id: Option<Uuid>,
    /// What becomes of a flagged row, over the default
    /// ([`StatementRow::default_choice`]).
    #[uniffi(default = None)]
    pub on_match: Option<StatementMatchChoice>,
}

/// What would happen to a previewed row.
#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Enum)]
pub enum StatementRowStatus {
    #[default]
    New,
    AlreadyImported,
    /// An entry already in the vault looks like this row: the money moving
    /// the same way (an expense, or an income or a refund), the same amount,
    /// at most 3 days apart, in the statement's wallet or in one of
    /// [`StatementOptions::also_check_wallets`]; or, for a transfer row, an
    /// entry standing for its transfer, at most 4 days apart (in the wallet
    /// it names, the money moving the other way). It is in
    /// [`StatementRow::existing`], and [`StatementRow::match_choice`] says
    /// what the import does; a row replacing its entry stays flagged and is
    /// written.
    PossibleDuplicate,
    /// The other side of a transfer between wallets already in the vault:
    /// same amount, the right direction, at most 4 days apart. It is in
    /// [`StatementRow::existing`]; the row is left out unless the user
    /// chooses [`StatementMatchChoice::ImportAnyway`].
    MatchedTransfer,
    /// Not imported on purpose. `code` is stable and meant for the app to
    /// translate; `reason` is the English detail:
    ///
    /// - `skipped_by_rule`: a description rule, a type rule or the default
    ///   action says `Skip` (the reason names the rule);
    /// - `skipped_status`: the status is in `skip_statuses` (the reason
    ///   names it);
    /// - `needs_wallet`: a transfer whose other wallet is not chosen yet, or
    ///   is one the money cannot move to or from: the imported wallet, one
    ///   not in the vault, an archived one;
    /// - `zero_amount`: the amount is zero;
    /// - `skipped_by_you`: a [`StatementRowOverride`] with `skip`, or a
    ///   `Skip` action of the user's;
    /// - `before_start`: dated before [`StatementOptions::since`];
    /// - `skipped_account`: its account goes to no wallet
    ///   ([`StatementAccountWallet::wallet_id`] is `None`);
    /// - `internal_transfer`: one half of a move between two accounts of the
    ///   file that go to one wallet, where it moves nothing;
    /// - `transfer_pair`: the money-in half of a move between two accounts
    ///   of the file, written as the transfer of the other half
    ///   ([`StatementRow::paired_line`]).
    Skipped {
        code: String,
        reason: String,
    },
    /// Cannot be imported as it is: `invalid_row`, `invalid_date`,
    /// `invalid_amount` or `currency_mismatch`, with an English message.
    Invalid {
        code: String,
        message: String,
    },
}

/// One row of a [`StatementPreview`].
#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct StatementRow {
    /// 1-based line in the file, header included.
    pub line: u32,
    /// The id the row's command gets: stable across imports.
    pub command_id: Uuid,
    pub status: StatementRowStatus,
    pub kind: Option<TransactionKind>,
    pub occurred_at: Option<DateTime<FixedOffset>>,
    /// Minor units, always positive.
    pub amount: i64,
    /// The amount had more decimals than the currency and was rounded.
    pub rounded: bool,
    /// The description columns, trimmed and joined.
    pub payee: String,
    pub bank_category: Option<String>,
    /// An existing category the bank's matched by name or alias.
    pub matched_category: Option<String>,
    /// The other wallet of a transfer.
    pub counter_wallet_id: Option<Uuid>,
    /// `12.34 USD` when the row carries an original amount and currency.
    pub original: Option<String>,
    /// What the row is taken for: the user's action, else the description
    /// rule's, else the type rule's, else the default action.
    pub action: StatementAction,
    /// The money goes out, as the file signs it; `false` when the amount is
    /// unreadable.
    pub outflow: bool,
    /// The note before the user's own: the description rule's, else the
    /// payee. The original amount is appended to it on import.
    pub note: String,
    /// The category before the user's own: the description rule's, else
    /// [`Self::matched_category`].
    pub category: Option<String>,
    /// Index in [`StatementMapping::description_rules`] of the rule that
    /// matched the row.
    pub description_rule: Option<u32>,
    /// The transaction a flagged row matched. It stays when the user imports
    /// the row anyway, so the review can still show it.
    pub existing: Option<TransactionView>,
    /// What becomes of a row that matched: the user's choice, else the
    /// default. `None` when nothing matched.
    pub match_choice: Option<StatementMatchChoice>,
    /// What [`Self::match_choice`] would be without the user's `on_match`.
    /// `None` when nothing matched.
    #[uniffi(default = None)]
    pub default_choice: Option<StatementMatchChoice>,
    /// What the row is taken for before the user's own action: the
    /// description rule's, else the type rule's, else the default action.
    /// `None` for a row that could not be read as one (`invalid_row`).
    #[uniffi(default = None)]
    pub base_action: Option<StatementAction>,
    /// The wallet the row goes to: its account's, else
    /// [`StatementOptions::wallet_id`]. `None` for a row of an account left
    /// out, or one that could not be read as a row (`invalid_row`).
    #[uniffi(default = None)]
    pub wallet_id: Option<Uuid>,
    /// The account column's value, trimmed.
    #[uniffi(default = None)]
    pub account: Option<String>,
    /// The transfer column says the row is one half of a move between two
    /// of the user's accounts. Set on a half with no other side in the file
    /// too, which is then imported as what its type says.
    #[uniffi(default = false)]
    pub transfer_leg: bool,
    /// The line of the other half the row was paired with in the file.
    #[uniffi(default = None)]
    pub paired_line: Option<u32>,
}

/// One value of a statement's column, and how many rows hold it
/// ([`column_values`]).
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StatementColumnValue {
    /// As the first row holding it writes it, trimmed.
    pub value: String,
    pub rows: u32,
}

/// A wallet the statement does not check for rows typed by hand where some
/// of its new rows look like entries already there: advice to check it.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StatementLookalike {
    pub wallet_id: Uuid,
    /// New rows that matched nothing and would match an entry there.
    pub rows: u32,
}

/// Every row of a statement with what would happen to it, plus counts.
#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct StatementPreview {
    pub rows: Vec<StatementRow>,
    pub new_rows: u32,
    pub already_imported: u32,
    pub skipped: u32,
    pub invalid: u32,
    /// New rows rounded, and rows replacing their match rounded.
    pub rounded: u32,
    /// Rows flagged [`StatementRowStatus::PossibleDuplicate`], those
    /// replacing their match included.
    pub possible_duplicates: u32,
    /// Rows flagged [`StatementRowStatus::MatchedTransfer`].
    pub matched_transfers: u32,
    /// Possible duplicates whose match the import would move.
    pub moves: u32,
    /// Wallets not checked for rows typed by hand, nor any statement's (a
    /// claim or a move names it, or [`StatementOptions::statement_wallets`]
    /// holds it), where new rows that matched nothing look like entries
    /// already there,
    /// the most such rows first. Advice only: matching does not use them.
    #[uniffi(default = [])]
    pub lookalikes: Vec<StatementLookalike>,
}

/// Outcome of [`Core::import_statement`].
#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct StatementReport {
    /// Rows written, the transfer of a row replacing its match included.
    pub executed: u32,
    pub deduplicated: u32,
    pub skipped: u32,
    pub rounded: u32,
    pub rejected: Vec<StatementRejection>,
    /// Possible duplicates: left out (moved ones included), or replacing
    /// their match.
    pub possible_duplicates: u32,
    /// Matched transfers left out.
    pub matched_transfers: u32,
    /// Transactions moved into the statement's wallet.
    pub moved: u32,
}

/// A row the core refused.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StatementRejection {
    pub line: u32,
    pub code: String,
    pub message: String,
}

/// A built-in mapping, recognised from the header.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StatementPreset {
    pub id: String,
    pub name: String,
    pub mapping: StatementMapping,
}

/// What [`detect`] learned about a file before any mapping.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StatementDetection {
    pub delimiter: String,
    pub headers: Vec<String>,
    /// The built-in preset whose header matches, if any.
    pub preset_id: Option<String>,
    /// Data rows, header excluded.
    pub rows: u32,
    /// The first few data rows, for the mapping editor.
    pub sample: Vec<Vec<String>>,
}

/// How a statement's date and amount columns are written, as far as their
/// values tell ([`guess_formats`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct StatementFormatGuess {
    /// The first of UTC (only when every value says so), ISO date-time, ISO
    /// date, day-first and month-first that reads every value.
    pub date_format: Option<StatementDateFormat>,
    /// `true` when only a decimal comma reads every amount, `false` when
    /// only a decimal point does.
    pub decimal_comma: Option<bool>,
}

pub use guess::guess as guess_formats;
pub use presets::all as presets;

/// Delimiter, header, matching preset and a sample of a statement.
///
/// The delimiter is the one among `,`, `;` and tab the first rows agree on;
/// the header is the first non-blank row. A file with no row, or whose header
/// never closes a quote, is an [`DomainError::InvalidCommand`].
pub fn detect(text: &str) -> Result<StatementDetection> {
    let delimiter = csv::sniff_delimiter(text);
    let mut records = csv::records(text, delimiter);
    let headers = csv::header(&mut records)?;
    let mut rows = 0u32;
    let mut sample = Vec::new();
    for record in records {
        rows = rows.saturating_add(1);
        if sample.len() < SAMPLE_ROWS {
            sample.push(record.fields);
        }
    }
    Ok(StatementDetection {
        delimiter: delimiter.to_string(),
        preset_id: presets::matching(&headers).map(str::to_string),
        headers,
        rows,
        sample,
    })
}

/// The distinct non-empty values of `column` (named as a mapping names it,
/// trimmed, whatever the case) with how many data rows hold each, the most
/// rows first, then by value: for the app to list a file's accounts or type
/// values. Values compare trimmed, whatever the case, and a row that never
/// closes a quote is not read. Empty when the delimiter is not one
/// character or the column is not in the header.
#[must_use]
pub fn column_values(text: &str, delimiter: &str, column: &str) -> Vec<StatementColumnValue> {
    let Ok(delimiter) = plan::delimiter(delimiter) else {
        return Vec::new();
    };
    let mut records = csv::records(text, delimiter);
    let Some(index) = records
        .next()
        .and_then(|header| csv::column(&header.fields, column))
    else {
        return Vec::new();
    };
    let mut values: Vec<(String, StatementColumnValue)> = Vec::new();
    let mut known: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for record in records.filter(|record| !record.unclosed_quote) {
        let Some(value) = record
            .fields
            .get(index)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let key = value.to_lowercase();
        if let Some(&at) = known.get(&key) {
            values[at].1.rows += 1;
        } else {
            known.insert(key.clone(), values.len());
            values.push((
                key,
                StatementColumnValue {
                    value: value.to_string(),
                    rows: 1,
                },
            ));
        }
    }
    values.sort_by(|(a_key, a), (b_key, b)| b.rows.cmp(&a.rows).then_with(|| a_key.cmp(b_key)));
    values.into_iter().map(|(_, value)| value).collect()
}

/// A mapping as JSON, for the app to remember.
#[must_use]
pub fn encode_mapping(mapping: &StatementMapping) -> String {
    serde_json::to_string(mapping).unwrap_or_default()
}

/// A mapping back from [`encode_mapping`].
pub fn decode_mapping(json: &str) -> Result<StatementMapping> {
    serde_json::from_str(json)
        .map_err(|err| DomainError::InvalidCommand(format!("malformed statement mapping: {err}")))
}

impl Core {
    /// What importing `text` with the user's `overrides` would do, row by
    /// row: fed the same overrides, [`Core::import_statement`] does exactly
    /// this. Writes nothing.
    pub fn preview_statement(
        &self,
        vault_id: Uuid,
        text: &str,
        mapping: &StatementMapping,
        options: &StatementOptions,
        overrides: &[StatementRowOverride],
    ) -> Result<StatementPreview> {
        let plan::Plan {
            rows: planned,
            lookalikes,
        } = plan::plan(self, vault_id, text, mapping, options, overrides)?;
        let mut preview = StatementPreview {
            rows: Vec::with_capacity(planned.len()),
            lookalikes,
            ..StatementPreview::default()
        };
        for plan::Planned { row, .. } in planned {
            match row.status {
                StatementRowStatus::New => {
                    preview.new_rows += 1;
                    preview.rounded += u32::from(row.rounded);
                }
                StatementRowStatus::AlreadyImported => preview.already_imported += 1,
                StatementRowStatus::PossibleDuplicate => {
                    preview.possible_duplicates += 1;
                    preview.moves +=
                        u32::from(row.match_choice == Some(StatementMatchChoice::MoveExisting));
                    // A row replacing its match is written, rounded as it is.
                    if row.match_choice == Some(StatementMatchChoice::ReplaceExisting) {
                        preview.rounded += u32::from(row.rounded);
                    }
                }
                StatementRowStatus::MatchedTransfer => preview.matched_transfers += 1,
                StatementRowStatus::Skipped { .. } => preview.skipped += 1,
                StatementRowStatus::Invalid { .. } => preview.invalid += 1,
            }
            preview.rows.push(row);
        }
        Ok(preview)
    }

    /// Imports `text`, one command per row, with the user's `overrides`.
    /// Refused rows land in the report; only a storage error aborts.
    pub fn import_statement(
        &mut self,
        vault_id: Uuid,
        author: &str,
        text: &str,
        mapping: &StatementMapping,
        options: &StatementOptions,
        overrides: &[StatementRowOverride],
    ) -> Result<StatementReport> {
        let planned = plan::plan(self, vault_id, text, mapping, options, overrides)?.rows;
        let mut report = StatementReport::default();
        // Each row goes to its own wallet: its account's, else the options'.
        let wallet_of = |row: &StatementRow| row.wallet_id.unwrap_or(options.wallet_id);
        // Each new row's batch, with the claim of a lone transfer's own side
        // to write once it is in.
        let mut new = Vec::new();
        let mut later = Vec::new();
        for plan::Planned {
            row,
            command,
            replaces_other_row,
            paired_in,
        } in planned
        {
            match (&row.status, command) {
                (StatementRowStatus::New, Some(command)) => {
                    let write = match paired_in {
                        // A move between two accounts is written whole or not
                        // at all: the transfer and the claims of both halves.
                        Some(in_row) => (plan::paired(row.command_id, command, in_row), None),
                        None => {
                            let claim = plan::own_side(row.command_id, &command, wallet_of(&row));
                            (vec![(row.command_id, command)], claim)
                        }
                    };
                    new.push((row, write));
                }
                // So is a replacement: the transfer, the void of the entry and
                // their claims.
                (StatementRowStatus::PossibleDuplicate, Some(command)) => {
                    report.possible_duplicates += 1;
                    let batch = plan::replacing(&row, command, wallet_of(&row), replaces_other_row);
                    new.push((row, (batch, None)));
                }
                (StatementRowStatus::AlreadyImported, _) => report.deduplicated += 1,
                (StatementRowStatus::PossibleDuplicate, None) => {
                    report.possible_duplicates += 1;
                    later.extend(plan::afterwards(&row, wallet_of(&row)).map(|then| (row, then)));
                }
                (StatementRowStatus::MatchedTransfer, _) => {
                    report.matched_transfers += 1;
                    later.extend(plan::afterwards(&row, wallet_of(&row)).map(|then| (row, then)));
                }
                _ => report.skipped += 1,
            }
        }
        // `Some(deduplicated)` once the batch is in, `None` when the core
        // refused it; only a storage error stops the import.
        let mut write = |line, commands: Vec<(Uuid, Command)>, report: &mut StatementReport| {
            let batch = commands
                .into_iter()
                .map(|(id, command)| CommandEnvelope::with_id(id, vault_id, author, command))
                .collect();
            match self.execute_batch(batch) {
                Ok(receipts) => Ok(Some(receipts.first().is_some_and(|r| r.deduplicated))),
                Err(err @ DomainError::Storage(_)) => Err(err),
                Err(err) => {
                    report.rejected.push(StatementRejection {
                        line,
                        code: err.code().to_string(),
                        message: err.to_string(),
                    });
                    Ok(None)
                }
            }
        };
        // Oldest first, so the log reads in the order things happened; the
        // rows typed by hand move, and the matches are claimed, once the new
        // rows are in.
        new.sort_by_key(|(row, _)| (row.occurred_at, row.line));
        later.sort_by_key(|(row, _)| (row.occurred_at, row.line));
        for (row, (batch, claim)) in new {
            match write(row.line, batch, &mut report)? {
                Some(true) => report.deduplicated += 1,
                Some(false) => {
                    report.executed += 1;
                    report.rounded += u32::from(row.rounded);
                }
                None => continue,
            }
            if let Some(claim) = claim {
                write(row.line, vec![claim], &mut report)?;
            }
        }
        for (row, (id, command)) in later {
            // A move counts; a claim changes nothing.
            if let Some(deduplicated) = write(row.line, vec![(id, command)], &mut report)?
                && id == plan::move_id(row.command_id)
            {
                if deduplicated {
                    report.deduplicated += 1;
                } else {
                    report.moved += 1;
                }
            }
        }
        Ok(report)
    }
}
