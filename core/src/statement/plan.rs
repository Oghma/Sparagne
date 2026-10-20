//! From a statement and a mapping to one planned row per record: what the row
//! is, the id its command gets and, when it is new, the command itself.
//!
//! Every row is read on its own first ([`Context::read`]); a second pass over
//! the file numbers the rows with the same key, turns keys into ids, looks
//! the ids up in the log and decides each row with the user's override
//! ([`Context::decide`]). The rows are then matched against what the vault
//! already holds ([`matching::claim`]), and last the user's `skip` leaves out
//! the rows still new.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use chrono_tz::Tz;
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use super::amount::{Amount, parse_amount};
use super::csv::{self, Record};
use super::dates::{StatementDate, parse_date};
use super::matching;
use super::{
    AmountSign, STATEMENT_NAMESPACE, StatementAction, StatementDateFormat,
    StatementDescriptionRule, StatementLookalike, StatementMapping, StatementMatchChoice,
    StatementOptions, StatementRow, StatementRowOverride, StatementRowStatus,
};
use crate::{
    Command, Core, Currency, DomainError, Entry, Result, TransactionKind, TransactionPatch,
    normalize_category_key,
};

/// Version of the key format, first in every key: changing the format means
/// a new prefix, never a silent change of the ids of rows already imported.
const KEY_VERSION: &str = "v1";

/// A row of the statement with what happens to it.
pub(super) struct Planned {
    pub row: StatementRow,
    /// The command of a [`StatementRowStatus::New`] row, or of one replacing
    /// its match.
    pub command: Option<Command>,
    /// The entry the row would replace is the statement row of the wallet
    /// the row names (its import wrote it, or its claim or move names it):
    /// replacing it claims that wallet's side of the transfer for that row.
    pub replaces_other_row: bool,
    /// The money-out half of a move between two accounts of the file, which
    /// writes the transfer: the id of the money-in half, whose side the
    /// import claims for it.
    pub paired_in: Option<Uuid>,
}

impl Planned {
    pub(super) fn new(row: StatementRow, command: Option<Command>) -> Self {
        Self {
            row,
            command,
            replaces_other_row: false,
            paired_in: None,
        }
    }
}

/// The user's overrides by line; the last one of a line wins.
pub(super) type Overrides<'o> = HashMap<u32, &'o StatementRowOverride>;

/// Every row of a statement with what happens to it, and the wallets worth
/// checking for rows typed by hand.
pub(super) struct Plan {
    pub rows: Vec<Planned>,
    pub lookalikes: Vec<StatementLookalike>,
}

/// Reads every row of `text` and decides what becomes of it, with the user's
/// `overrides`. Writes nothing.
pub(super) fn plan(
    core: &Core,
    vault_id: Uuid,
    text: &str,
    mapping: &StatementMapping,
    options: &StatementOptions,
    overrides: &[StatementRowOverride],
) -> Result<Plan> {
    let delimiter = delimiter(&mapping.delimiter)?;
    let mut records = csv::records(text, delimiter);
    let context = Context::new(core, vault_id, &mut records, mapping, options)?;
    let overrides: Overrides<'_> = overrides.iter().map(|o| (o.line, o)).collect();

    // The row's own command, or the move, the claim or the replacement an
    // earlier import made for it, whatever became of it: a command refused
    // by the server stays in the log, and executing it again would be a
    // no-op.
    let mut known = core
        .conn
        .prepare_cached("SELECT id, payload FROM commands WHERE id IN (?1, ?2, ?3, ?4)")?;
    // The claim made under a transaction's own id: a transfer that took the
    // place of an entry, claimed on the side of the row the entry stood for.
    let mut taken_over = core
        .conn
        .prepare_cached("SELECT payload FROM commands WHERE id = ?1")?;
    let mut occurrences: HashMap<String, u32> = HashMap::new();
    let mut planned = Vec::new();
    // The transactions that stand for the rows already imported: never a
    // match for another row.
    let mut standing = HashSet::new();
    for record in records {
        let draft = context.read(&record);
        let mut id = Uuid::nil();
        let mut imported = false;
        if let Some(key) = &draft.key {
            let seen = occurrences.entry(key.clone()).or_insert(0);
            id = row_id(&format!("{key}|{seen}"));
            *seen += 1;
            let ids = params![id, move_id(id), claim_id(id), replace_id(id)];
            let found =
                known.query_map(ids, |r| Ok((r.get::<_, Uuid>(0)?, r.get::<_, String>(1)?)))?;
            let mut named = Vec::new();
            for command in found {
                let (command_id, payload) = command?;
                imported = true;
                named.push(if command_id == id {
                    id
                } else {
                    named_transaction(&payload).unwrap_or(id)
                });
            }
            for transaction in named {
                standing.insert(transaction);
                let successor = taken_over
                    .query_row(params![claim_id(transaction)], |r| r.get::<_, String>(0))
                    .optional()?;
                if let Some(successor) = successor.as_deref().and_then(named_transaction) {
                    standing.insert(successor);
                }
            }
        }
        // A row in the log stays imported whatever the rules or the user now
        // say, so its override changes nothing.
        let user = if imported {
            None
        } else {
            overrides.get(&record.line).copied()
        };
        let mut item = context.decide(draft, user);
        item.row.command_id = id;
        if imported {
            item.row.status = StatementRowStatus::AlreadyImported;
            item.command = None;
        }
        planned.push(item);
    }
    // The file's own moves between accounts first: their halves match
    // nothing in the vault.
    matching::pair_legs(&mut planned, &overrides);
    let lookalikes = matching::claim(core, vault_id, options, &overrides, &standing, &mut planned)?;
    leave_out(&mut planned, &overrides);
    Ok(Plan {
        rows: planned,
        lookalikes,
    })
}

/// The id of the command that moves the row typed by hand for the
/// statement row `row_id` into the statement's wallet: derived from the
/// row's, so moving it again is a no-op and, once it is in the log, the row
/// reads as imported.
pub(super) fn move_id(row_id: Uuid) -> Uuid {
    Uuid::new_v5(&row_id, b"move-existing")
}

/// The id of the command that claims, for the statement row `row_id`, the
/// transaction that stands for it: the match the user left in place, or
/// the statement wallet's side of a transfer. Derived from the row's, like
/// [`move_id`].
pub(super) fn claim_id(row_id: Uuid) -> Uuid {
    Uuid::new_v5(&row_id, b"claim")
}

/// The move ([`move_id`]) or the claim an import writes for a flagged row,
/// once the new rows are in: a duplicate to move goes to `wallet`; one left
/// in place, or the other side of a transfer, is claimed with an
/// `UpdateTransaction` naming the wallet (or the transfer's side) it already
/// has, which changes nothing but says, in the log, which row it stands for.
/// Nothing for a row imported anyway.
pub(super) fn afterwards(row: &StatementRow, wallet: Uuid) -> Option<(Uuid, Command)> {
    let existing = row.existing.as_ref()?;
    let entry = |wallet_id| TransactionPatch {
        wallet_id: Some(wallet_id),
        ..TransactionPatch::default()
    };
    let (id, patch) = match (&row.status, row.match_choice?) {
        (StatementRowStatus::PossibleDuplicate, StatementMatchChoice::MoveExisting) => {
            (move_id(row.command_id), entry(wallet))
        }
        (StatementRowStatus::PossibleDuplicate, StatementMatchChoice::Skip) => {
            (claim_id(row.command_id), entry(existing.wallet_id?))
        }
        (StatementRowStatus::MatchedTransfer, StatementMatchChoice::Skip) => (
            claim_id(row.command_id),
            side(existing.from_id == Some(wallet), wallet),
        ),
        _ => return None,
    };
    Some((id, update(existing.id, patch)))
}

/// The claim of the statement wallet's side of the transfer a new row
/// creates (the transfer's id is the row's), with its id.
pub(super) fn own_side(row_id: Uuid, command: &Command, wallet: Uuid) -> Option<(Uuid, Command)> {
    let Command::TransferWallet { from_wallet_id, .. } = command else {
        return None;
    };
    Some((
        claim_id(row_id),
        update(row_id, side(*from_wallet_id == wallet, wallet)),
    ))
}

/// What an import writes, all at once, for the money-out half of a move
/// between two accounts of the file (`row_id`, its transfer `command`): the
/// transfer under the half's id, and the claims of both its sides, the
/// money-in half's (`in_row`) under that half's claim id, so it reads as
/// imported from then on too.
pub(super) fn paired(row_id: Uuid, command: Command, in_row: Uuid) -> Vec<(Uuid, Command)> {
    let Command::TransferWallet {
        from_wallet_id,
        to_wallet_id,
        ..
    } = command
    else {
        return vec![(row_id, command)];
    };
    vec![
        (row_id, command),
        (claim_id(row_id), update(row_id, side(true, from_wallet_id))),
        (claim_id(in_row), update(row_id, side(false, to_wallet_id))),
    ]
}

/// The id of the command that voids, for the statement row `row_id`, the
/// entry that stood for the transfer the row is. Derived from the row's,
/// like [`move_id`].
pub(super) fn replace_id(row_id: Uuid) -> Uuid {
    Uuid::new_v5(&row_id, b"replace")
}

/// What an import writes, all at once, for a row replacing its match: the
/// row's transfer under the row's id, the void of the entry, the claim of
/// the statement wallet's side and, when the entry stood for the named
/// wallet's statement row (`other_row`), the claim of that wallet's side
/// under the entry's own claim id, which a later preview of that statement
/// follows from the row to the transfer. With their ids, in that order.
pub(super) fn replacing(
    row: &StatementRow,
    command: Command,
    wallet: Uuid,
    other_row: bool,
) -> Vec<(Uuid, Command)> {
    let Some(existing) = row.existing.as_ref() else {
        return vec![(row.command_id, command)];
    };
    let mut batch = Vec::new();
    batch.extend(own_side(row.command_id, &command, wallet));
    if other_row
        && let Command::TransferWallet {
            from_wallet_id,
            to_wallet_id,
            ..
        } = &command
    {
        let other = if *from_wallet_id == wallet {
            *to_wallet_id
        } else {
            *from_wallet_id
        };
        batch.push((
            claim_id(existing.id),
            update(row.command_id, side(*from_wallet_id == other, other)),
        ));
    }
    let void = Command::VoidTransaction {
        transaction_id: existing.id,
    };
    batch.insert(0, (replace_id(row.command_id), void));
    batch.insert(0, (row.command_id, command));
    batch
}

/// A patch naming `wallet` as the source (`from`) or the destination of a
/// transfer.
fn side(from: bool, wallet: Uuid) -> TransactionPatch {
    if from {
        TransactionPatch {
            from_id: Some(wallet),
            ..TransactionPatch::default()
        }
    } else {
        TransactionPatch {
            to_id: Some(wallet),
            ..TransactionPatch::default()
        }
    }
}

fn update(transaction_id: Uuid, patch: TransactionPatch) -> Command {
    Command::UpdateTransaction {
        transaction_id,
        patch,
    }
}

/// The transaction a move, a claim or a replacement (as the log stores it)
/// names.
fn named_transaction(payload: &str) -> Option<Uuid> {
    match serde_json::from_str(payload).ok()? {
        Command::UpdateTransaction { transaction_id, .. }
        | Command::VoidTransaction { transaction_id } => Some(transaction_id),
        _ => None,
    }
}

/// The user's `skip` on the rows still new.
fn leave_out(planned: &mut [Planned], overrides: &Overrides<'_>) {
    for item in planned {
        if item.row.status == StatementRowStatus::New
            && overrides.get(&item.row.line).is_some_and(|user| user.skip)
        {
            item.row.status = skipped_by_you();
            item.command = None;
        }
    }
}

pub(super) fn skipped(code: &str, reason: String) -> StatementRowStatus {
    StatementRowStatus::Skipped {
        code: code.to_string(),
        reason,
    }
}

fn invalid(code: &str, message: String) -> StatementRowStatus {
    StatementRowStatus::Invalid {
        code: code.to_string(),
        message,
    }
}

pub(super) fn skipped_by_you() -> StatementRowStatus {
    skipped("skipped_by_you", "skipped by you".to_string())
}

/// The id of the row whose full key (occurrence index included) is `key`.
fn row_id(key: &str) -> Uuid {
    Uuid::new_v5(&STATEMENT_NAMESPACE, key.as_bytes())
}

/// The one-character delimiter of a mapping. `\t` and `tab` spelled out are
/// read as a tab.
pub(super) fn delimiter(value: &str) -> Result<char> {
    let bad = || {
        DomainError::InvalidCommand(format!(
            "the delimiter must be one character, not '{value}'"
        ))
    };
    if matches!(value, "\\t" | "tab" | "TAB") {
        return Ok('\t');
    }
    let mut chars = value.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return Err(bad());
    };
    if matches!(c, '"' | '\r' | '\n') {
        return Err(bad());
    }
    Ok(c)
}

/// A row read on its own, before the file-wide pass gives it an id and
/// [`Context::decide`] says what becomes of it.
struct Draft<'a> {
    row: StatementRow,
    /// The key without the occurrence index; `None` when the date or the
    /// amount could not be read, and the row then keeps the nil id.
    key: Option<String>,
    /// Why the record is no row at all (`invalid_row`).
    broken: Option<String>,
    /// What the type rules and the default action make of the row.
    base: &'a StatementAction,
    /// The description rule that matched, by position in [`Context::rules`].
    rule: Option<usize>,
    /// The type column's value, for the reason of a skip.
    type_value: Option<String>,
    /// Whether the money goes out; `None` when the amount is unreadable.
    outflow: Option<bool>,
    /// The row's status, when `skip_statuses` holds it.
    skipped_status: Option<String>,
    /// The first reason the row cannot be imported, whatever it is taken
    /// for.
    problem: Option<StatementRowStatus>,
}

/// Who said what a row is, for the reason of a skip.
enum By<'r> {
    User,
    /// A description rule, with its text.
    Rule(&'r str),
    /// The type rules or the default action, with the type column's value.
    Mapping(Option<&'r str>),
}

impl By<'_> {
    fn skip(&self) -> StatementRowStatus {
        let reason = match self {
            Self::User => return skipped_by_you(),
            Self::Rule(text) => format!("description contains \"{text}\" is skipped"),
            Self::Mapping(None) => "skipped by the mapping".to_string(),
            Self::Mapping(Some(value)) => format!("type {value} is skipped"),
        };
        skipped("skipped_by_rule", reason)
    }
}

/// A [`StatementDescriptionRule`] ready to be matched. A rule whose
/// `contains` is blank is none, so a rule still being typed never fails a
/// preview.
struct Rule<'a> {
    /// Position in [`StatementMapping::description_rules`].
    index: u32,
    /// `contains` lowercased, inner runs of spaces collapsed: what the
    /// lowercased payee must hold.
    needle: String,
    /// `contains` trimmed, inner runs of spaces collapsed, for the reason of
    /// a skip.
    text: String,
    action: Option<&'a StatementAction>,
    /// Non-blank, trimmed.
    category: Option<String>,
    /// Non-blank, trimmed.
    note: Option<String>,
}

impl<'a> Rule<'a> {
    fn all(rules: &'a [StatementDescriptionRule]) -> Vec<Self> {
        let filled = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        rules
            .iter()
            .enumerate()
            .filter_map(|(index, rule)| {
                let text = collapse(std::iter::once(rule.contains.as_str()));
                (!text.is_empty()).then(|| Rule {
                    index: u32::try_from(index).unwrap_or(u32::MAX),
                    needle: text.to_lowercase(),
                    text,
                    action: rule.action.as_ref(),
                    category: filled(&rule.category),
                    note: filled(&rule.note),
                })
            })
            .collect()
    }
}

/// The indices of the mapped columns in the header.
struct Columns {
    /// Columns in the header; a row with fewer, or with more that are not
    /// empty, is invalid.
    count: usize,
    date: usize,
    amount: usize,
    description: Vec<usize>,
    kind: Option<usize>,
    status: Option<usize>,
    currency: Option<usize>,
    category: Option<usize>,
    original_amount: Option<usize>,
    original_currency: Option<usize>,
    account: Option<usize>,
    transfer: Option<usize>,
}

impl Columns {
    /// Every named column must be in the header (case-insensitive, trimmed).
    /// A blank optional column is no column.
    fn resolve(headers: &[String], mapping: &StatementMapping) -> Result<Self> {
        let find = |name: &str| -> Result<usize> {
            let name = name.trim();
            if name.is_empty() {
                return Err(DomainError::InvalidCommand(
                    "the mapping leaves a required column blank".to_string(),
                ));
            }
            csv::column(headers, name).ok_or_else(|| {
                DomainError::InvalidCommand(format!("column '{name}' is not in the statement"))
            })
        };
        let optional = |name: &Option<String>| -> Result<Option<usize>> {
            match name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                Some(name) => find(name).map(Some),
                None => Ok(None),
            }
        };
        Ok(Self {
            count: headers.len(),
            date: find(&mapping.date_column)?,
            amount: find(&mapping.amount_column)?,
            description: mapping
                .description_columns
                .iter()
                .filter(|c| !c.trim().is_empty())
                .map(|c| find(c))
                .collect::<Result<_>>()?,
            kind: optional(&mapping.type_column)?,
            status: optional(&mapping.status_column)?,
            currency: optional(&mapping.currency_column)?,
            category: optional(&mapping.category_column)?,
            original_amount: optional(&mapping.original_amount_column)?,
            original_currency: optional(&mapping.original_currency_column)?,
            account: optional(&mapping.account_column)?,
            transfer: optional(&mapping.transfer_column)?,
        })
    }
}

/// What every row needs and nothing in the row changes.
struct Context<'a> {
    mapping: &'a StatementMapping,
    columns: Columns,
    vault_id: Uuid,
    wallet_id: Uuid,
    flow_id: Option<Uuid>,
    currency: Currency,
    tz: Tz,
    /// Rows dated before it, in their own offset, are skipped.
    since: Option<NaiveDate>,
    /// Normalized name or alias -> name of an active, non-system category.
    categories: HashMap<String, String>,
    /// Every wallet of the vault, whether it is archived.
    wallets: HashMap<Uuid, bool>,
    /// The envelopes an entry can go to, Unallocated included.
    active_flows: HashSet<Uuid>,
    unallocated: Uuid,
    /// The description rules that are rules, in order.
    rules: Vec<Rule<'a>>,
    /// Account trimmed and lowercased -> its wallet, `None` to leave it out.
    accounts: HashMap<String, Option<Uuid>>,
}

impl<'a> Context<'a> {
    /// Checks the vault, the wallets, the flow, the timezone, the rules and
    /// the mapping against the header, which it takes from `records`.
    fn new(
        core: &Core,
        vault_id: Uuid,
        records: &mut csv::Records<'_>,
        mapping: &'a StatementMapping,
        options: &StatementOptions,
    ) -> Result<Self> {
        let snapshot = core.snapshot(vault_id)?;
        let wallets: HashMap<Uuid, bool> = snapshot
            .wallets
            .iter()
            .map(|w| (w.id, w.archived))
            .collect();
        if !wallets.contains_key(&options.wallet_id) {
            return Err(DomainError::NotFound(format!(
                "wallet {}",
                options.wallet_id
            )));
        }
        if let Some(flow) = options.flow_id
            && !snapshot.flows.iter().any(|f| f.id == flow)
        {
            return Err(DomainError::NotFound(format!("flow {flow}")));
        }
        // An archived one is fine: an entry can be moved out of it.
        if let Some(other) = options
            .also_check_wallets
            .iter()
            .chain(
                options
                    .account_wallets
                    .iter()
                    .filter_map(|a| a.wallet_id.as_ref()),
            )
            .find(|id| !wallets.contains_key(id))
        {
            return Err(DomainError::NotFound(format!("wallet {other}")));
        }
        // The last of an account listed twice wins, as an override's does.
        let accounts = options
            .account_wallets
            .iter()
            .map(|account| (account_key(&account.account), account.wallet_id))
            .collect();
        let rules = Rule::all(&mapping.description_rules);
        for rule in &rules {
            if let Some(category) = &rule.category {
                normalize_category_key(category)?;
            }
        }
        let actions = mapping
            .type_rules
            .iter()
            .map(|rule| &rule.action)
            .chain(rules.iter().filter_map(|rule| rule.action))
            .chain([&mapping.default_action]);
        for other in actions.filter_map(counter_wallet) {
            if !wallets.contains_key(&other) {
                return Err(DomainError::NotFound(format!("wallet {other}")));
            }
            if other == options.wallet_id {
                return Err(DomainError::InvalidCommand(
                    "a transfer rule moves money between the imported wallet and itself"
                        .to_string(),
                ));
            }
        }
        let tz: Tz = options.timezone.parse().map_err(|_| {
            DomainError::InvalidName(format!("unknown timezone '{}'", options.timezone))
        })?;
        if let StatementDateFormat::Custom { pattern } = &mapping.date_format
            && pattern.trim().is_empty()
        {
            return Err(DomainError::InvalidCommand(
                "the custom date pattern is empty".to_string(),
            ));
        }
        Ok(Self {
            columns: Columns::resolve(&csv::header(records)?, mapping)?,
            mapping,
            vault_id,
            wallet_id: options.wallet_id,
            flow_id: options.flow_id,
            currency: snapshot.currency,
            tz,
            since: options.since,
            categories: category_names(core, vault_id)?,
            active_flows: snapshot
                .flows
                .iter()
                .filter(|f| !f.archived)
                .map(|f| f.id)
                .collect(),
            unallocated: snapshot.unallocated_flow_id,
            wallets,
            rules,
            accounts,
        })
    }

    /// The wallet the rows of `account` go to: the one the options give it
    /// (`None` leaves them out), else the options' wallet, as for every row
    /// of a file without an account column.
    fn wallet_of(&self, account: Option<&str>) -> Option<Uuid> {
        match account.and_then(|account| self.accounts.get(&account_key(account))) {
            Some(listed) => *listed,
            None => Some(self.wallet_id),
        }
    }

    /// Everything about one record that holds whatever the user says, but
    /// its id and whether it is in the log.
    fn read(&self, record: &Record) -> Draft<'a> {
        let mut row = StatementRow {
            line: record.line,
            action: self.mapping.default_action.clone(),
            ..StatementRow::default()
        };
        let broken = |row, message: String| Draft {
            row,
            key: None,
            broken: Some(message),
            base: &self.mapping.default_action,
            rule: None,
            type_value: None,
            outflow: None,
            skipped_status: None,
            problem: None,
        };
        if record.unclosed_quote {
            return broken(
                row,
                "a quoted field is never closed, so the rest of the file is in this row"
                    .to_string(),
            );
        }
        // Empty cells past the header are the trailing delimiter some banks
        // end every row with, not a ragged row.
        if !csv::agrees(record, self.columns.count) {
            return broken(
                row,
                format!(
                    "the row has {} columns, the header {}",
                    record.fields.len(),
                    self.columns.count
                ),
            );
        }
        let field = |index: usize| record.fields.get(index).map_or("", String::as_str);
        let optional = |index: Option<usize>| index.map(field).filter(|v| !v.is_empty());

        row.payee = collapse(self.columns.description.iter().map(|&i| field(i)));
        row.account = optional(self.columns.account).map(|account| account.trim().to_string());
        row.wallet_id = self.wallet_of(row.account.as_deref());
        row.transfer_leg = optional(self.columns.transfer).is_some_and(truthy);
        row.bank_category = optional(self.columns.category).map(str::to_string);
        row.matched_category = row
            .bank_category
            .as_deref()
            .and_then(|bank| self.match_category(bank));
        row.note.clone_from(&row.payee);
        row.category.clone_from(&row.matched_category);
        // The first rule the description holds decides on its own.
        let payee = row.payee.to_lowercase();
        let rule = self
            .rules
            .iter()
            .position(|rule| payee.contains(&rule.needle));
        if let Some(found) = rule.map(|at| &self.rules[at]) {
            row.description_rule = Some(found.index);
            if let Some(note) = &found.note {
                row.note.clone_from(note);
            }
            if let Some(category) = &found.category {
                row.category = Some(category.clone());
            }
        }

        let type_value = optional(self.columns.kind);
        let status = optional(self.columns.status);
        let currency = optional(self.columns.currency);
        let date = parse_date(field(self.columns.date), &self.mapping.date_format, self.tz);
        let amount = parse_amount(
            field(self.columns.amount),
            self.mapping.decimal_comma,
            self.currency.minor_units(),
        );
        let base = self.action(type_value);
        let outflow = amount.as_ref().ok().map(|amount| self.outflow(amount));

        row.occurred_at = date.as_ref().ok().map(|date| date.at);
        if let Ok(amount) = &amount {
            row.amount = amount.minor.abs();
            row.rounded = amount.rounded;
        }
        row.outflow = outflow == Some(true);
        row.original = self.original(
            optional(self.columns.original_amount),
            optional(self.columns.original_currency),
            currency,
            amount.as_ref().ok(),
        );
        // The kind the type rules and the default action give: the one the
        // key holds, whatever a description rule or the user make of the row.
        let base_kind = outflow.and_then(|outflow| kind_of(base, outflow));
        let key = match (&date, &amount) {
            (Ok(date), Ok(amount)) => Some(self.key(
                &date.stamp,
                type_value,
                base_kind,
                (&row.payee, row.account.as_deref()),
                amount.minor,
            )),
            _ => None,
        };
        let skipped_status = status
            .filter(|status| {
                self.mapping
                    .skip_statuses
                    .iter()
                    .any(|skipped| same_text(skipped, status))
            })
            .map(str::to_string);
        Draft {
            row,
            key,
            broken: None,
            base,
            rule,
            type_value: type_value.map(str::to_string),
            outflow,
            skipped_status,
            problem: self.problem(currency, date, amount),
        }
    }

    /// Whether the money of `amount` goes out.
    fn outflow(&self, amount: &Amount) -> bool {
        match self.mapping.amount_sign {
            AmountSign::OutflowPositive => amount.minor > 0,
            AmountSign::OutflowNegative => amount.minor < 0,
        }
    }

    /// The first reason a row cannot be imported, whatever it is taken for:
    /// an unreadable date, a day before `since`, another currency, an
    /// unreadable amount, a zero amount. A row before the start is left out
    /// whatever else is wrong with it, so only its date has to be read.
    fn problem(
        &self,
        currency: Option<&str>,
        date: std::result::Result<StatementDate, String>,
        amount: std::result::Result<Amount, String>,
    ) -> Option<StatementRowStatus> {
        let date = match date {
            Ok(date) => date,
            Err(message) => return Some(invalid("invalid_date", message)),
        };
        if let Some(since) = self.since
            && date.at.date_naive() < since
        {
            let reason = format!("dated before {since}, the day the import starts");
            return Some(skipped("before_start", reason));
        }
        if let Some(currency) = currency
            && !currency.eq_ignore_ascii_case(self.currency.code())
        {
            let vault = self.currency.code();
            let message = format!("the row is in {currency}, the vault in {vault}");
            return Some(invalid("currency_mismatch", message));
        }
        match amount {
            Err(message) => Some(invalid("invalid_amount", message)),
            Ok(amount) if amount.minor == 0 => {
                Some(skipped("zero_amount", "the amount is zero".to_string()))
            }
            Ok(_) => None,
        }
    }

    /// What becomes of a row not in the log yet, with the user's override.
    /// A row the mapping reads as invalid stays as it reads it: the override
    /// changes nothing.
    fn decide(&self, draft: Draft<'a>, user: Option<&StatementRowOverride>) -> Planned {
        let Draft {
            mut row,
            broken,
            base,
            rule,
            type_value,
            outflow,
            skipped_status,
            problem,
            ..
        } = draft;
        if let Some(message) = broken {
            row.status = invalid("invalid_row", message);
            return Planned::new(row, None);
        }
        // The user's action, else the description rule's, else the type
        // rule's or the default action.
        let ruled = rule
            .map(|at| &self.rules[at])
            .and_then(|rule| rule.action.map(|action| (action, By::Rule(&rule.text))));
        let mapped = ruled.as_ref().map_or(base, |(action, _)| *action);
        let invalid = *mapped != StatementAction::Skip
            && skipped_status.is_none()
            && matches!(problem, Some(StatementRowStatus::Invalid { .. }));
        let user = user.filter(|_| !invalid);
        let (action, by) = match (user.and_then(|user| user.action.as_ref()), ruled) {
            (Some(action), _) => (action, By::User),
            (None, Some(ruled)) => ruled,
            (None, None) => (base, By::Mapping(type_value.as_deref())),
        };
        row.base_action = Some(mapped.clone());
        row.action = action.clone();
        row.kind = outflow.and_then(|outflow| kind_of(action, outflow));
        row.counter_wallet_id = counter_wallet(action);
        let Some(wallet) = row.wallet_id else {
            let account = row.account.as_deref().unwrap_or_default();
            row.status = skipped("skipped_account", format!("account {account} is left out"));
            return Planned::new(row, None);
        };
        // In the order a user expects to read it: what the mapping or the
        // user skip, then what cannot be read, then what is missing.
        let outcome = if *action == StatementAction::Skip {
            Err(by.skip())
        } else if let Some(status) = skipped_status {
            Err(skipped(
                "skipped_status",
                format!("status {status} is skipped"),
            ))
        } else if let Some(problem) = problem {
            Err(problem)
        } else {
            self.import(&row, wallet, action, &by, user)
        };
        match outcome {
            Ok(command) => Planned::new(row, Some(command)),
            Err(status) => Planned::new(StatementRow { status, ..row }, None),
        }
    }

    /// The command of a row whose date and amount were read, going to
    /// `wallet` as the action `by` whoever said it, or why it waits for a
    /// wallet.
    fn import(
        &self,
        row: &StatementRow,
        wallet: Uuid,
        action: &StatementAction,
        by: &By<'_>,
        user: Option<&StatementRowOverride>,
    ) -> std::result::Result<Command, StatementRowStatus> {
        // Read whenever no problem was found.
        let Some(occurred_at) = row.occurred_at else {
            return Err(invalid(
                "invalid_date",
                "the date cannot be read".to_string(),
            ));
        };
        let text = user
            .and_then(|user| user.note.as_deref())
            .unwrap_or(&row.note);
        let note = compose_note(text, row.original.as_deref());
        let transfer = |from_wallet_id, to_wallet_id| Command::TransferWallet {
            amount: row.amount,
            from_wallet_id,
            to_wallet_id,
            note: note.clone(),
            occurred_at,
        };
        let entry = || Entry {
            amount: row.amount,
            wallet_id: Some(wallet),
            flow_id: self.flow(user),
            category: match user.and_then(|user| user.category.as_deref()) {
                Some(typed) => {
                    let typed = typed.trim();
                    (!typed.is_empty()).then(|| typed.to_string())
                }
                None => row.category.clone(),
            },
            note: note.clone(),
            occurred_at,
            // A statement is the importer's own account.
            person: None,
        };
        // A sign that contradicts the action (a refund written as money out)
        // keeps the action: the rule names what the row is, the amount is
        // taken as absolute.
        Ok(match action {
            StatementAction::TransferIn { from_wallet_id } => transfer(
                self.other_wallet(*from_wallet_id, wallet, "came from")?,
                wallet,
            ),
            StatementAction::TransferOut { to_wallet_id } => {
                transfer(wallet, self.other_wallet(*to_wallet_id, wallet, "went to")?)
            }
            StatementAction::Skip => return Err(by.skip()),
            StatementAction::BySign if row.outflow => Command::Expense(entry()),
            StatementAction::BySign | StatementAction::Income => Command::Income(entry()),
            StatementAction::Expense => Command::Expense(entry()),
            StatementAction::Refund => Command::Refund(entry()),
        })
    }

    /// The other wallet of a transfer, or why the row waits for one
    /// (`needs_wallet`): none named, or one the money cannot move to or from
    /// (the row's `own` wallet, one not in the vault, an archived one). `way`
    /// is `came from` or `went to`.
    fn other_wallet(
        &self,
        named: Option<Uuid>,
        own: Uuid,
        way: &str,
    ) -> std::result::Result<Uuid, StatementRowStatus> {
        let reason = match named {
            None => format!("choose the wallet the money {way}"),
            Some(id) if id == own => format!(
                "the money cannot move between the imported wallet and itself: choose the wallet it {way}"
            ),
            Some(id) => match self.wallets.get(&id) {
                None => {
                    format!("wallet {id} is not in the vault: choose the wallet the money {way}")
                }
                Some(true) => format!("the wallet the money {way} is archived: choose another"),
                Some(false) => return Ok(id),
            },
        };
        Err(skipped("needs_wallet", reason))
    }

    /// The envelope of an entry: the user's when it is an active envelope of
    /// the vault (an archived or unknown one is ignored), else the options'.
    /// The user's Unallocated is `None`, as the options name it.
    fn flow(&self, user: Option<&StatementRowOverride>) -> Option<Uuid> {
        match user
            .and_then(|user| user.flow_id)
            .filter(|id| self.active_flows.contains(id))
        {
            Some(id) if id == self.unallocated => None,
            Some(id) => Some(id),
            None => self.flow_id,
        }
    }

    /// The rule for the type value, else the default action.
    fn action(&self, type_value: Option<&str>) -> &'a StatementAction {
        type_value
            .and_then(|value| {
                self.mapping
                    .type_rules
                    .iter()
                    .find(|rule| same_text(&rule.value, value))
            })
            .map_or(&self.mapping.default_action, |rule| &rule.action)
    }

    /// `v1|vault|wallet|date|type|signed minor units|payee`: what the row
    /// is, never what the mapping does with it. The date is the
    /// [`StatementDate::stamp`] (the UTC instant for a timestamp); the type
    /// is the type column's value, or else `base_kind`, the kind the type
    /// rules and the default action give the row (`skip` for none), never a
    /// description rule's nor the user's; the amount is signed as the file
    /// wrote it; the payee is lowercased. The status is left out on purpose.
    /// With an account column, the wallet is `acct:` and the row's account
    /// trimmed and lowercased instead, never the wallet the account goes to:
    /// an account mapped to another wallet later finds its rows imported.
    fn key(
        &self,
        stamp: &str,
        type_value: Option<&str>,
        base_kind: Option<TransactionKind>,
        (payee, account): (&str, Option<&str>),
        signed: i64,
    ) -> String {
        let what = type_value.map_or_else(
            || {
                base_kind
                    .map_or("skip", TransactionKind::as_str)
                    .to_string()
            },
            str::to_lowercase,
        );
        let wallet = if self.columns.account.is_some() {
            format!("acct:{}", account_key(account.unwrap_or_default()))
        } else {
            self.wallet_id.to_string()
        };
        format!(
            "{KEY_VERSION}|{}|{wallet}|{stamp}|{what}|{signed}|{}",
            self.vault_id,
            payee.to_lowercase()
        )
    }

    /// An active category whose name or alias is the bank's value, or the
    /// value without a `5462 - ` merchant category code.
    fn match_category(&self, bank: &str) -> Option<String> {
        let found = |value: &str| {
            normalize_category_key(value)
                .ok()
                .and_then(|key| self.categories.get(&key).cloned())
        };
        found(bank).or_else(|| strip_mcc(bank).and_then(found))
    }

    /// `12.34 USD` from the original amount and currency, when the row has
    /// both and they say something the amount does not: another currency, or
    /// the same currency with another amount.
    fn original(
        &self,
        value: Option<&str>,
        currency: Option<&str>,
        row_currency: Option<&str>,
        amount: Option<&Amount>,
    ) -> Option<String> {
        let (value, currency) = (value?, currency?);
        let row_currency = row_currency.unwrap_or(self.currency.code());
        if currency.eq_ignore_ascii_case(row_currency) {
            let original = parse_amount(
                value,
                self.mapping.decimal_comma,
                self.currency.minor_units(),
            );
            if let (Ok(original), Some(amount)) = (original, amount)
                && original.minor.unsigned_abs() == amount.minor.unsigned_abs()
            {
                return None;
            }
        }
        let magnitude = value
            .trim_start_matches(['-', '+', '\u{2212}'])
            .trim_start();
        Some(format!("{magnitude} {currency}"))
    }
}

/// Normalized names and aliases of the vault's active, non-system categories.
/// A bank category is only ever matched against these: the import never
/// creates a category from the bank's.
fn category_names(core: &Core, vault_id: Uuid) -> Result<HashMap<String, String>> {
    let mut names = HashMap::new();
    for sql in [
        "SELECT a.alias_norm, c.name FROM category_aliases a
         JOIN categories c ON c.id = a.category_id
         WHERE a.vault_id = ?1 AND c.archived = 0 AND c.is_system = 0",
        "SELECT name_norm, name FROM categories
         WHERE vault_id = ?1 AND archived = 0 AND is_system = 0",
    ] {
        let mut stmt = core.conn.prepare(sql)?;
        let rows = stmt.query_map(params![vault_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (key, name) = row?;
            names.insert(key, name);
        }
    }
    Ok(names)
}

/// The kind an action gives a row whose money goes out (`outflow`) or in.
fn kind_of(action: &StatementAction, outflow: bool) -> Option<TransactionKind> {
    match action {
        StatementAction::BySign if outflow => Some(TransactionKind::Expense),
        StatementAction::BySign | StatementAction::Income => Some(TransactionKind::Income),
        StatementAction::Expense => Some(TransactionKind::Expense),
        StatementAction::Refund => Some(TransactionKind::Refund),
        StatementAction::TransferIn { .. } | StatementAction::TransferOut { .. } => {
            Some(TransactionKind::TransferWallet)
        }
        StatementAction::Skip => None,
    }
}

/// The other wallet a transfer action names.
fn counter_wallet(action: &StatementAction) -> Option<Uuid> {
    match action {
        StatementAction::TransferIn { from_wallet_id } => *from_wallet_id,
        StatementAction::TransferOut { to_wallet_id } => *to_wallet_id,
        _ => None,
    }
}

/// The parts, trimmed, joined with a space, with inner runs of spaces
/// collapsed.
fn collapse<'s>(parts: impl Iterator<Item = &'s str>) -> String {
    parts
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The note of the command: the payee (or the user's note) with the original
/// amount in parentheses.
fn compose_note(text: &str, original: Option<&str>) -> Option<String> {
    let text = text.trim();
    match (text.is_empty(), original) {
        (false, Some(original)) => Some(format!("{text} ({original})")),
        (false, None) => Some(text.to_string()),
        (true, Some(original)) => Some(original.to_string()),
        (true, None) => None,
    }
}

/// `5462 - Bakeries` -> `Bakeries`.
fn strip_mcc(value: &str) -> Option<&str> {
    let value = value.trim();
    let digits = value.bytes().take_while(u8::is_ascii_digit).count();
    if digits != 4 {
        return None;
    }
    let rest = value[digits..].trim_start().strip_prefix('-')?.trim_start();
    (!rest.is_empty()).then_some(rest)
}

fn same_text(left: &str, right: &str) -> bool {
    left.trim().to_lowercase() == right.trim().to_lowercase()
}

/// An account as the options and the ids compare it: trimmed, lowercased.
pub(super) fn account_key(account: &str) -> String {
    account.trim().to_lowercase()
}

/// A transfer column's cell that says the row is one half of a move:
/// `true`, `1`, `yes`, `y`, `si`, `sì` or `x`, whatever the case.
fn truthy(cell: &str) -> bool {
    matches!(
        cell.trim().to_lowercase().as_str(),
        "true" | "1" | "yes" | "y" | "si" | "sì" | "x"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planned(line: u32) -> Planned {
        Planned::new(
            StatementRow {
                line,
                kind: Some(TransactionKind::Expense),
                amount: 120,
                payee: "BAR".to_string(),
                action: StatementAction::Expense,
                outflow: true,
                note: "BAR".to_string(),
                base_action: Some(StatementAction::Expense),
                wallet_id: Some(Uuid::nil()),
                ..StatementRow::default()
            },
            None,
        )
    }

    #[test]
    fn a_transfer_cell_says_yes_in_many_ways() {
        for yes in ["true", " TRUE ", "1", "yes", "Y", "si", "Sì", "x"] {
            assert!(truthy(yes), "{yes}");
        }
        for no in ["false", "0", "no", "", "transfer"] {
            assert!(!truthy(no), "{no}");
        }
    }

    #[test]
    fn an_override_skip_carries_its_own_code() {
        let mut rows = [planned(2), planned(3)];
        let skip = StatementRowOverride {
            line: 3,
            skip: true,
            ..Default::default()
        };
        leave_out(&mut rows, &HashMap::from([(3, &skip)]));
        assert_eq!(rows[0].row.status, StatementRowStatus::New);
        assert_eq!(
            rows[1].row.status,
            StatementRowStatus::Skipped {
                code: "skipped_by_you".to_string(),
                reason: "skipped by you".to_string(),
            }
        );
    }
}
