//! Statement rows against what the vault already holds: the other side of a
//! transfer between wallets that another statement (or the user) already
//! recorded, and the rows the user entered by hand.
//!
//! Stateless: every preview matches again, so re-importing a statement finds
//! the same transactions and writes nothing new.
//!
//! The vault is read once per preview: the live incomes, expenses, refunds
//! and transfers between wallets with a leg on the statement's wallet or on
//! one of [`StatementOptions::also_check_wallets`], around the rows' dates,
//! opening balances left out, and the transactions of the rows already
//! imported left out too, so a row is never its own duplicate and the rows
//! next to it in an overlapping export are not flagged against it.
//!
//! A row and a transaction are as many days apart as their calendar dates,
//! each in its own offset. Transfers are matched first, then duplicates among
//! the rows and transactions still free; each pass pairs every row with every
//! transaction that fits it and takes the pairs closest first (fewer days,
//! then fewer seconds, then the earlier line, the earlier transaction and the
//! lower id), each row and each transaction once.

use std::collections::{HashMap, HashSet};

use rusqlite::{params_from_iter, types::Value};
use uuid::Uuid;

use super::plan::{Overrides, Planned};
use super::{
    StatementAction, StatementMatchChoice, StatementOptions, StatementRowOverride,
    StatementRowStatus,
};
use crate::{
    Command, Core, Result, TransactionKind, TransactionView,
    category::OPENING_KEY,
    query::{TRANSACTION_VIEW_COLUMNS, blob, load_legs, transaction_view},
};

/// The most days a row and the other side of its transfer are apart.
const TRANSFER_DAYS: i64 = 4;
/// The most days a row and the row the user entered for it are apart.
const DUPLICATE_DAYS: i64 = 3;
/// How far around the rows the vault is read: more than the most days a
/// match is apart, with room for any two offsets.
const WINDOW_SECONDS: i64 = 7 * 86_400;

/// What a row is matched as.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Found {
    /// The other side of a transfer: [`StatementRowStatus::MatchedTransfer`].
    Transfer,
    /// [`StatementRowStatus::PossibleDuplicate`].
    Duplicate,
}

/// Flags the planned rows that a live transaction already stands for, one
/// transaction per row at most, and settles what becomes of them with the
/// user's `on_match` (or `skip`).
pub(super) fn claim(
    core: &Core,
    vault_id: Uuid,
    options: &StatementOptions,
    overrides: &Overrides<'_>,
    planned: &mut [Planned],
) -> Result<()> {
    let wallet = options.wallet_id;
    let transfers: Vec<usize> = (0..planned.len())
        .filter(|&at| seeks_transfer(&planned[at]))
        .collect();
    let duplicates: Vec<usize> = (0..planned.len())
        .filter(|&at| seeks_duplicate(&planned[at]))
        .collect();
    let instants: Vec<i64> = transfers
        .iter()
        .chain(&duplicates)
        .filter_map(|&at| planned[at].row.occurred_at)
        .map(|at| at.timestamp())
        .collect();
    let (Some(&earliest), Some(&latest)) = (instants.iter().min(), instants.iter().max()) else {
        return Ok(());
    };
    let mut checked = vec![wallet];
    checked.extend(
        options
            .also_check_wallets
            .iter()
            .copied()
            .filter(|id| *id != wallet),
    );
    let imported: HashSet<Uuid> = planned
        .iter()
        .filter(|item| item.row.status == StatementRowStatus::AlreadyImported)
        .map(|item| item.row.command_id)
        .collect();
    let mut views = read_pool(
        core,
        vault_id,
        &checked,
        earliest - WINDOW_SECONDS,
        latest + WINDOW_SECONDS,
    )?;
    views.retain(|tx| !imported.contains(&tx.id));
    let mut pool = Pool::new(views);

    let transfers = pool.pair(planned, &transfers, |item, tx| {
        within(item, tx, TRANSFER_DAYS).filter(|_| fits_transfer(item, tx, wallet))
    });
    let paired: HashSet<usize> = transfers.iter().map(|&(row, _)| row).collect();
    let free: Vec<usize> = duplicates
        .into_iter()
        .filter(|row| !paired.contains(row))
        .collect();
    let duplicates = pool.pair(planned, &free, |item, tx| {
        within(item, tx, DUPLICATE_DAYS).filter(|_| fits_duplicate(item, tx, &checked))
    });

    for (found, pairs) in [(Found::Transfer, transfers), (Found::Duplicate, duplicates)] {
        for (row, tx) in pairs {
            let user = overrides.get(&planned[row].row.line).copied();
            settle(&mut planned[row], &pool.views[tx], found, wallet, user);
        }
    }
    Ok(())
}

/// The transactions a preview may match, by amount, each claimed once.
struct Pool {
    views: Vec<TransactionView>,
    by_amount: HashMap<i64, Vec<usize>>,
    taken: Vec<bool>,
}

impl Pool {
    fn new(views: Vec<TransactionView>) -> Self {
        let mut by_amount: HashMap<i64, Vec<usize>> = HashMap::new();
        for (at, tx) in views.iter().enumerate() {
            by_amount.entry(tx.amount).or_default().push(at);
        }
        Self {
            taken: vec![false; views.len()],
            views,
            by_amount,
        }
    }

    /// Pairs each of `rows` (positions in `planned`) with a free transaction
    /// of its amount that `fits` accepts, saying how many days apart they
    /// are; closest first, each row and transaction once. Returns `(row,
    /// transaction)` positions.
    fn pair(
        &mut self,
        planned: &[Planned],
        rows: &[usize],
        fits: impl Fn(&Planned, &TransactionView) -> Option<i64>,
    ) -> Vec<(usize, usize)> {
        let mut pairs = Vec::new();
        for &row in rows {
            let item = &planned[row];
            let Some(at) = item.row.occurred_at else {
                continue;
            };
            let candidates = self.by_amount.get(&item.row.amount).into_iter().flatten();
            for &tx in candidates.filter(|&&tx| !self.taken[tx]) {
                let candidate = &self.views[tx];
                if let Some(days) = fits(item, candidate) {
                    let then = candidate.occurred_at.timestamp();
                    let order = (
                        days,
                        (at.timestamp() - then).abs(),
                        item.row.line,
                        then,
                        candidate.id,
                    );
                    pairs.push((order, row, tx));
                }
            }
        }
        pairs.sort_unstable_by_key(|(order, ..)| *order);
        let mut done = HashSet::new();
        let mut chosen = Vec::new();
        for (_, row, tx) in pairs {
            if !self.taken[tx] && done.insert(row) {
                self.taken[tx] = true;
                chosen.push((row, tx));
            }
        }
        chosen
    }
}

/// A row the other side of a transfer may stand for: one that would be new
/// or waits for its other wallet, taken for a transfer (with or without the
/// wallet) or by its sign.
fn seeks_transfer(item: &Planned) -> bool {
    let waiting = match &item.row.status {
        StatementRowStatus::New => true,
        StatementRowStatus::Skipped { code, .. } => code == "needs_wallet",
        _ => false,
    };
    waiting
        && matches!(
            item.row.action,
            StatementAction::TransferIn { .. }
                | StatementAction::TransferOut { .. }
                | StatementAction::BySign
        )
}

/// A row a hand-entered one may stand for: a new income, expense or refund.
fn seeks_duplicate(item: &Planned) -> bool {
    item.row.status == StatementRowStatus::New
        && matches!(
            item.command,
            Some(Command::Income(_) | Command::Expense(_) | Command::Refund(_))
        )
}

/// A transfer between wallets touching the statement's `wallet` the way the
/// row says the money went, to or from the wallet it names if it names one.
fn fits_transfer(item: &Planned, tx: &TransactionView, wallet: Uuid) -> bool {
    if tx.kind != TransactionKind::TransferWallet {
        return false;
    }
    let (from, to) = (tx.from_id, tx.to_id);
    match &item.row.action {
        StatementAction::TransferOut { to_wallet_id } => {
            from == Some(wallet) && to_wallet_id.is_none_or(|other| to == Some(other))
        }
        StatementAction::TransferIn { from_wallet_id } => {
            to == Some(wallet) && from_wallet_id.is_none_or(|other| from == Some(other))
        }
        StatementAction::BySign if item.row.outflow => from == Some(wallet),
        StatementAction::BySign => to == Some(wallet),
        _ => false,
    }
}

/// An entry of the row's kind on one of the `checked` wallets.
fn fits_duplicate(item: &Planned, tx: &TransactionView, checked: &[Uuid]) -> bool {
    Some(tx.kind) == item.row.kind && tx.wallet_id.is_some_and(|id| checked.contains(&id))
}

/// The days between the row's calendar date and the transaction's, each in
/// its own offset, when they are at most `most`.
fn within(item: &Planned, tx: &TransactionView, most: i64) -> Option<i64> {
    let at = item.row.occurred_at?;
    let days = (at.date_naive() - tx.occurred_at.date_naive())
        .num_days()
        .abs();
    (days <= most).then_some(days)
}

/// What becomes of a row `tx` matched: the user's `on_match` (`Skip` when
/// the user left the row out), else move a duplicate found in another wallet
/// and skip any other. `MoveExisting` reads as `Skip` but for a duplicate in
/// another wallet. Imported anyway, the row stays what it was, with the
/// match beside it.
fn settle(
    item: &mut Planned,
    tx: &TransactionView,
    found: Found,
    wallet: Uuid,
    user: Option<&StatementRowOverride>,
) {
    let movable = found == Found::Duplicate && tx.wallet_id != Some(wallet);
    let asked = match user {
        Some(user) if user.skip => Some(StatementMatchChoice::Skip),
        Some(user) => user.on_match,
        None => None,
    };
    let choice = match asked {
        Some(StatementMatchChoice::MoveExisting) | None if movable => {
            StatementMatchChoice::MoveExisting
        }
        Some(StatementMatchChoice::ImportAnyway) => StatementMatchChoice::ImportAnyway,
        _ => StatementMatchChoice::Skip,
    };
    item.row.existing = Some(tx.clone());
    item.row.match_choice = Some(choice);
    if choice == StatementMatchChoice::ImportAnyway {
        return;
    }
    item.command = None;
    item.row.status = match found {
        Found::Transfer => {
            item.row.kind = Some(TransactionKind::TransferWallet);
            item.row.counter_wallet_id = if tx.from_id == Some(wallet) {
                tx.to_id
            } else {
                tx.from_id
            };
            StatementRowStatus::MatchedTransfer
        }
        Found::Duplicate => StatementRowStatus::PossibleDuplicate,
    };
}

/// The live transactions a row may match, oldest first: incomes, expenses,
/// refunds and transfers between wallets dated from `from` to `to` (UTC
/// seconds) with a wallet leg on one of `wallets`, opening balances left out.
///
/// The `+` before `l.target_kind` keeps the legs on their primary key, one
/// transaction at a time, as in the allocation base.
fn read_pool(
    core: &Core,
    vault_id: Uuid,
    wallets: &[Uuid],
    from: i64,
    to: i64,
) -> Result<Vec<TransactionView>> {
    const FIRST_WALLET: usize = 9;
    let placeholders: Vec<String> = (0..wallets.len())
        .map(|at| format!("?{}", at + FIRST_WALLET))
        .collect();
    let sql = format!(
        "SELECT {TRANSACTION_VIEW_COLUMNS}
         FROM transactions t JOIN categories c ON c.id = t.category_id
         WHERE t.vault_id = ?1 AND t.voided_at IS NULL
           AND t.occurred_at BETWEEN ?2 AND ?3
           AND t.kind IN (?4, ?5, ?6, ?7)
           AND NOT (c.is_system = 1 AND c.name_norm = ?8)
           AND EXISTS (
               SELECT 1 FROM legs l
               WHERE l.transaction_id = t.id AND +l.target_kind = 'wallet'
                 AND l.target_id IN ({})
           )
         ORDER BY t.occurred_at, t.id",
        placeholders.join(", ")
    );
    let mut args = vec![blob(vault_id), Value::Integer(from), Value::Integer(to)];
    args.extend(
        [
            TransactionKind::Income,
            TransactionKind::Expense,
            TransactionKind::Refund,
            TransactionKind::TransferWallet,
        ]
        .map(|kind| Value::Text(kind.as_str().to_string())),
    );
    args.push(Value::Text(OPENING_KEY.to_string()));
    args.extend(wallets.iter().map(|&id| blob(id)));
    let mut stmt = core.conn.prepare(&sql)?;
    let mut views: Vec<TransactionView> = stmt
        .query_map(params_from_iter(args), transaction_view)?
        .collect::<std::result::Result<_, _>>()?;
    load_legs(&core.conn, &mut views)?;
    Ok(views)
}
