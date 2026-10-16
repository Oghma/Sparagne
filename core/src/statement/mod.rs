//! Importing a bank or card statement from CSV.
//!
//! The app reads the file, [`detect`] guesses the delimiter and a built-in
//! preset from the header, the user adjusts a [`StatementMapping`], and
//! [`Core::preview_statement`] shows what would happen to each row before
//! [`Core::import_statement`] runs one command per row.
//!
//! Re-importing the same statement is a no-op: every row's command id is a
//! UUID v5 over a frozen namespace and the row's content (status left out, so
//! a row that went from pending to cleared is still the same row).
//!
//! # What becomes of a row
//!
//! The first reason that applies wins:
//!
//! 1. a row with fewer columns than the header, or more that are not empty,
//!    or that swallowed the rest of the file in an unclosed quote: `Invalid`
//!    (`invalid_row`);
//! 2. its id already in the vault's log: `AlreadyImported`, whatever the
//!    mapping now says about it;
//! 3. a `Skip` action, then a status in `skip_statuses`: `Skipped`;
//! 4. a currency other than the vault's: `Invalid` (`currency_mismatch`),
//!    then an unreadable date (`invalid_date`) or amount (`invalid_amount`);
//! 5. a zero amount, or a transfer whose other wallet is not chosen yet:
//!    `Skipped`;
//! 6. otherwise `New`.
//!
//! The action is the type rule whose value equals the type column
//! (case-insensitive), else the default action. `BySign` is an expense when
//! the money goes out and an income when it comes in. A sign that contradicts
//! the action (a refund written as money out) keeps the action with the
//! absolute amount: the rule says what the row is, and card exports write
//! top-ups with the sign of a spend.
//!
//! Amounts are read exactly from the text. More decimals than the currency
//! keeps are rounded half away from zero; [`StatementRow::rounded`] is set
//! only when that changed the value (`12.500` is not rounded). The original
//! amount, when the row has one in another currency or with another value,
//! is appended to the note in parentheses, after the payee or the user's
//! note.
//!
//! A bank category never creates one: it only selects an active category
//! whose name or alias it equals, with or without a `5462 - ` merchant code in
//! front. A category typed in a [`StatementRowOverride`] goes through the
//! normal resolution, so it may create one.
//!
//! # Ids
//!
//! The key of a row is `v1|vault|wallet|date|type|amount|payee|n`: the UTC
//! instant (`2026-09-16T08:54:40Z`) when the date column fixes one, else the
//! wall-clock time or the day as written, so importing from another timezone
//! finds the same rows; the type column's value (or the kind, without a type
//! column); the amount in minor units signed as the file wrote it; the payee
//! lowercased; and `n` the number of rows with the same key earlier in the
//! file, so two identical rows in the same second both import. Line numbers
//! are those of the file, header included; a row with a newline inside quotes
//! starts on its first.
//!
//! [`Core::import_statement`] runs one command per new row, oldest first,
//! with the row's id: refused rows land in the report, a storage error stops
//! the import (every command before it stays, and a new run deduplicates
//! them).

use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{CommandEnvelope, Core, DomainError, Result, TransactionKind, TransactionView};

mod amount;
mod csv;
mod dates;
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StatementAction {
    /// Expense when money goes out, income when it comes in.
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
    /// duplicate of a statement row: a row found there is moved into
    /// `wallet_id` by default ([`StatementMatchChoice::MoveExisting`]).
    #[uniffi(default = [])]
    pub also_check_wallets: Vec<Uuid>,
    /// Rows dated before this day (in their own offset) are skipped with
    /// `before_start`.
    #[uniffi(default = None)]
    pub since: Option<NaiveDate>,
}

/// What becomes of a row flagged [`StatementRowStatus::PossibleDuplicate`]
/// or [`StatementRowStatus::MatchedTransfer`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum StatementMatchChoice {
    /// Leave the row out: the transaction already there stands for it.
    Skip,
    /// Import the row as if nothing matched it.
    ImportAnyway,
    /// Leave the row out and move the transaction it matched into the
    /// statement's wallet (`UpdateTransaction` with its `wallet_id`). Only for
    /// a possible duplicate found in one of
    /// [`StatementOptions::also_check_wallets`]; anywhere else it reads as
    /// `Skip`.
    MoveExisting,
}

/// A user's change to one previewed row.
#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct StatementRowOverride {
    /// 1-based line of the row in the file, header included.
    pub line: u32,
    /// Typed by the user, so it may create a category.
    pub category: Option<String>,
    pub note: Option<String>,
    pub skip: bool,
    /// What the row is, over the description rules, the type rules and the
    /// default action. Never part of the row's id.
    #[uniffi(default = None)]
    pub action: Option<StatementAction>,
    /// The envelope of an entry, over [`StatementOptions::flow_id`]; the
    /// vault's Unallocated flow id names Unallocated.
    #[uniffi(default = None)]
    pub flow_id: Option<Uuid>,
    /// What becomes of a flagged row, over the default (move a duplicate
    /// found in another wallet, skip any other).
    #[uniffi(default = None)]
    pub on_match: Option<StatementMatchChoice>,
}

/// What would happen to a previewed row.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum StatementRowStatus {
    New,
    AlreadyImported,
    /// A transaction already in the vault looks like this row: same kind,
    /// same amount, at most 3 days apart, in the statement's wallet or in one
    /// of [`StatementOptions::also_check_wallets`]. It is in
    /// [`StatementRow::existing`], and [`StatementRow::match_choice`] says
    /// what the import does.
    PossibleDuplicate,
    /// The other side of a transfer between wallets already in the vault:
    /// same amount, the right direction, at most 4 days apart. It is in
    /// [`StatementRow::existing`]; the row is left out unless the user
    /// chooses [`StatementMatchChoice::ImportAnyway`].
    MatchedTransfer,
    /// Not imported on purpose. `code` is stable and meant for the app to
    /// translate; `reason` is the English detail:
    ///
    /// - `skipped_by_rule`: a type rule or the default action says `Skip`;
    /// - `skipped_status`: the status is in `skip_statuses` (the reason
    ///   names it);
    /// - `needs_wallet`: a transfer whose other wallet is not chosen yet;
    /// - `zero_amount`: the amount is zero;
    /// - `skipped_by_you`: a [`StatementRowOverride`] with `skip`, or a
    ///   `Skip` action of the user's;
    /// - `before_start`: dated before [`StatementOptions::since`].
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
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
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
}

/// Every row of a statement with what would happen to it, plus counts.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StatementPreview {
    pub rows: Vec<StatementRow>,
    pub new_rows: u32,
    pub already_imported: u32,
    pub skipped: u32,
    pub invalid: u32,
    pub rounded: u32,
    /// Rows flagged [`StatementRowStatus::PossibleDuplicate`].
    pub possible_duplicates: u32,
    /// Rows flagged [`StatementRowStatus::MatchedTransfer`].
    pub matched_transfers: u32,
    /// Possible duplicates whose match the import would move.
    pub moves: u32,
}

/// Outcome of [`Core::import_statement`].
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StatementReport {
    pub executed: u32,
    pub deduplicated: u32,
    pub skipped: u32,
    pub rounded: u32,
    pub rejected: Vec<StatementRejection>,
    /// Possible duplicates left out, moved ones included.
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

/// The built-in presets.
#[must_use]
pub fn presets() -> Vec<StatementPreset> {
    presets::all()
}

/// Delimiter, header, matching preset and a sample of a statement.
///
/// The delimiter is the one among `,`, `;` and tab the first rows agree on;
/// the header is the first non-blank row. A file with no row, or whose header
/// never closes a quote, is an [`DomainError::InvalidCommand`].
pub fn detect(text: &str) -> Result<StatementDetection> {
    let delimiter = csv::sniff_delimiter(text);
    let mut records = csv::records(text, delimiter);
    let header = records
        .next()
        .ok_or_else(|| DomainError::InvalidCommand("the statement is empty".to_string()))?;
    if header.unclosed_quote {
        return Err(DomainError::InvalidCommand(
            "the header of the statement never closes a quote".to_string(),
        ));
    }
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
        preset_id: presets::matching(&header.fields).map(str::to_string),
        headers: header.fields,
        rows,
        sample,
    })
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
    /// What importing `text` would do, row by row. Writes nothing.
    pub fn preview_statement(
        &self,
        vault_id: Uuid,
        text: &str,
        mapping: &StatementMapping,
        options: &StatementOptions,
        overrides: &[StatementRowOverride],
    ) -> Result<StatementPreview> {
        let mut planned = plan::plan(self, vault_id, text, mapping, options)?;
        plan::apply_overrides(&mut planned, overrides);
        let mut preview = StatementPreview {
            rows: Vec::with_capacity(planned.len()),
            new_rows: 0,
            already_imported: 0,
            skipped: 0,
            invalid: 0,
            rounded: 0,
            possible_duplicates: 0,
            matched_transfers: 0,
            moves: 0,
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
        let mut planned = plan::plan(self, vault_id, text, mapping, options)?;
        plan::apply_overrides(&mut planned, overrides);
        let mut report = StatementReport {
            executed: 0,
            deduplicated: 0,
            skipped: 0,
            rounded: 0,
            rejected: Vec::new(),
            possible_duplicates: 0,
            matched_transfers: 0,
            moved: 0,
        };
        let mut new = Vec::new();
        for plan::Planned { row, command } in planned {
            match (&row.status, command) {
                (StatementRowStatus::New, Some(command)) => new.push((row, command)),
                (StatementRowStatus::AlreadyImported, _) => report.deduplicated += 1,
                (StatementRowStatus::PossibleDuplicate, _) => report.possible_duplicates += 1,
                (StatementRowStatus::MatchedTransfer, _) => report.matched_transfers += 1,
                _ => report.skipped += 1,
            }
        }
        // Oldest first, so the log reads in the order things happened.
        new.sort_by_key(|(row, _)| (row.occurred_at, row.line));
        for (row, command) in new {
            let envelope = CommandEnvelope {
                id: row.command_id,
                vault_id,
                author: author.to_string(),
                command,
            };
            match self.execute(envelope) {
                Ok(receipt) if receipt.deduplicated => report.deduplicated += 1,
                Ok(_) => {
                    report.executed += 1;
                    report.rounded += u32::from(row.rounded);
                }
                Err(DomainError::Storage(message)) => return Err(DomainError::Storage(message)),
                Err(err) => report.rejected.push(StatementRejection {
                    line: row.line,
                    code: err.code().to_string(),
                    message: err.to_string(),
                }),
            }
        }
        Ok(report)
    }
}
