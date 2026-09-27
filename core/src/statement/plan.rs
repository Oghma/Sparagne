//! From a statement and a mapping to one planned row per record: what the row
//! is, the id its command gets and, when it is new, the command itself.
//!
//! Every row is read on its own first ([`Context::read`]); a second pass over
//! the file numbers the rows with the same key, turns keys into ids and looks
//! the ids up in the log.

use std::collections::HashMap;

use chrono_tz::Tz;
use rusqlite::params;
use uuid::Uuid;

use super::amount::{Amount, parse_amount};
use super::csv::{self, Record};
use super::dates::{StatementDate, parse_date};
use super::{
    AmountSign, STATEMENT_NAMESPACE, StatementAction, StatementDateFormat, StatementMapping,
    StatementOptions, StatementRow, StatementRowOverride, StatementRowStatus,
};
use crate::{
    Command, Core, Currency, DomainError, Entry, Result, TransactionKind, normalize_category_key,
};

/// Version of the key format, first in every key: changing the format means
/// a new prefix, never a silent change of the ids of rows already imported.
const KEY_VERSION: &str = "v1";

/// A row of the statement with what happens to it.
pub(super) struct Planned {
    pub row: StatementRow,
    /// The command of a [`StatementRowStatus::New`] row.
    pub command: Option<Command>,
}

/// Reads every row of `text` and decides what becomes of it. Writes nothing.
pub(super) fn plan(
    core: &Core,
    vault_id: Uuid,
    text: &str,
    mapping: &StatementMapping,
    options: &StatementOptions,
) -> Result<Vec<Planned>> {
    let delimiter = delimiter(&mapping.delimiter)?;
    let mut records = csv::records(text, delimiter);
    let context = Context::new(core, vault_id, &mut records, mapping, options)?;

    let mut known = core
        .conn
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM commands WHERE id = ?1)")?;
    let mut occurrences: HashMap<String, u32> = HashMap::new();
    let mut planned = Vec::new();
    for record in records {
        let Draft {
            mut row,
            outcome,
            key,
        } = context.read(&record);
        let mut imported = false;
        if let Some(key) = key {
            let seen = occurrences.entry(key.clone()).or_insert(0);
            row.command_id = command_id(&format!("{key}|{seen}"));
            *seen += 1;
            imported = known.query_row(params![row.command_id], |r| r.get(0))?;
        }
        let mut command = None;
        row.status = if imported {
            StatementRowStatus::AlreadyImported
        } else {
            match outcome {
                Outcome::Import(planned_command) => {
                    command = Some(planned_command);
                    StatementRowStatus::New
                }
                Outcome::Skip(code, reason) => StatementRowStatus::Skipped {
                    code: code.to_string(),
                    reason,
                },
                Outcome::Invalid(code, message) => StatementRowStatus::Invalid {
                    code: code.to_string(),
                    message,
                },
            }
        };
        planned.push(Planned { row, command });
    }
    Ok(planned)
}

/// Applies the user's changes to the new rows: skip them, or change their
/// category or note. Overrides of rows that are not new change nothing; the
/// last override of a line wins.
pub(super) fn apply_overrides(planned: &mut [Planned], overrides: &[StatementRowOverride]) {
    let by_line: HashMap<u32, &StatementRowOverride> =
        overrides.iter().map(|o| (o.line, o)).collect();
    for item in planned {
        let Some(user) = by_line.get(&item.row.line) else {
            continue;
        };
        if item.row.status != StatementRowStatus::New {
            continue;
        }
        if user.skip {
            item.row.status = StatementRowStatus::Skipped {
                code: "skipped_by_you".to_string(),
                reason: "skipped by you".to_string(),
            };
            item.command = None;
            continue;
        }
        let note = user
            .note
            .as_deref()
            .map(|note| compose_note(note, item.row.original.as_deref()));
        match item.command.as_mut() {
            Some(Command::Income(entry) | Command::Expense(entry) | Command::Refund(entry)) => {
                if let Some(category) = &user.category {
                    let category = category.trim();
                    entry.category = (!category.is_empty()).then(|| category.to_string());
                }
                if let Some(note) = note {
                    entry.note = note;
                }
            }
            Some(Command::TransferWallet { note: current, .. }) => {
                if let Some(note) = note {
                    *current = note;
                }
            }
            _ => {}
        }
    }
}

/// The id of the row whose full key (occurrence index included) is `key`.
fn command_id(key: &str) -> Uuid {
    Uuid::new_v5(&STATEMENT_NAMESPACE, key.as_bytes())
}

/// The one-character delimiter of a mapping. `\t` and `tab` spelled out are
/// read as a tab.
fn delimiter(value: &str) -> Result<char> {
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

// ---------------------------------------------------------------------------
// Reading one row
// ---------------------------------------------------------------------------

/// A row read on its own, before the file-wide pass gives it an id.
struct Draft {
    row: StatementRow,
    outcome: Outcome,
    /// The key without the occurrence index; `None` when the date or the
    /// amount could not be read, and the row then keeps the nil id.
    key: Option<String>,
}

/// The cells of a row the outcome depends on, read once.
struct Cells<'r> {
    type_value: Option<&'r str>,
    status: Option<&'r str>,
    currency: Option<&'r str>,
    date: std::result::Result<StatementDate, String>,
    amount: std::result::Result<Amount, String>,
}

/// What a row becomes when its id is not in the log yet. `Skip` and
/// `Invalid` carry the code of [`StatementRowStatus`] and the English detail.
enum Outcome {
    Import(Command),
    Skip(&'static str, String),
    Invalid(&'static str, String),
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
}

impl Columns {
    /// Every named column must be in the header (case-insensitive, trimmed).
    /// A blank optional column is no column.
    fn resolve(headers: &[String], mapping: &StatementMapping) -> Result<Self> {
        let find = |name: &str| -> Result<usize> {
            let wanted = name.trim().to_lowercase();
            if wanted.is_empty() {
                return Err(DomainError::InvalidCommand(
                    "the mapping leaves a required column blank".to_string(),
                ));
            }
            headers
                .iter()
                .position(|h| h.trim().to_lowercase() == wanted)
                .ok_or_else(|| {
                    DomainError::InvalidCommand(format!(
                        "column '{}' is not in the statement",
                        name.trim()
                    ))
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
    /// Normalized name or alias -> name of an active, non-system category.
    categories: HashMap<String, String>,
}

impl<'a> Context<'a> {
    /// Checks the vault, the wallets, the flow, the timezone and the mapping
    /// against the header, which it takes from `records`.
    fn new(
        core: &Core,
        vault_id: Uuid,
        records: &mut csv::Records<'_>,
        mapping: &'a StatementMapping,
        options: &StatementOptions,
    ) -> Result<Self> {
        let snapshot = core.snapshot(vault_id)?;
        let wallet_exists = |id: Uuid| snapshot.wallets.iter().any(|w| w.id == id);
        if !wallet_exists(options.wallet_id) {
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
        let actions = mapping.type_rules.iter().map(|rule| &rule.action);
        for other in actions
            .chain([&mapping.default_action])
            .filter_map(counter_wallet)
        {
            if !wallet_exists(other) {
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

        let header = records
            .next()
            .ok_or_else(|| DomainError::InvalidCommand("the statement is empty".to_string()))?;
        if header.unclosed_quote {
            return Err(DomainError::InvalidCommand(
                "the header of the statement never closes a quote".to_string(),
            ));
        }
        Ok(Self {
            columns: Columns::resolve(&header.fields, mapping)?,
            mapping,
            vault_id,
            wallet_id: options.wallet_id,
            flow_id: options.flow_id,
            currency: snapshot.currency,
            tz,
            categories: category_names(core, vault_id)?,
        })
    }

    /// Everything about one record, but its id and whether it is in the log.
    fn read(&self, record: &Record) -> Draft {
        let mut row = StatementRow {
            line: record.line,
            command_id: Uuid::nil(),
            status: StatementRowStatus::New,
            kind: None,
            occurred_at: None,
            amount: 0,
            rounded: false,
            payee: String::new(),
            bank_category: None,
            matched_category: None,
            counter_wallet_id: None,
            original: None,
        };
        let invalid = |row, message: String| Draft {
            row,
            outcome: Outcome::Invalid("invalid_row", message),
            key: None,
        };
        if record.unclosed_quote {
            return invalid(
                row,
                "a quoted field is never closed, so the rest of the file is in this row"
                    .to_string(),
            );
        }
        // Empty cells past the header are the trailing delimiter some banks
        // end every row with, not a ragged row.
        if !csv::agrees(record, self.columns.count) {
            return invalid(
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
        row.bank_category = optional(self.columns.category).map(str::to_string);
        row.matched_category = row
            .bank_category
            .as_deref()
            .and_then(|bank| self.match_category(bank));

        let cells = Cells {
            type_value: optional(self.columns.kind),
            status: optional(self.columns.status),
            currency: optional(self.columns.currency),
            date: parse_date(field(self.columns.date), &self.mapping.date_format, self.tz),
            amount: parse_amount(
                field(self.columns.amount),
                self.mapping.decimal_comma,
                self.currency.minor_units(),
            ),
        };
        let action = self.action(cells.type_value);

        row.occurred_at = cells.date.as_ref().ok().map(|date| date.at);
        if let Ok(amount) = &cells.amount {
            row.amount = amount.minor.abs();
            row.rounded = amount.rounded;
        }
        row.kind = self
            .outflow(&cells)
            .and_then(|outflow| kind_of(action, outflow));
        row.counter_wallet_id = counter_wallet(action);
        row.original = self.original(
            optional(self.columns.original_amount),
            optional(self.columns.original_currency),
            cells.currency,
            cells.amount.as_ref().ok(),
        );
        let key = match (&cells.date, &cells.amount) {
            (Ok(date), Ok(amount)) => {
                Some(self.key(&row, &date.stamp, cells.type_value, amount.minor))
            }
            _ => None,
        };
        let outcome = self.outcome(&row, action, cells);
        Draft { row, outcome, key }
    }

    /// Whether the row's money goes out; `None` when the amount is unreadable.
    fn outflow(&self, cells: &Cells<'_>) -> Option<bool> {
        let minor = cells.amount.as_ref().ok()?.minor;
        Some(match self.mapping.amount_sign {
            AmountSign::OutflowPositive => minor > 0,
            AmountSign::OutflowNegative => minor < 0,
        })
    }

    /// Why the row is not imported, in the order a user expects to read it:
    /// what the mapping skips, then what cannot be read, then what is missing.
    fn outcome(&self, row: &StatementRow, action: &StatementAction, cells: Cells<'_>) -> Outcome {
        let outflow = self.outflow(&cells);
        let type_value = cells.type_value;
        if *action == StatementAction::Skip {
            return Outcome::Skip("skipped_by_rule", skip_reason(type_value));
        }
        if let Some(status) = cells.status
            && self
                .mapping
                .skip_statuses
                .iter()
                .any(|skipped| same_text(skipped, status))
        {
            return Outcome::Skip("skipped_status", format!("status {status} is skipped"));
        }
        if let Some(currency) = cells.currency
            && !currency.eq_ignore_ascii_case(self.currency.code())
        {
            return Outcome::Invalid(
                "currency_mismatch",
                format!(
                    "the row is in {currency}, the vault in {}",
                    self.currency.code()
                ),
            );
        }
        let occurred_at = match cells.date {
            Ok(date) => date.at,
            Err(message) => return Outcome::Invalid("invalid_date", message),
        };
        let minor = match cells.amount {
            Ok(amount) => amount.minor,
            Err(message) => return Outcome::Invalid("invalid_amount", message),
        };
        if minor == 0 {
            return Outcome::Skip("zero_amount", "the amount is zero".to_string());
        }

        let note = compose_note(&row.payee, row.original.as_deref());
        let transfer = |from_wallet_id, to_wallet_id| {
            Outcome::Import(Command::TransferWallet {
                amount: row.amount,
                from_wallet_id,
                to_wallet_id,
                note: note.clone(),
                occurred_at,
            })
        };
        let entry = || Entry {
            amount: row.amount,
            wallet_id: Some(self.wallet_id),
            flow_id: self.flow_id,
            category: row.matched_category.clone(),
            note: note.clone(),
            occurred_at,
        };
        // A sign that contradicts the action (a refund written as money out)
        // keeps the action: the rule names what the row is, the amount is
        // taken as absolute.
        match action {
            StatementAction::TransferIn {
                from_wallet_id: Some(from),
            } => transfer(*from, self.wallet_id),
            StatementAction::TransferIn {
                from_wallet_id: None,
            } => Outcome::Skip(
                "needs_wallet",
                "choose the wallet the money came from".to_string(),
            ),
            StatementAction::TransferOut {
                to_wallet_id: Some(to),
            } => transfer(self.wallet_id, *to),
            StatementAction::TransferOut { to_wallet_id: None } => Outcome::Skip(
                "needs_wallet",
                "choose the wallet the money went to".to_string(),
            ),
            StatementAction::Skip => Outcome::Skip("skipped_by_rule", skip_reason(type_value)),
            StatementAction::BySign if outflow == Some(true) => {
                Outcome::Import(Command::Expense(entry()))
            }
            StatementAction::BySign | StatementAction::Income => {
                Outcome::Import(Command::Income(entry()))
            }
            StatementAction::Expense => Outcome::Import(Command::Expense(entry())),
            StatementAction::Refund => Outcome::Import(Command::Refund(entry())),
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
    /// is the type column's value, or the kind when there is none; the amount
    /// is signed as the file wrote it; the payee is lowercased. The status is
    /// left out on purpose.
    fn key(
        &self,
        row: &StatementRow,
        stamp: &str,
        type_value: Option<&str>,
        signed: i64,
    ) -> String {
        let what = type_value.map_or_else(
            || row.kind.map_or("skip", TransactionKind::as_str).to_string(),
            str::to_lowercase,
        );
        format!(
            "{KEY_VERSION}|{}|{}|{stamp}|{what}|{signed}|{}",
            self.vault_id,
            self.wallet_id,
            row.payee.to_lowercase()
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

fn skip_reason(type_value: Option<&str>) -> String {
    type_value.map_or_else(
        || "skipped by the mapping".to_string(),
        |value| format!("type {value} is skipped"),
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn planned(line: u32) -> Planned {
        Planned {
            row: StatementRow {
                line,
                command_id: Uuid::nil(),
                status: StatementRowStatus::New,
                kind: Some(TransactionKind::Expense),
                occurred_at: None,
                amount: 120,
                rounded: false,
                payee: "BAR".to_string(),
                bank_category: None,
                matched_category: None,
                counter_wallet_id: None,
                original: None,
            },
            command: None,
        }
    }

    #[test]
    fn an_override_skip_carries_its_own_code() {
        let mut rows = [planned(2), planned(3)];
        apply_overrides(
            &mut rows,
            &[StatementRowOverride {
                line: 3,
                category: None,
                note: None,
                skip: true,
            }],
        );
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
