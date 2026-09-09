//! Quick-add: the one-line grammar for entering a transaction.
//!
//! Two stages: [`parse`] turns a line of text into a [`QuickAdd`] with no
//! database access, then [`Core::resolve_quick_add`] resolves the wallet and
//! flow names it carries against a vault's active entities and produces a
//! [`Command`] ready for [`crate::Core::execute`].
//!
//! Grammar (see `docs/v2/DISTILLATO_V1.md` §3.1):
//!
//! ```text
//! [+|-|r] importo  [nota…]  [#categoria]  [@wallet]  [>busta]  [data]
//! tw> importo @da @a [nota] [data]        transfer between wallets
//! tf> importo >da >a [nota] [data]        transfer between flows
//! ```

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, TimeZone};
use thiserror::Error;
use uuid::Uuid;

use crate::{Command, Core, Currency, DomainError, Entry, Money, TransactionKind};

/// Error raised while parsing or resolving a quick-add line.
#[derive(Error, Debug, Clone, PartialEq, Eq, uniffi::Error)]
#[uniffi(flat_error)]
pub enum QuickAddError {
    #[error("the line is empty")]
    EmptyInput,
    #[error("the amount must be the first token")]
    MissingAmount,
    #[error("invalid amount: {0}")]
    InvalidAmount(String),
    #[error("duplicate '{0}' marker")]
    DuplicateMarker(char),
    #[error("marker '{marker}' is not allowed here")]
    MarkerNotAllowed { marker: char },
    #[error("a transfer needs two '{0}' targets")]
    MissingTransferTarget(char),
    #[error("invalid date '{0}'")]
    InvalidDate(String),
    #[error("only one date token is allowed")]
    DuplicateDate,
    #[error("'{name}' is ambiguous among {candidates:?}")]
    AmbiguousName {
        name: String,
        candidates: Vec<String>,
    },
    #[error("unknown {kind} '{name}'")]
    UnknownName { kind: &'static str, name: String },
    #[error("the transfer source and target are the same")]
    SameTarget,
    #[error(transparent)]
    Domain(#[from] DomainError),
}

impl QuickAddError {
    /// Stable snake_case code for clients.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::EmptyInput => "empty_input",
            Self::MissingAmount => "missing_amount",
            Self::InvalidAmount(_) => "invalid_amount",
            Self::DuplicateMarker(_) => "duplicate_marker",
            Self::MarkerNotAllowed { .. } => "marker_not_allowed",
            Self::MissingTransferTarget(_) => "missing_transfer_target",
            Self::InvalidDate(_) => "invalid_date",
            Self::DuplicateDate => "duplicate_date",
            Self::AmbiguousName { .. } => "ambiguous_name",
            Self::UnknownName { .. } => "unknown_name",
            Self::SameTarget => "same_target",
            Self::Domain(e) => e.code(),
        }
    }
}

impl From<QuickAddError> for DomainError {
    fn from(err: QuickAddError) -> Self {
        DomainError::InvalidCommand(err.to_string())
    }
}

/// A relative or absolute date token from a quick-add line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DateSpec {
    Today,
    Yesterday,
    /// `N` days before today, `N >= 1`.
    DaysAgo(u32),
    /// Day and month, year resolved against a reference date.
    DayMonth {
        day: u8,
        month: u8,
    },
    Date(NaiveDate),
}

impl DateSpec {
    /// Resolves against `today`. `DayMonth` uses `today`'s year; invalid
    /// calendar dates (e.g. 31 February) are errors.
    pub fn resolve(self, today: NaiveDate) -> Result<NaiveDate, QuickAddError> {
        match self {
            Self::Today => Ok(today),
            Self::Yesterday => today
                .pred_opt()
                .ok_or_else(|| QuickAddError::InvalidDate("yesterday".to_string())),
            Self::DaysAgo(n) => today
                .checked_sub_signed(chrono::Duration::days(i64::from(n)))
                .ok_or_else(|| QuickAddError::InvalidDate(format!("-{n}d"))),
            Self::DayMonth { day, month } => {
                NaiveDate::from_ymd_opt(today.year(), u32::from(month), u32::from(day))
                    .ok_or_else(|| QuickAddError::InvalidDate(format!("{day:02}/{month:02}")))
            }
            Self::Date(date) => Ok(date),
        }
    }
}

/// A parsed quick-add line, before name resolution.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum QuickAdd {
    Entry {
        kind: TransactionKind,
        /// Absolute, `> 0`.
        amount: i64,
        note: Option<String>,
        category: Option<String>,
        wallet: Option<String>,
        flow: Option<String>,
        date: Option<DateSpec>,
    },
    TransferWallet {
        amount: i64,
        from: String,
        to: String,
        note: Option<String>,
        date: Option<DateSpec>,
    },
    TransferFlow {
        amount: i64,
        from: String,
        to: String,
        note: Option<String>,
        date: Option<DateSpec>,
    },
}

/// Parses one quick-add line. Pure: no database access.
pub fn parse(input: &str, currency: Currency) -> Result<QuickAdd, QuickAddError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(QuickAddError::EmptyInput);
    }

    if let Some(rest) = strip_ci_prefix(trimmed, "tw>") {
        return parse_transfer(rest, currency, TransferKind::Wallet);
    }
    if let Some(rest) = strip_ci_prefix(trimmed, "tf>") {
        return parse_transfer(rest, currency, TransferKind::Flow);
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let first = tokens[0];
    let (kind, amount_token, note_start): (TransactionKind, Option<&str>, usize) =
        if first.eq_ignore_ascii_case("r") {
            (TransactionKind::Refund, tokens.get(1).copied(), 2)
        } else if first == "+" {
            (TransactionKind::Income, tokens.get(1).copied(), 2)
        } else if first == "-" {
            (TransactionKind::Expense, tokens.get(1).copied(), 2)
        } else if let Some(rest) = first.strip_prefix('+') {
            (TransactionKind::Income, Some(rest), 1)
        } else if let Some(rest) = first.strip_prefix('-') {
            (TransactionKind::Expense, Some(rest), 1)
        } else if first.len() > 1 && (first.starts_with('r') || first.starts_with('R')) {
            (TransactionKind::Refund, Some(&first[1..]), 1)
        } else {
            (TransactionKind::Expense, Some(first), 1)
        };

    let amount_token = match amount_token {
        Some(t) if !t.is_empty() => t,
        _ => return Err(QuickAddError::MissingAmount),
    };
    let amount = parse_positive_amount(amount_token, currency)?;

    let mut category: Option<String> = None;
    let mut wallet: Option<String> = None;
    let mut flow: Option<String> = None;
    let mut date: Option<DateSpec> = None;
    let mut note_tokens: Vec<&str> = Vec::new();

    for token in &tokens[note_start..] {
        if let Some(rest) = token.strip_prefix('#') {
            if rest.is_empty() {
                note_tokens.push(token);
            } else if category.is_some() {
                return Err(QuickAddError::DuplicateMarker('#'));
            } else {
                category = Some(rest.to_string());
            }
        } else if let Some(rest) = token.strip_prefix('@') {
            if rest.is_empty() {
                note_tokens.push(token);
            } else if wallet.is_some() {
                return Err(QuickAddError::DuplicateMarker('@'));
            } else {
                wallet = Some(rest.to_string());
            }
        } else if let Some(rest) = token.strip_prefix('>') {
            if rest.is_empty() {
                note_tokens.push(token);
            } else if flow.is_some() {
                return Err(QuickAddError::DuplicateMarker('>'));
            } else {
                flow = Some(rest.to_string());
            }
        } else if let Some(result) = try_parse_date_token(token) {
            if date.is_some() {
                return Err(QuickAddError::DuplicateDate);
            }
            date = Some(result?);
        } else {
            note_tokens.push(token);
        }
    }

    Ok(QuickAdd::Entry {
        kind,
        amount,
        note: join_note(&note_tokens),
        category,
        wallet,
        flow,
        date,
    })
}

#[derive(Clone, Copy)]
enum TransferKind {
    Wallet,
    Flow,
}

fn parse_transfer(
    rest: &str,
    currency: Currency,
    kind: TransferKind,
) -> Result<QuickAdd, QuickAddError> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let amount_token = tokens
        .first()
        .copied()
        .ok_or(QuickAddError::MissingAmount)?;
    let amount = parse_positive_amount(amount_token, currency)?;

    let (marker, forbidden) = match kind {
        TransferKind::Wallet => ('@', '>'),
        TransferKind::Flow => ('>', '@'),
    };

    let mut targets: Vec<String> = Vec::new();
    let mut date: Option<DateSpec> = None;
    let mut note_tokens: Vec<&str> = Vec::new();

    for token in &tokens[1..] {
        if let Some(tag) = token.strip_prefix('#') {
            if tag.is_empty() {
                note_tokens.push(token);
            } else {
                return Err(QuickAddError::MarkerNotAllowed { marker: '#' });
            }
        } else if let Some(tag) = token.strip_prefix(marker) {
            if tag.is_empty() {
                note_tokens.push(token);
            } else if targets.len() >= 2 {
                return Err(QuickAddError::DuplicateMarker(marker));
            } else {
                targets.push(tag.to_string());
            }
        } else if let Some(tag) = token.strip_prefix(forbidden) {
            if tag.is_empty() {
                note_tokens.push(token);
            } else {
                return Err(QuickAddError::MarkerNotAllowed { marker: forbidden });
            }
        } else if let Some(result) = try_parse_date_token(token) {
            if date.is_some() {
                return Err(QuickAddError::DuplicateDate);
            }
            date = Some(result?);
        } else {
            note_tokens.push(token);
        }
    }

    if targets.len() < 2 {
        return Err(QuickAddError::MissingTransferTarget(marker));
    }
    let from = targets[0].clone();
    let to = targets[1].clone();
    let note = join_note(&note_tokens);

    Ok(match kind {
        TransferKind::Wallet => QuickAdd::TransferWallet {
            amount,
            from,
            to,
            note,
            date,
        },
        TransferKind::Flow => QuickAdd::TransferFlow {
            amount,
            from,
            to,
            note,
            date,
        },
    })
}

fn parse_positive_amount(token: &str, currency: Currency) -> Result<i64, QuickAddError> {
    let amount = Money::parse_major(token, currency)
        .map_err(|e| QuickAddError::InvalidAmount(e.to_string()))?;
    if amount.minor() <= 0 {
        return Err(QuickAddError::InvalidAmount(
            "amount must be greater than zero".to_string(),
        ));
    }
    Ok(amount.minor())
}

fn join_note(tokens: &[&str]) -> Option<String> {
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" "))
    }
}

/// Case-insensitive prefix strip, byte-exact (the prefixes are ASCII).
fn strip_ci_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len()
        && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
    {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// Recognizes one of the date token forms. `None` means the token is not
/// shaped like a date and should be treated as note text; `Some(Err(_))`
/// means it is shaped like a date but is not a valid one.
fn try_parse_date_token(token: &str) -> Option<Result<DateSpec, QuickAddError>> {
    let lower = token.to_ascii_lowercase();
    if lower == "oggi" || lower == "today" {
        return Some(Ok(DateSpec::Today));
    }
    if lower == "ieri" || lower == "yesterday" {
        return Some(Ok(DateSpec::Yesterday));
    }
    if let Some(rest) = lower.strip_prefix('-')
        && let Some(digits) = rest.strip_suffix('d')
        && !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
    {
        return Some(
            digits
                .parse::<u32>()
                .map_err(|_| QuickAddError::InvalidDate(token.to_string()))
                .and_then(|n| {
                    if n >= 1 {
                        Ok(DateSpec::DaysAgo(n))
                    } else {
                        Err(QuickAddError::InvalidDate(token.to_string()))
                    }
                }),
        );
    }
    if !token.starts_with('-')
        && let Some(parts) = split_numeric(token, '-')
        && parts.len() == 3
        && parts[0].len() == 4
    {
        return Some(build_date(parts[0], parts[1], parts[2], token));
    }
    if let Some(parts) = split_numeric(token, '/') {
        match parts.len() {
            2 => {
                return Some(
                    parts[0]
                        .parse::<u8>()
                        .and_then(|day| parts[1].parse::<u8>().map(|month| (day, month)))
                        .map_err(|_| QuickAddError::InvalidDate(token.to_string()))
                        .map(|(day, month)| DateSpec::DayMonth { day, month }),
                );
            }
            3 => return Some(build_date(parts[2], parts[1], parts[0], token)),
            _ => {}
        }
    }
    None
}

/// Splits `token` on `sep`; `Some` only when every part is non-empty and
/// all-digit.
fn split_numeric(token: &str, sep: char) -> Option<Vec<&str>> {
    if token.is_empty() {
        return None;
    }
    let parts: Vec<&str> = token.split(sep).collect();
    if parts
        .iter()
        .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        Some(parts)
    } else {
        None
    }
}

fn build_date(year: &str, month: &str, day: &str, token: &str) -> Result<DateSpec, QuickAddError> {
    let invalid = || QuickAddError::InvalidDate(token.to_string());
    let year: i32 = year.parse().map_err(|_| invalid())?;
    let month: u32 = month.parse().map_err(|_| invalid())?;
    let day: u32 = day.parse().map_err(|_| invalid())?;
    NaiveDate::from_ymd_opt(year, month, day)
        .map(DateSpec::Date)
        .ok_or_else(invalid)
}

/// Sticky/last-used entities used when a quick-add line names no wallet or
/// flow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct QuickAddDefaults {
    pub wallet_id: Option<Uuid>,
    pub flow_id: Option<Uuid>,
}

impl Core {
    /// Resolves a parsed quick-add line against `vault_id`'s active wallets
    /// and flows and builds the [`Command`] to execute.
    pub fn resolve_quick_add(
        &self,
        vault_id: Uuid,
        parsed: &QuickAdd,
        now: DateTime<FixedOffset>,
        defaults: &QuickAddDefaults,
    ) -> Result<Command, QuickAddError> {
        let snapshot = self.snapshot(vault_id)?;
        let wallets: Vec<(Uuid, &str)> = snapshot
            .wallets
            .iter()
            .filter(|w| !w.archived)
            .map(|w| (w.id, w.name.as_str()))
            .collect();
        let flows: Vec<(Uuid, &str)> = snapshot
            .flows
            .iter()
            .filter(|f| !f.archived)
            .map(|f| (f.id, f.name.as_str()))
            .collect();

        match parsed {
            QuickAdd::Entry {
                kind,
                amount,
                note,
                category,
                wallet,
                flow,
                date,
            } => {
                let wallet_id = match wallet {
                    Some(name) => Some(resolve_name(&wallets, name, "wallet")?),
                    None => defaults.wallet_id,
                };
                let flow_id = match flow {
                    Some(name) => Some(resolve_name(&flows, name, "flow")?),
                    None => defaults.flow_id,
                };
                let entry = Entry {
                    amount: *amount,
                    wallet_id,
                    flow_id,
                    category: category.clone(),
                    note: note.clone(),
                    occurred_at: resolve_occurred_at(date, now)?,
                };
                Ok(match kind {
                    TransactionKind::Income => Command::Income(entry),
                    TransactionKind::Expense => Command::Expense(entry),
                    TransactionKind::Refund => Command::Refund(entry),
                    TransactionKind::TransferWallet | TransactionKind::TransferFlow => {
                        unreachable!("quick_add entries are never transfers")
                    }
                })
            }
            QuickAdd::TransferWallet {
                amount,
                from,
                to,
                note,
                date,
            } => {
                let from_wallet_id = resolve_name(&wallets, from, "wallet")?;
                let to_wallet_id = resolve_name(&wallets, to, "wallet")?;
                if from_wallet_id == to_wallet_id {
                    return Err(QuickAddError::SameTarget);
                }
                Ok(Command::TransferWallet {
                    amount: *amount,
                    from_wallet_id,
                    to_wallet_id,
                    note: note.clone(),
                    occurred_at: resolve_occurred_at(date, now)?,
                })
            }
            QuickAdd::TransferFlow {
                amount,
                from,
                to,
                note,
                date,
            } => {
                let from_flow_id = resolve_name(&flows, from, "flow")?;
                let to_flow_id = resolve_name(&flows, to, "flow")?;
                if from_flow_id == to_flow_id {
                    return Err(QuickAddError::SameTarget);
                }
                Ok(Command::TransferFlow {
                    amount: *amount,
                    from_flow_id,
                    to_flow_id,
                    note: note.clone(),
                    occurred_at: resolve_occurred_at(date, now)?,
                })
            }
        }
    }
}

/// `now` with its date replaced by the resolved `date` token, keeping the
/// wall-clock time and offset; `now` unchanged when there is no token.
fn resolve_occurred_at(
    date: &Option<DateSpec>,
    now: DateTime<FixedOffset>,
) -> Result<DateTime<FixedOffset>, QuickAddError> {
    let Some(spec) = date else {
        return Ok(now);
    };
    let resolved_date = spec.resolve(now.date_naive())?;
    let naive = NaiveDateTime::new(resolved_date, now.time());
    let offset = *now.offset();
    offset
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| QuickAddError::InvalidDate("ambiguous local time".to_string()))
}

/// Matches `query` case-insensitively against `candidates`, priority exact >
/// prefix > contains within the best non-empty tier.
fn resolve_name(
    candidates: &[(Uuid, &str)],
    query: &str,
    kind: &'static str,
) -> Result<Uuid, QuickAddError> {
    let query_lower = query.to_lowercase();
    let mut exact: Vec<(Uuid, &str)> = Vec::new();
    let mut prefix: Vec<(Uuid, &str)> = Vec::new();
    let mut contains: Vec<(Uuid, &str)> = Vec::new();

    for &(id, name) in candidates {
        let name_lower = name.to_lowercase();
        if name_lower == query_lower {
            exact.push((id, name));
        } else if name_lower.starts_with(&query_lower) {
            prefix.push((id, name));
        } else if name_lower.contains(&query_lower) {
            contains.push((id, name));
        }
    }

    let tier = if !exact.is_empty() {
        exact
    } else if !prefix.is_empty() {
        prefix
    } else {
        contains
    };

    match tier.as_slice() {
        [] => Err(QuickAddError::UnknownName {
            kind,
            name: query.to_string(),
        }),
        [(id, _)] => Ok(*id),
        many => Err(QuickAddError::AmbiguousName {
            name: query.to_string(),
            candidates: many.iter().map(|(_, name)| (*name).to_string()).collect(),
        }),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn eur() -> Currency {
        Currency::Eur
    }

    fn parse_ok(input: &str) -> QuickAdd {
        parse(input, eur()).unwrap_or_else(|e| panic!("expected Ok for {input:?}, got {e}"))
    }

    fn parse_err(input: &str) -> QuickAddError {
        parse(input, eur()).expect_err(&format!("expected Err for {input:?}"))
    }

    // -- sign forms -----------------------------------------------------

    #[test]
    fn no_sign_is_expense() {
        match parse_ok("12.50 bar") {
            QuickAdd::Entry {
                kind, amount, note, ..
            } => {
                assert_eq!(kind, TransactionKind::Expense);
                assert_eq!(amount, 1250);
                assert_eq!(note.as_deref(), Some("bar"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn glued_minus_is_expense() {
        match parse_ok("-12.50 bar") {
            QuickAdd::Entry { kind, amount, .. } => {
                assert_eq!(kind, TransactionKind::Expense);
                assert_eq!(amount, 1250);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn separate_minus_is_expense() {
        match parse_ok("- 12.50 bar") {
            QuickAdd::Entry { kind, amount, .. } => {
                assert_eq!(kind, TransactionKind::Expense);
                assert_eq!(amount, 1250);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn glued_plus_is_income() {
        match parse_ok("+1000 stipendio") {
            QuickAdd::Entry { kind, amount, .. } => {
                assert_eq!(kind, TransactionKind::Income);
                assert_eq!(amount, 100_000);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn separate_plus_is_income() {
        match parse_ok("+ 5 gift") {
            QuickAdd::Entry { kind, amount, .. } => {
                assert_eq!(kind, TransactionKind::Income);
                assert_eq!(amount, 500);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn glued_refund_with_comma_decimal() {
        match parse_ok("r3,20 rimborso") {
            QuickAdd::Entry {
                kind, amount, note, ..
            } => {
                assert_eq!(kind, TransactionKind::Refund);
                assert_eq!(amount, 320);
                assert_eq!(note.as_deref(), Some("rimborso"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn separate_refund() {
        match parse_ok("r 3.20 rimborso") {
            QuickAdd::Entry { kind, amount, .. } => {
                assert_eq!(kind, TransactionKind::Refund);
                assert_eq!(amount, 320);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn uppercase_refund_forms() {
        match parse_ok("R 3.20 rimborso") {
            QuickAdd::Entry { kind, .. } => assert_eq!(kind, TransactionKind::Refund),
            other => panic!("unexpected {other:?}"),
        }
        match parse_ok("R3.20 rimborso") {
            QuickAdd::Entry { kind, .. } => assert_eq!(kind, TransactionKind::Refund),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn word_starting_with_r_that_is_not_a_valid_amount_is_an_error() {
        // "rent" glued-refund-parses as amount "ent", which is not a number.
        assert!(matches!(
            parse_err("rent 50"),
            QuickAddError::InvalidAmount(_)
        ));
    }

    // -- markers ----------------------------------------------------------

    #[test]
    fn markers_in_any_order_and_note_collected() {
        match parse_ok("15 pizza >groceries @cash #food") {
            QuickAdd::Entry {
                category,
                wallet,
                flow,
                note,
                ..
            } => {
                assert_eq!(category.as_deref(), Some("food"));
                assert_eq!(wallet.as_deref(), Some("cash"));
                assert_eq!(flow.as_deref(), Some("groceries"));
                assert_eq!(note.as_deref(), Some("pizza"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn markers_before_note_words_still_resolve() {
        match parse_ok("12.50 #food bar caffè") {
            QuickAdd::Entry { category, note, .. } => {
                assert_eq!(category.as_deref(), Some("food"));
                assert_eq!(note.as_deref(), Some("bar caffè"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn bare_marker_stays_in_note() {
        match parse_ok("10 price is # not a category") {
            QuickAdd::Entry { category, note, .. } => {
                assert_eq!(category, None);
                assert_eq!(note.as_deref(), Some("price is # not a category"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn duplicate_category_marker_is_an_error() {
        assert!(matches!(
            parse_err("15 pizza #food #snack"),
            QuickAddError::DuplicateMarker('#')
        ));
    }

    #[test]
    fn duplicate_wallet_marker_is_an_error() {
        assert!(matches!(
            parse_err("15 pizza @cash @card"),
            QuickAddError::DuplicateMarker('@')
        ));
    }

    #[test]
    fn duplicate_flow_marker_is_an_error() {
        assert!(matches!(
            parse_err("15 pizza >ufficio >casa"),
            QuickAddError::DuplicateMarker('>')
        ));
    }

    // -- transfers ----------------------------------------------------------

    #[test]
    fn transfer_wallet_basic() {
        match parse_ok("tw>50 @bank @cash spostamento") {
            QuickAdd::TransferWallet {
                amount,
                from,
                to,
                note,
                ..
            } => {
                assert_eq!(amount, 5000);
                assert_eq!(from, "bank");
                assert_eq!(to, "cash");
                assert_eq!(note.as_deref(), Some("spostamento"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn transfer_wallet_case_insensitive_prefix() {
        match parse_ok("TW>50 @bank @cash") {
            QuickAdd::TransferWallet { amount, .. } => assert_eq!(amount, 5000),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn transfer_flow_basic() {
        match parse_ok("tf>30 >food >savings rialloco") {
            QuickAdd::TransferFlow {
                amount,
                from,
                to,
                note,
                ..
            } => {
                assert_eq!(amount, 3000);
                assert_eq!(from, "food");
                assert_eq!(to, "savings");
                assert_eq!(note.as_deref(), Some("rialloco"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn transfer_wallet_requires_two_targets() {
        assert!(matches!(
            parse_err("tw>50 @bank"),
            QuickAddError::MissingTransferTarget('@')
        ));
    }

    #[test]
    fn transfer_flow_requires_two_targets() {
        assert!(matches!(
            parse_err("tf>50 >food"),
            QuickAddError::MissingTransferTarget('>')
        ));
    }

    #[test]
    fn transfer_rejects_category_marker() {
        assert!(matches!(
            parse_err("tw>50 @bank @cash #food"),
            QuickAddError::MarkerNotAllowed { marker: '#' }
        ));
    }

    #[test]
    fn transfer_wallet_rejects_flow_marker() {
        assert!(matches!(
            parse_err("tw>50 @bank @cash >food"),
            QuickAddError::MarkerNotAllowed { marker: '>' }
        ));
    }

    #[test]
    fn transfer_flow_rejects_wallet_marker() {
        assert!(matches!(
            parse_err("tf>50 >food >savings @cash"),
            QuickAddError::MarkerNotAllowed { marker: '@' }
        ));
    }

    // -- date tokens ----------------------------------------------------------

    #[test]
    fn date_today_and_yesterday_it_and_en() {
        for token in ["oggi", "today", "OGGI"] {
            match parse_ok(&format!("10 lunch {token}")) {
                QuickAdd::Entry { date, .. } => assert_eq!(date, Some(DateSpec::Today)),
                other => panic!("unexpected {other:?}"),
            }
        }
        for token in ["ieri", "yesterday", "IERI"] {
            match parse_ok(&format!("10 lunch {token}")) {
                QuickAdd::Entry { date, .. } => assert_eq!(date, Some(DateSpec::Yesterday)),
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn date_days_ago() {
        match parse_ok("10 lunch -3d") {
            QuickAdd::Entry { date, .. } => assert_eq!(date, Some(DateSpec::DaysAgo(3))),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn date_days_ago_zero_is_invalid() {
        assert!(matches!(
            parse_err("10 lunch -0d"),
            QuickAddError::InvalidDate(_)
        ));
    }

    #[test]
    fn date_day_month_current_year() {
        match parse_ok("10 lunch 12/03") {
            QuickAdd::Entry { date, .. } => {
                assert_eq!(date, Some(DateSpec::DayMonth { day: 12, month: 3 }));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn date_full_slash_form() {
        match parse_ok("10 lunch 12/03/2024") {
            QuickAdd::Entry { date, .. } => {
                assert_eq!(
                    date,
                    Some(DateSpec::Date(
                        NaiveDate::from_ymd_opt(2024, 3, 12).unwrap()
                    ))
                );
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn date_iso_form() {
        match parse_ok("10 lunch 2024-03-12") {
            QuickAdd::Entry { date, .. } => {
                assert_eq!(
                    date,
                    Some(DateSpec::Date(
                        NaiveDate::from_ymd_opt(2024, 3, 12).unwrap()
                    ))
                );
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn date_invalid_calendar_date_is_an_error() {
        assert!(matches!(
            parse_err("10 lunch 31/02/2024"),
            QuickAddError::InvalidDate(_)
        ));
    }

    #[test]
    fn duplicate_date_token_is_an_error() {
        assert!(matches!(
            parse_err("10 lunch oggi ieri"),
            QuickAddError::DuplicateDate
        ));
    }

    #[test]
    fn date_token_removed_from_note() {
        match parse_ok("10 lunch with friends oggi") {
            QuickAdd::Entry { note, date, .. } => {
                assert_eq!(note.as_deref(), Some("lunch with friends"));
                assert_eq!(date, Some(DateSpec::Today));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    // -- note formatting --------------------------------------------------

    #[test]
    fn note_collapses_extra_whitespace() {
        match parse_ok("10   lunch    with   friends") {
            QuickAdd::Entry { note, .. } => {
                assert_eq!(note.as_deref(), Some("lunch with friends"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn no_note_is_none() {
        match parse_ok("10") {
            QuickAdd::Entry { note, .. } => assert_eq!(note, None),
            other => panic!("unexpected {other:?}"),
        }
    }

    // -- edge cases ---------------------------------------------------------

    #[test]
    fn too_many_decimals_is_invalid() {
        assert!(matches!(
            parse_err("12.345 x"),
            QuickAddError::InvalidAmount(_)
        ));
    }

    #[test]
    fn empty_input_is_an_error() {
        assert_eq!(parse_err(""), QuickAddError::EmptyInput);
        assert_eq!(parse_err("   "), QuickAddError::EmptyInput);
    }

    #[test]
    fn only_a_sign_is_missing_amount() {
        assert_eq!(parse_err("-"), QuickAddError::MissingAmount);
        assert_eq!(parse_err("+"), QuickAddError::MissingAmount);
        assert_eq!(parse_err("r"), QuickAddError::MissingAmount);
    }

    #[test]
    fn negative_zero_is_invalid() {
        assert!(matches!(
            parse_err("-0.00 x"),
            QuickAddError::InvalidAmount(_)
        ));
    }

    #[test]
    fn huge_amount_is_invalid() {
        assert!(matches!(
            parse_err("99999999999999999999 x"),
            QuickAddError::InvalidAmount(_)
        ));
    }
}
