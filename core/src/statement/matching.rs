//! Statement rows against what the vault already holds: the other side of a
//! transfer between wallets that another statement (or the user) already
//! recorded, and the rows the user entered by hand.
//!
//! Every preview matches again; what a match decided is kept in the log, by
//! the claims and moves an import writes (see [`super::plan::claim_id`] and
//! [`super::plan::move_id`]), so re-importing a statement finds the same
//! transactions and writes nothing new, and a transaction that stands for a
//! row of one export is not taken again by a row of the next.
//!
//! The vault is read once per preview, around the rows' dates: the live
//! incomes, expenses, refunds and transfers between wallets with a leg on
//! the statement's wallet or on one of
//! [`StatementOptions::also_check_wallets`], opening balances left out, and
//! the claims and moves of the log. The transactions that stand for the rows
//! already imported (their own, or the one their claim or move names) match
//! nothing.
//!
//! A row and a transaction are as many days apart as their calendar dates,
//! each in its own offset. Each amount is paired on its own, in one pass for
//! transfers and duplicates alike: as many rows as can be get a transaction,
//! and among the ways to pair that many, the one closest in all (fewest days
//! apart summed, then fewest seconds, a transfer before a duplicate on a
//! tie).

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::NaiveDate;
use rusqlite::{params, params_from_iter, types::Value};
use uuid::{Uuid, Version};

use super::plan::{Overrides, Planned};
use super::{
    StatementAction, StatementMatchChoice, StatementOptions, StatementRowOverride,
    StatementRowStatus,
};
use crate::{
    Command, Core, Result, TransactionKind, TransactionPatch, TransactionView,
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
/// One day apart weighs more than any number of seconds within a match.
const DAY_COST: i64 = 10_000_000;

/// What a row is matched as. A transfer comes first on a tie.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Found {
    /// The other side of a transfer: [`StatementRowStatus::MatchedTransfer`].
    Transfer,
    /// [`StatementRowStatus::PossibleDuplicate`].
    Duplicate,
}

/// Flags the planned rows that a live transaction already stands for, one
/// transaction per row at most, and settles what becomes of them with the
/// user's `on_match` (or `skip`). `standing` holds the transactions that
/// stand for the rows already imported, which match nothing.
pub(super) fn claim(
    core: &Core,
    vault_id: Uuid,
    options: &StatementOptions,
    overrides: &Overrides<'_>,
    standing: &HashSet<Uuid>,
    planned: &mut [Planned],
) -> Result<()> {
    let wallet = options.wallet_id;
    let seekers: Vec<usize> = (0..planned.len())
        .filter(|&at| seeks_transfer(&planned[at]) || seeks_duplicate(&planned[at]))
        .collect();
    let instants: Vec<i64> = seekers
        .iter()
        .filter_map(|&at| planned[at].row.occurred_at)
        .map(|at| at.timestamp())
        .collect();
    let (Some(&earliest), Some(&latest)) = (instants.iter().min(), instants.iter().max()) else {
        return Ok(());
    };
    let mut also: Vec<Uuid> = Vec::new();
    for &id in &options.also_check_wallets {
        if id != wallet && !also.contains(&id) {
            also.push(id);
        }
    }
    let mut checked = vec![wallet];
    checked.extend(&also);
    let claims = read_claims(core, vault_id)?;
    let candidates: Vec<Candidate> = read_pool(
        core,
        vault_id,
        &checked,
        earliest - WINDOW_SECONDS,
        latest + WINDOW_SECONDS,
    )?
    .into_iter()
    .filter(|(view, _)| !standing.contains(&view.id))
    .map(|(view, imported)| Candidate {
        claimed: claims.entries.contains(&view.id),
        sides: claims.sides.get(&view.id).cloned().unwrap_or_default(),
        imported,
        view,
    })
    .collect();
    let dates = planned
        .iter()
        .filter_map(|item| item.row.occurred_at)
        .map(|at| at.date_naive());
    let fit = Fit {
        wallet,
        also: &also,
        span: dates.clone().min().zip(dates.max()),
        since: options.since,
    };

    let mut by_amount: HashMap<i64, Vec<usize>> = HashMap::new();
    for (at, tx) in candidates.iter().enumerate() {
        by_amount.entry(tx.view.amount).or_default().push(at);
    }
    let mut groups: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for &row in &seekers {
        groups.entry(planned[row].row.amount).or_default().push(row);
    }
    let mut pairs = Vec::new();
    for (amount, mut rows) in groups {
        let Some(txs) = by_amount.get(&amount) else {
            continue;
        };
        rows.sort_by_key(|&row| (planned[row].row.occurred_at, planned[row].row.line));
        let edges: Vec<Vec<Edge>> = rows
            .iter()
            .map(|&row| fit.edges(&planned[row], txs, &candidates))
            .collect();
        for (local, tx) in pair(&edges, txs.len()).into_iter().enumerate() {
            if let Some(edge) = tx {
                pairs.push((rows[local], txs[edge.tx], edge.found));
            }
        }
    }
    for (row, tx, found) in pairs {
        let user = overrides.get(&planned[row].row.line).copied();
        settle(&mut planned[row], &candidates[tx].view, found, wallet, user);
    }
    Ok(())
}

/// A transaction a row may match.
struct Candidate {
    view: TransactionView,
    /// Written by an import under its own command's id, a UUID v5: a row of
    /// an earlier statement, or of any import that names its commands after
    /// what it imports. A row typed by hand, or one a recurring template
    /// recorded, has an id the app minted (v7).
    imported: bool,
    /// An entry a claim or a move of the log names: it stands for a row of
    /// some statement.
    claimed: bool,
    /// The wallets whose side of this transfer a claim of the log names.
    sides: Vec<Uuid>,
}

/// One transaction a row may be paired with.
#[derive(Clone, Copy, Debug)]
struct Edge {
    /// Position among the transactions of the row's amount.
    tx: usize,
    /// Days apart, then seconds, then a transfer before a duplicate.
    cost: i64,
    found: Found,
}

/// Which transactions may stand for a row of the statement.
struct Fit<'a> {
    /// The statement's wallet.
    wallet: Uuid,
    /// The other wallets checked for rows typed by hand.
    also: &'a [Uuid],
    /// The first and last day of the file's readable rows.
    span: Option<(NaiveDate, NaiveDate)>,
    since: Option<NaiveDate>,
}

impl Fit<'_> {
    /// The transactions among `txs` (positions in `candidates`) that may
    /// stand for `item`, closest first.
    fn edges(&self, item: &Planned, txs: &[usize], candidates: &[Candidate]) -> Vec<Edge> {
        let Some(at) = item.row.occurred_at else {
            return Vec::new();
        };
        let mut edges: Vec<(Edge, i64, Uuid)> = Vec::new();
        for (local, &tx) in txs.iter().enumerate() {
            let candidate = &candidates[tx];
            let then = candidate.view.occurred_at;
            let days = (at.date_naive() - then.date_naive()).num_days().abs();
            let found = if candidate.view.kind == TransactionKind::TransferWallet {
                let fits = seeks_transfer(item)
                    && days <= TRANSFER_DAYS
                    && self.fits_transfer(item, candidate);
                fits.then_some(Found::Transfer)
            } else {
                let fits = seeks_duplicate(item)
                    && days <= DUPLICATE_DAYS
                    && self.fits_duplicate(item, candidate);
                fits.then_some(Found::Duplicate)
            };
            if let Some(found) = found {
                let seconds = (at.timestamp() - then.timestamp()).abs();
                let cost = days * DAY_COST + seconds * 2 + i64::from(found == Found::Duplicate);
                let edge = Edge {
                    tx: local,
                    cost,
                    found,
                };
                edges.push((edge, then.timestamp(), candidate.view.id));
            }
        }
        edges.sort_by_key(|(edge, then, id)| (edge.cost, *then, *id));
        edges.into_iter().map(|(edge, ..)| edge).collect()
    }

    /// A transfer between wallets touching the statement's wallet the way
    /// the row says the money went, to or from the wallet it names if it
    /// names one, whose side on the statement's wallet no claim names.
    fn fits_transfer(&self, item: &Planned, tx: &Candidate) -> bool {
        let wallet = self.wallet;
        if tx.sides.contains(&wallet) {
            return false;
        }
        let (from, to) = (tx.view.from_id, tx.view.to_id);
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

    /// An entry moving the money the row's way (an expense for money going
    /// out, an income or a refund for money coming in), on the statement's
    /// wallet or on one also checked.
    ///
    /// On the statement's wallet a row typed by hand that nothing claimed is
    /// always a candidate (a row typed on the 30th may be booked on the 1st);
    /// one a claim or a move names, or one an import wrote, only when it is
    /// dated within the file's own days: it stands for a row of another
    /// export, unless this file holds that row again with other ids. On a
    /// wallet also checked, an entry a claim or a move names stands for a
    /// row of another statement already; an imported one is the user's own
    /// sheet, and is a candidate from [`StatementOptions::since`] on.
    fn fits_duplicate(&self, item: &Planned, tx: &Candidate) -> bool {
        let same_way = match item.row.kind {
            Some(TransactionKind::Expense) => tx.view.kind == TransactionKind::Expense,
            Some(TransactionKind::Income | TransactionKind::Refund) => matches!(
                tx.view.kind,
                TransactionKind::Income | TransactionKind::Refund
            ),
            _ => false,
        };
        let day = tx.view.occurred_at.date_naive();
        same_way
            && match tx.view.wallet_id {
                Some(id) if id == self.wallet => {
                    !(tx.claimed || tx.imported)
                        || self
                            .span
                            .is_some_and(|(first, last)| first <= day && day <= last)
                }
                Some(id) if self.also.contains(&id) => {
                    !tx.claimed && self.since.is_none_or(|since| since <= day)
                }
                _ => false,
            }
    }
}

/// Pairs as many rows as can be with a transaction each, and among the ways
/// to pair that many the cheapest in all: successive cheapest augmenting
/// paths over `edges` (per row, closest first), rows in their order. Returns
/// the edge each row got.
fn pair(edges: &[Vec<Edge>], txs: usize) -> Vec<Option<Edge>> {
    let rows = edges.len();
    let mut row_edge: Vec<Option<Edge>> = vec![None; rows];
    let mut tx_row: Vec<Option<usize>> = vec![None; txs];
    loop {
        // The cheapest way to reach every row from a row still free, giving
        // each row passed through the next transaction on the way.
        let mut dist: Vec<Option<i64>> = row_edge
            .iter()
            .map(|edge| edge.is_none().then_some(0))
            .collect();
        let mut via: Vec<Option<(usize, Edge)>> = vec![None; rows];
        for _ in 0..=rows {
            let mut changed = false;
            for row in 0..rows {
                let Some(here) = dist[row] else {
                    continue;
                };
                for edge in &edges[row] {
                    let Some(other) = tx_row[edge.tx] else {
                        continue;
                    };
                    if other == row {
                        continue;
                    }
                    let held = row_edge[other].map_or(0, |held| held.cost);
                    let there = here + edge.cost - held;
                    if dist[other].is_none_or(|old| there < old) {
                        dist[other] = Some(there);
                        via[other] = Some((row, *edge));
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut best: Option<(i64, usize, Edge)> = None;
        for row in 0..rows {
            let Some(here) = dist[row] else {
                continue;
            };
            for edge in edges[row].iter().filter(|edge| tx_row[edge.tx].is_none()) {
                let total = here + edge.cost;
                if best.is_none_or(|(cheapest, ..)| total < cheapest) {
                    best = Some((total, row, *edge));
                }
            }
        }
        let Some((_, mut row, mut edge)) = best else {
            return row_edge;
        };
        loop {
            row_edge[row] = Some(edge);
            tx_row[edge.tx] = Some(row);
            match via[row] {
                Some((previous, through)) => {
                    row = previous;
                    edge = through;
                }
                None => break,
            }
        }
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

/// What the claims and moves of the log name.
#[derive(Default)]
struct Claims {
    /// Entries a claim or a move names.
    entries: HashSet<Uuid>,
    /// Transfer -> the wallets whose side a claim names.
    sides: HashMap<Uuid, Vec<Uuid>>,
}

/// The transaction and what of it a command claims, when it is a statement
/// import's claim or move: an `UpdateTransaction` under a UUID v5 whose patch
/// names one wallet (`wallet_id`, `from_id` or `to_id`) and nothing else.
/// Nothing else writes that shape: the app mints v7 ids, and a script that
/// relabels rows under v5 ids patches their category.
fn claimed_by(id: Uuid, command: &Command) -> Option<(Uuid, Claimed)> {
    if id.get_version() != Some(Version::Sha1) {
        return None;
    }
    let Command::UpdateTransaction {
        transaction_id,
        patch,
    } = command
    else {
        return None;
    };
    let only = |expected: TransactionPatch| expected == *patch;
    let claimed = match (patch.wallet_id, patch.from_id, patch.to_id) {
        (Some(wallet_id), None, None)
            if only(TransactionPatch {
                wallet_id: Some(wallet_id),
                ..TransactionPatch::default()
            }) =>
        {
            Claimed::Entry
        }
        (None, Some(from_id), None)
            if only(TransactionPatch {
                from_id: Some(from_id),
                ..TransactionPatch::default()
            }) =>
        {
            Claimed::Side(from_id)
        }
        (None, None, Some(to_id))
            if only(TransactionPatch {
                to_id: Some(to_id),
                ..TransactionPatch::default()
            }) =>
        {
            Claimed::Side(to_id)
        }
        _ => return None,
    };
    Some((*transaction_id, claimed))
}

/// What a claim names of a transaction.
enum Claimed {
    /// The whole entry.
    Entry,
    /// The side of a transfer on this wallet.
    Side(Uuid),
}

/// Every claim and move of the vault's log, whatever became of it: a claim
/// the server refused still counts, as a row refused on its own id still
/// reads as imported.
fn read_claims(core: &Core, vault_id: Uuid) -> Result<Claims> {
    // The version of a UUID is its 13th hex digit.
    let mut stmt = core.conn.prepare(
        "SELECT id, payload FROM commands
         WHERE vault_id = ?1 AND kind = 'update_transaction'
           AND substr(hex(id), 13, 1) = '5'",
    )?;
    let rows = stmt.query_map(params![vault_id], |r| {
        Ok((r.get::<_, Uuid>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut claims = Claims::default();
    for row in rows {
        let (id, payload) = row?;
        let Ok(command) = serde_json::from_str::<Command>(&payload) else {
            continue;
        };
        match claimed_by(id, &command) {
            Some((tx, Claimed::Entry)) => {
                claims.entries.insert(tx);
            }
            Some((tx, Claimed::Side(wallet))) => claims.sides.entry(tx).or_default().push(wallet),
            None => {}
        }
    }
    Ok(claims)
}

/// The live transactions a row may match, oldest first, each with whether
/// an import wrote it under its own command's id: incomes, expenses, refunds
/// and transfers between wallets dated from `from` to `to` (UTC seconds)
/// with a wallet leg on one of `wallets`, opening balances left out.
///
/// The `+` before `l.target_kind` keeps the legs on their primary key, one
/// transaction at a time, as in the allocation base.
fn read_pool(
    core: &Core,
    vault_id: Uuid,
    wallets: &[Uuid],
    from: i64,
    to: i64,
) -> Result<Vec<(TransactionView, bool)>> {
    const FIRST_WALLET: usize = 9;
    // Right after the view's own columns.
    let own_id = TRANSACTION_VIEW_COLUMNS.split(',').count();
    let placeholders: Vec<String> = (0..wallets.len())
        .map(|at| format!("?{}", at + FIRST_WALLET))
        .collect();
    let sql = format!(
        "SELECT {TRANSACTION_VIEW_COLUMNS}, t.id = t.command_id
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
    let (mut views, own): (Vec<TransactionView>, Vec<bool>) = stmt
        .query_map(params_from_iter(args), |r| {
            Ok((transaction_view(r)?, r.get::<_, bool>(own_id)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .unzip();
    load_legs(&core.conn, &mut views)?;
    Ok(views
        .into_iter()
        .zip(own)
        .map(|(view, own)| {
            let imported = own && view.id.get_version() == Some(Version::Sha1);
            (view, imported)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(tx: usize, days: i64) -> Edge {
        Edge {
            tx,
            cost: days * DAY_COST,
            found: Found::Duplicate,
        }
    }

    fn txs_of(pairs: &[Option<Edge>]) -> Vec<Option<usize>> {
        pairs.iter().map(|edge| edge.map(|edge| edge.tx)).collect()
    }

    #[test]
    fn as_many_pairs_as_can_be_then_the_closest() {
        // Rows on the 10th, 11th and 12th; transactions on the 11th (0) and
        // the 12th (1): the 11th and the 12th pair with their own day.
        let edges = vec![
            vec![edge(0, 1), edge(1, 2)],
            vec![edge(0, 0), edge(1, 1)],
            vec![edge(1, 0), edge(0, 1)],
        ];
        assert_eq!(txs_of(&pair(&edges, 2)), [None, Some(0), Some(1)]);
        // Rows on the 10th and 13th, transactions on the 11th (0) and the
        // 8th (1): both rows pair, though the 10th is closer to the 11th.
        let edges = vec![vec![edge(0, 1), edge(1, 2)], vec![edge(0, 2)]];
        assert_eq!(txs_of(&pair(&edges, 2)), [Some(1), Some(0)]);
    }
}
