//! Importing a bank or card statement from CSV.
//!
//! The app reads the file, [`detect`] guesses the delimiter and a built-in
//! preset from the header, the user adjusts a [`StatementMapping`], and
//! [`Core::preview_statement`] shows what would happen to each row with the
//! user's [`StatementRowOverride`]s; [`Core::import_statement`], fed the same
//! overrides, does exactly that.
//!
//! Re-importing the same statement is a no-op: every row's command id is a
//! UUID v5 over a frozen namespace and the row's content (status left out, so
//! a row that went from pending to cleared is still the same row), and what
//! the import decided about a row it left out, which transaction stands for
//! it, is in the log too (see "The import").
//!
//! # What a row is taken for
//!
//! The action is the first of: the user's ([`StatementRowOverride::action`]);
//! the first description rule whose `contains` the description holds
//! ([`StatementMapping::description_rules`]: case-insensitive, inner runs of
//! spaces collapsed on both sides, a blank `contains` is no rule, and a rule
//! without an action leaves the action to what follows); the type rule whose
//! value equals the type column (case-insensitive); the default action.
//! [`StatementRow::action`] is the one that won, [`StatementRow::kind`] and
//! [`StatementRow::counter_wallet_id`] follow it; [`StatementRow::base_action`]
//! is the one before the user's, on every row read, imported before or not,
//! so the app never has to work it out. `BySign` is an expense when
//! the money goes out and an income when it comes in. A sign that contradicts
//! the action (a refund written as money out) keeps the action with the
//! absolute amount: the action says what the row is, and card exports write
//! top-ups with the sign of a spend.
//!
//! The note is the user's, else the matching rule's, else the payee; the
//! category is the user's (a blank one is Uncategorized), else the rule's,
//! else the bank's matched one. [`StatementRow::note`] and
//! [`StatementRow::category`] are what the row gets before the user's own.
//! The envelope of an entry is the user's ([`StatementRowOverride::flow_id`],
//! the vault's Unallocated id for Unallocated) when it is an active envelope
//! of the vault, else [`StatementOptions::flow_id`]: an archived or unknown
//! one is ignored, the file's envelope stays.
//!
//! A transfer whose other wallet is not named, is not the vault's, is
//! archived or is the imported wallet itself waits for one. A type or
//! description rule naming a wallet not in the vault, or the imported one, is
//! refused for the whole statement, as is a rule's category that cannot be a
//! category's name.
//!
//! Amounts are read exactly from the text. More decimals than the currency
//! keeps are rounded half away from zero; [`StatementRow::rounded`] is set
//! only when that changed the value (`12.500` is not rounded). The original
//! amount, when the row has one in another currency or with another value,
//! is appended to the note in parentheses, after the payee, the rule's or the
//! user's note.
//!
//! A bank category never creates one: it only selects an active category
//! whose name or alias it equals, with or without a `5462 - ` merchant code in
//! front. A category typed in a description rule or a [`StatementRowOverride`]
//! goes through the normal resolution, so it may create one.
//!
//! # What becomes of a row
//!
//! The first reason that applies wins:
//!
//! 1. a row with fewer columns than the header, or more that are not empty,
//!    or that swallowed the rest of the file in an unclosed quote: `Invalid`
//!    (`invalid_row`);
//! 2. its id, or the id of the move, the claim or the replacement an earlier
//!    import wrote for it, already in the vault's log: `AlreadyImported`,
//!    whatever the rules or the user now say about it;
//! 3. a `Skip` action: `Skipped`, `skipped_by_you` when it is the user's,
//!    `skipped_by_rule` when it is a rule's or the default action; then a
//!    status in `skip_statuses` (`skipped_status`);
//! 4. an unreadable date: `Invalid` (`invalid_date`); then a day before
//!    [`StatementOptions::since`], in the row's own offset: `Skipped`
//!    (`before_start`), whatever else is wrong with the row; then a currency
//!    other than the vault's (`currency_mismatch`) or an unreadable amount
//!    (`invalid_amount`): `Invalid`;
//! 5. a zero amount: `Skipped` (`zero_amount`);
//! 6. the other side of a transfer already in the vault: `MatchedTransfer`;
//! 7. a transfer without a usable other wallet: `Skipped` (`needs_wallet`);
//! 8. an entry of the vault that looks like it, or that stands for the
//!    transfer it is: `PossibleDuplicate`;
//! 9. otherwise `New`.
//!
//! The user's `skip` leaves any row of 3 to 9 out (`skipped_by_you`), a
//! matched one too, and nothing is written or claimed for it.
//!
//! The user's override changes nothing on a row of 1 or 2, nor on a row the
//! mapping alone reads as invalid: only a row the mapping could import is the
//! user's to change.
//!
//! # Rows already in the vault
//!
//! Every preview matches the rows against the vault again; what earlier
//! imports decided is read back from their claims, moves and replacements in
//! the log (see "The import"). Days apart are counted between calendar
//! dates, each in its own offset. A transfer between the imported wallet and
//! one also checked dated before [`StatementOptions::since`] (the bank funded
//! from Cash the day before the start) is never a match, nor an entry of a
//! wallet also checked dated more than 3 days before it (a pizza typed on the
//! 30th of August may be booked by the bank on the 1st of September); the
//! imported wallet's own rows and the other banks' sides of a transfer are,
//! as a bank's start day is not the others'.
//!
//! - The other side of a transfer: a row that would be new or waits for its
//!   wallet (6 and 7), taken for a transfer or by its sign, matches a live
//!   transfer between wallets of the same amount at most 4 days away that
//!   goes the row's way: out of the imported wallet for money going out, into
//!   it for money coming in, and to or from the wallet the row names, if it
//!   names one. Only the imported wallet's transfers count. An expense,
//!   income or refund action never matches a transfer. The matched row reads
//!   as the transfer it is: kind `TransferWallet`, the other wallet in
//!   [`StatementRow::counter_wallet_id`].
//! - A row typed by hand: a new income, expense or refund matches a live
//!   entry of the same amount moving the money the same way (an expense for
//!   money going out, an income or a refund for money coming in) at most 3
//!   days away, in the imported wallet or in one of
//!   [`StatementOptions::also_check_wallets`].
//! - An entry standing for a transfer: a new transfer between the imported
//!   wallet and the one it names matches, at most 4 days away and of the
//!   same amount, an entry in the wallet it names moving the money the other
//!   way (there an income or a refund for money going out to it, an expense
//!   for money coming in from it: the other bank's row imported without a
//!   rule), or one in the imported wallet or a wallet also checked moving it
//!   the row's way (the bonifico typed by hand as an expense or an income).
//!
//! Whose the match is decides whether it may stand for the row:
//!
//! - in the imported wallet, an entry typed by hand or recorded from a
//!   recurring template that no claim or move names always may (one typed on
//!   the 30th may be booked on the 1st), as may a transfer whose side there
//!   no claim names;
//! - one a claim or a move names, or an entry an import wrote under its own
//!   command's id (a UUID v5: a row of a statement, or of a script), stands
//!   for a row of an export already in: it may only when it is dated within
//!   the file's own days, from its first readable row on or after the start
//!   day to its last, as it is when the same file is read with another
//!   mapping; and within the match's days (3, or 4 for a transfer) of the
//!   first or the last of them, where it is more likely the next or the
//!   previous export's neighbour (next month's coffee, a payment of the same
//!   amount as a transfer the day before), the row is imported by default;
//! - in a wallet also checked, an entry a claim or a move names stands for a
//!   row of another statement and never matches; one an import wrote does
//!   (the user's own sheet, imported there once);
//! - in the wallet a transfer row names, any live entry may (see below for
//!   when it is replaced by default).
//!
//! Opening balances, voided rows and the transactions that stand for the rows
//! of this statement already imported (their own, or the one their claim,
//! move or replacement names) never match: a row is never its own duplicate,
//! nor are the rows next to it in an overlapping export. Each amount is
//! paired in one pass for every kind of match, one to one: as many rows as
//! can be get a transaction, and among the ways to pair that many, the one
//! closest in all, fewest days apart summed, then fewest seconds; on a tie a
//! transfer, then an entry of the row's kind, then an entry standing for a
//! transfer. So a row typed on the same day wins over a transfer three days
//! away, and rows on the 10th and the 13th both pair with rows typed on the
//! 8th and the 11th. The match is in [`StatementRow::existing`].
//!
//! What becomes of a flagged row is [`StatementRow::match_choice`]: the
//! user's [`StatementRowOverride::on_match`], else the default, which
//! [`StatementRow::default_choice`] gives on every matched row whatever the
//! user chose: `ImportAnyway` for a match near the file's edges that stands
//! for another export's row (see above), `MoveExisting` for a duplicate
//! found in a wallet also checked, `Skip` for any other duplicate or
//! transfer. `Skip` says the transaction already there stands for the row,
//! and the import records it. On a duplicate found in a wallet also checked
//! it reads as `MoveExisting`: kept there, the entry would leave the bank
//! short and be claimed for good, so only moving it or importing the row
//! beside it make sense. `MoveExisting` reads as `Skip` on any other
//! duplicate or transfer; `ReplaceExisting` reads as the default but for an
//! entry standing for the transfer. `ImportAnyway` gives the row back what
//! it was (new with its command, or waiting for its wallet), with its match
//! still in `existing`; the transaction stays taken in this preview, so no
//! other row of it gets it, and nothing records it. The user's `skip` is not
//! a choice about the match: the row is left out (`skipped_by_you`), its
//! match still shown and `match_choice` still saying what including it
//! would do, and nothing is written or claimed, so a later import finds the
//! same match again.
//!
//! An entry standing for the transfer is replaced by default only when it
//! very likely is its other side: dated the row's day or a day off, and in
//! no wallet also checked, nor in a named wallet that is one (Cash holds the
//! user's own sheet, where an ATM withdrawal is not the dinner a friend paid
//! back two days on). Otherwise, and near the file's edges, the row is
//! imported beside it by default, its match still shown, for the user to
//! replace it. Keeping the entry in the transfer's place makes no sense, so
//! `Skip` and `MoveExisting` read as `ImportAnyway` there. A row replacing
//! its match stays `PossibleDuplicate`, and is written.
//!
//! [`StatementPreview::lookalikes`] is advice, and matching never reads it:
//! for every wallet neither imported into, nor checked, nor any statement's
//! (no claim or move names it), the new rows that matched nothing and would
//! match one of its entries as a duplicate in a wallet checked would. Twelve
//! rows looking like entries of Cash say Cash should be checked: forgotten,
//! it makes every one of them a second movement, which importing again does
//! not undo.
//!
//! # The import
//!
//! [`Core::import_statement`] runs one command per new row, oldest first,
//! with the row's id. A new transfer between wallets is claimed on the
//! imported wallet's side right after it. A row replacing its match is
//! written in the same place, in one batch, all of it or nothing on this
//! device: its transfer under its own id, the `VoidTransaction` of the entry
//! (`replace`), the claim of its own side, and, when the entry stood for the
//! named wallet's statement row, the claim of that wallet's side under the
//! entry's own claim id (the other row's id when its import wrote the
//! entry): the transfer stands for both rows now, and a preview of the other
//! statement follows the entry its row names to the transfer. Then, oldest
//! first, every flagged row left out is recorded. Each of these commands has
//! an id derived from the row's (or the entry's), so writing it twice is a
//! no-op and the row reads as imported from then on:
//!
//! - a duplicate whose choice is `MoveExisting` is moved into the imported
//!   wallet (`move-existing`): an `UpdateTransaction` of its `wallet_id`
//!   alone, so its kind, note, category, envelope, date and person stay;
//! - a duplicate whose choice is `Skip` is claimed (`claim`): an
//!   `UpdateTransaction` naming the `wallet_id` it already has;
//! - the other side of a transfer whose choice is `Skip` is claimed
//!   (`claim`): an `UpdateTransaction` naming the imported wallet as the
//!   `from_id` or the `to_id` it already is.
//!
//! A claim changes nothing in the vault; it says, in the log, that the
//! transaction stands for a row of a statement. A claim or a move is told
//! from any other command by its shape, which is reserved to the import (see
//! [`crate::TransactionPatch`]): an `UpdateTransaction` under a UUID v5 whose
//! patch carries one of `wallet_id`, `from_id` and `to_id` and nothing else.
//! The app mints v7 ids, and the script that relabels rows under v5 ids
//! patches their category only. A claim names the whole entry, or the side
//! of the transfer on the wallet it names. Once both banks of a transfer are
//! imported, both its sides are claimed.
//!
//! Refused rows, replacements, claims and moves land in the report on their
//! line (a claim is refused, like any update, on an entry whose envelope or
//! wallet is archived, or a transfer touching an archived wallet; a refused
//! replacement writes nothing at all); a storage error stops the import
//! (every command before it stays, and a new run deduplicates them). A
//! replacing row counts among the possible duplicates, and its transfer
//! among the rows executed and, rounded, among the rows rounded, in the
//! preview as in the report; claims are counted nowhere else, moves in
//! [`StatementReport::moved`].
//!
//! # Known limits
//!
//! - Matching sees only what this device holds. A device that has not
//!   pulled what another imported may plan the same rows otherwise, and the
//!   server takes a command whose id it knows without comparing what it says.
//!   A claim, a no-op when written, replayed after another device changed the
//!   entry's wallet, puts the wallet back. A move or a claim the server
//!   refused stays in the log as refused and still counts on this device: the
//!   row reads as imported there, and the transaction as claimed. What to
//!   do: sync before previewing and importing (the app does); dismissing a
//!   refused command makes its row importable again.
//! - The import plans the rows again rather than trusting the preview: if
//!   the vault changed in between, it may pair, move, replace or claim
//!   otherwise than the preview showed. What to do: preview again after a
//!   sync or an edit.
//! - Never check a wallet that is itself a statement's for rows typed by
//!   hand: the rows its own statement imported would be moved out of it.
//!   What to do: check only wallets typed into by hand, Cash for one.
//! - Forgetting to check a wallet typed into by hand is not undone by
//!   importing again: its rows the statement also holds are in twice, and a
//!   new import finds the statement's own rows imported. What to do: void
//!   the hand-typed twins by hand. [`StatementPreview::lookalikes`] names
//!   such a wallet before the import, to prevent exactly this.
//! - The app remembers a mapping and its options by the file's header, so
//!   two banks whose exports share a header share them too. What to do:
//!   check the wallet and the wallets also checked before each import.
//! - The shape of a claim is the import's alone: a bulk script writing
//!   `UpdateTransaction` under v5 ids must not patch `wallet_id`, `from_id`
//!   or `to_id` alone.
//! - Keeping a match (`Skip`) is not undone by importing again: the row
//!   reads as imported from then on. What to do: type the row by hand. Leaving
//!   a row out (`skip`) records nothing, and the next import shows it again.
//! - The server applies a replacement one command at a time, as it does
//!   every batch: if another device archives either wallet first, it may
//!   refuse the transfer and accept the void. What to do: type the movement
//!   again.
//! - Replacing an income that was already shared out into envelopes takes
//!   it out of Unallocated, which can go below zero. What to do: move the
//!   money back, or decide the allocation again.
//! - A transfer row replaces one entry only: a bonifico typed by hand into
//!   one bank and imported as an income into the other leaves one of the
//!   two. What to do: void it by hand.
//! - A short file read again with another mapping has most of its rows near
//!   its edges, and these are imported again by default, their match shown.
//!   What to do: choose `Skip` on them, or keep the mapping of a file
//!   already imported.
//!
//! # Ids
//!
//! The key of a row is `v1|vault|wallet|date|type|amount|payee|n`: the UTC
//! instant (`2026-09-16T08:54:40Z`) when the date column fixes one, else the
//! wall-clock time or the day as written, so importing from another timezone
//! finds the same rows; the type column's value, or without a type column the
//! kind the type rules and the default action give the row, never the one a
//! description rule or the user give it (ids handed out before either existed
//! stay what they were, and changing what a row is never makes it new again);
//! the amount in minor units signed as the file wrote it; the payee
//! lowercased; and `n` the number of rows with the same key earlier in the
//! file, so two identical rows in the same second both import. Nor does the
//! start day, the envelope or a match ever change an id. Line numbers are
//! those of the file, header included; a row with a newline inside quotes
//! starts on its first.

use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    Command, CommandEnvelope, Core, DomainError, Result, TransactionKind, TransactionView,
};

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
    /// default only when the entry very likely is that side: dated the row's
    /// day or a day off, and in no wallet also checked. Only for such a
    /// match; anywhere else it reads as the default.
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
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum StatementRowStatus {
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
    /// What [`Self::match_choice`] would be without the user's `on_match`.
    /// `None` when nothing matched.
    #[uniffi(default = None)]
    pub default_choice: Option<StatementMatchChoice>,
    /// What the row is taken for before the user's own action: the
    /// description rule's, else the type rule's, else the default action.
    /// `None` for a row that could not be read as one (`invalid_row`).
    #[uniffi(default = None)]
    pub base_action: Option<StatementAction>,
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
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
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
    /// Wallets not checked for rows typed by hand, nor any statement's,
    /// where new rows that matched nothing look like entries already there,
    /// the most such rows first. Advice only: matching does not use them.
    #[uniffi(default = [])]
    pub lookalikes: Vec<StatementLookalike>,
}

/// Outcome of [`Core::import_statement`].
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
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
            new_rows: 0,
            already_imported: 0,
            skipped: 0,
            invalid: 0,
            rounded: 0,
            possible_duplicates: 0,
            matched_transfers: 0,
            moves: 0,
            lookalikes,
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
        let wallet = options.wallet_id;
        let mut new = Vec::new();
        let mut later = Vec::new();
        for plan::Planned {
            row,
            command,
            replaces_other_row,
        } in planned
        {
            match (&row.status, command) {
                (StatementRowStatus::New, Some(command)) => new.push((row, command, false)),
                // Its transfer replaces the entry it matched.
                (StatementRowStatus::PossibleDuplicate, Some(command)) => {
                    report.possible_duplicates += 1;
                    new.push((row, command, replaces_other_row));
                }
                (StatementRowStatus::AlreadyImported, _) => report.deduplicated += 1,
                (
                    StatementRowStatus::PossibleDuplicate | StatementRowStatus::MatchedTransfer,
                    _,
                ) => {
                    if row.status == StatementRowStatus::PossibleDuplicate {
                        report.possible_duplicates += 1;
                    } else {
                        report.matched_transfers += 1;
                    }
                    if let Some(afterwards) = plan::afterwards(&row, wallet) {
                        later.push((row, afterwards));
                    }
                }
                _ => report.skipped += 1,
            }
        }
        // Oldest first, so the log reads in the order things happened; the
        // rows typed by hand move, and the matches are claimed, once the new
        // rows are in.
        new.sort_by_key(|(row, ..)| (row.occurred_at, row.line));
        later.sort_by_key(|(row, _)| (row.occurred_at, row.line));
        for (row, command, other_row) in new {
            if row.status == StatementRowStatus::PossibleDuplicate {
                // A replacement is written whole or not at all: the
                // transfer, the void of the entry and their claims.
                let batch = plan::replacing(&row, command, wallet, other_row)
                    .into_iter()
                    .map(|(id, command)| self::envelope(id, vault_id, author, command))
                    .collect();
                match self.execute_batch(batch) {
                    Ok(receipts) if receipts.first().is_some_and(|r| r.deduplicated) => {
                        report.deduplicated += 1;
                    }
                    Ok(_) => {
                        report.executed += 1;
                        report.rounded += u32::from(row.rounded);
                    }
                    Err(DomainError::Storage(message)) => {
                        return Err(DomainError::Storage(message));
                    }
                    Err(err) => report.rejected.push(rejection(&row, &err)),
                }
                continue;
            }
            let then = plan::own_side(row.command_id, &command, wallet);
            match self.execute(self::envelope(row.command_id, vault_id, author, command)) {
                Ok(receipt) if receipt.deduplicated => report.deduplicated += 1,
                Ok(_) => {
                    report.executed += 1;
                    report.rounded += u32::from(row.rounded);
                }
                Err(DomainError::Storage(message)) => return Err(DomainError::Storage(message)),
                Err(err) => {
                    report.rejected.push(rejection(&row, &err));
                    continue;
                }
            }
            // A transfer is claimed on this wallet's side as soon as it is in.
            if let Some((id, command)) = then {
                match self.execute(self::envelope(id, vault_id, author, command)) {
                    Ok(_) => {}
                    Err(DomainError::Storage(message)) => {
                        return Err(DomainError::Storage(message));
                    }
                    Err(err) => report.rejected.push(rejection(&row, &err)),
                }
            }
        }
        for (row, (afterwards, id, command)) in later {
            match self.execute(self::envelope(id, vault_id, author, command)) {
                Ok(receipt) => {
                    if let plan::Afterwards::Move = afterwards {
                        if receipt.deduplicated {
                            report.deduplicated += 1;
                        } else {
                            report.moved += 1;
                        }
                    }
                }
                Err(DomainError::Storage(message)) => return Err(DomainError::Storage(message)),
                Err(err) => report.rejected.push(rejection(&row, &err)),
            }
        }
        Ok(report)
    }
}

fn envelope(id: Uuid, vault_id: Uuid, author: &str, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        id,
        vault_id,
        author: author.to_string(),
        command,
    }
}

/// `err`, refusing a command written for `row`.
fn rejection(row: &StatementRow, err: &DomainError) -> StatementRejection {
    StatementRejection {
        line: row.line,
        code: err.code().to_string(),
        message: err.to_string(),
    }
}
