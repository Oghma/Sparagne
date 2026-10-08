//! Engine: partial update of an existing transaction.
//!
//! The kind never changes; only the fields the patch carries do. The legs are
//! rebuilt from scratch and the balance of every target they touch, old or
//! new, moves from its old leg to its new one through
//! [`Flow::apply_leg_change`](crate::Flow::apply_leg_change), so caps and
//! non-negativity are re-checked exactly as on create.

use rusqlite::{OptionalExtension, Transaction, params};
use uuid::Uuid;

use crate::{
    CommandEnvelope, DomainError, Result, TransactionKind, TransactionPatch,
    command::explicit_person,
};

pub(super) fn update_transaction(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    transaction_id: Uuid,
    patch: &TransactionPatch,
) -> Result<()> {
    super::require_vault(tx, env.vault_id)?;
    let row = load_transaction(tx, env.vault_id, transaction_id)?;
    if row.voided {
        return Err(DomainError::InvalidCommand(
            "transaction is voided".to_string(),
        ));
    }
    if patch.is_empty() {
        return Err(DomainError::InvalidCommand("nothing to update".to_string()));
    }
    let amount = patch.amount.unwrap_or(row.amount);
    if amount <= 0 {
        return Err(DomainError::InvalidAmount("amount must be > 0".to_string()));
    }

    let old_legs = load_legs(tx, transaction_id)?;
    let (new_legs, category_id) = match row.kind {
        TransactionKind::Income | TransactionKind::Expense | TransactionKind::Refund => {
            entry_legs(tx, env, patch, &row, &old_legs, amount)?
        }
        TransactionKind::TransferWallet => {
            transfer_legs(tx, env, patch, &row, &old_legs, Target::Wallet, amount)?
        }
        TransactionKind::TransferFlow => {
            transfer_legs(tx, env, patch, &row, &old_legs, Target::Flow, amount)?
        }
    };

    apply_balance_changes(tx, env.vault_id, &old_legs, &new_legs)?;

    let note = match patch.note.as_deref() {
        Some(text) => super::normalize_note(Some(text)),
        None => row.note,
    };
    let (occurred_at, occurred_offset) = match patch.occurred_at {
        Some(when) => (when.timestamp(), when.offset().local_minus_utc()),
        None => (row.occurred_at, row.occurred_offset),
    };
    // Transfers never get here with a person (see `transfer_legs`). A blank
    // one gives the row back to whoever recorded it.
    let person = match patch.person.as_deref() {
        Some(text) => explicit_person(Some(text)).unwrap_or(&row.created_by),
        None => &row.person,
    };
    tx.execute(
        "UPDATE transactions
         SET amount = ?1, occurred_at = ?2, occurred_offset = ?3, category_id = ?4, note = ?5,
             person = ?6
         WHERE id = ?7",
        params![
            amount,
            occurred_at,
            occurred_offset,
            category_id,
            note,
            person,
            transaction_id
        ],
    )?;
    tx.execute(
        "DELETE FROM legs WHERE transaction_id = ?1",
        params![transaction_id],
    )?;
    let [first, second] = new_legs;
    super::insert_leg(
        tx,
        transaction_id,
        0,
        first.kind.as_str(),
        first.id,
        first.amount,
    )?;
    super::insert_leg(
        tx,
        transaction_id,
        1,
        second.kind.as_str(),
        second.id,
        second.amount,
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Legs of the updated transaction
// ---------------------------------------------------------------------------

/// Wallet leg (ordinal 0) and flow leg (ordinal 1) of an income, expense or
/// refund, plus the resulting category.
fn entry_legs(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    patch: &TransactionPatch,
    row: &Row,
    old_legs: &[Leg],
    amount: i64,
) -> Result<([Leg; 2], Uuid)> {
    if patch.from_id.is_some() || patch.to_id.is_some() {
        return Err(DomainError::InvalidCommand(
            "from and to are only valid on transfers".to_string(),
        ));
    }
    let wallet_id = match patch.wallet_id {
        Some(id) => id,
        None => single_target(old_legs, Target::Wallet)?,
    };
    let flow_id = match patch.flow_id {
        Some(id) => id,
        None => single_target(old_legs, Target::Flow)?,
    };
    super::resolve_wallet(tx, env.vault_id, Some(wallet_id))?;
    require_active_flow(tx, env.vault_id, flow_id)?;
    let category_id = match patch.category.as_deref() {
        Some(text) => super::resolve_category(tx, env, Some(text))?,
        None => row.category_id,
    };
    let signed = if row.kind == TransactionKind::Expense {
        -amount
    } else {
        amount
    };
    Ok((
        [
            Leg::new(Target::Wallet, wallet_id, signed),
            Leg::new(Target::Flow, flow_id, signed),
        ],
        category_id,
    ))
}

/// Source leg (ordinal 0) and destination leg (ordinal 1) of a transfer. The
/// category of a transfer never changes.
fn transfer_legs(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    patch: &TransactionPatch,
    row: &Row,
    old_legs: &[Leg],
    kind: Target,
    amount: i64,
) -> Result<([Leg; 2], Uuid)> {
    if patch.category.is_some() || patch.wallet_id.is_some() || patch.flow_id.is_some() {
        return Err(DomainError::InvalidCommand(
            "category, wallet and flow are only valid on entries".to_string(),
        ));
    }
    // A transfer moves money between two of the household's own pots: it is
    // nobody's in particular, so it stays on whoever recorded it.
    if patch.person.is_some() {
        return Err(DomainError::InvalidCommand(
            "person is only valid on entries".to_string(),
        ));
    }
    let (old_from, old_to) = endpoints(old_legs, kind)?;
    let from = patch.from_id.unwrap_or(old_from);
    let to = patch.to_id.unwrap_or(old_to);
    if from == to {
        return Err(DomainError::InvalidCommand(
            "from and to must differ".to_string(),
        ));
    }
    for id in [from, to] {
        match kind {
            Target::Wallet => {
                super::resolve_wallet(tx, env.vault_id, Some(id))?;
            }
            Target::Flow => require_active_flow(tx, env.vault_id, id)?,
        }
    }
    Ok((
        [Leg::new(kind, from, -amount), Leg::new(kind, to, amount)],
        row.category_id,
    ))
}

// ---------------------------------------------------------------------------
// Balances
// ---------------------------------------------------------------------------

/// Move every target of the old and new legs from its old amount to its new
/// one. Keyed by target, so swapping the endpoints of a transfer nets out
/// instead of double-counting.
fn apply_balance_changes(
    tx: &Transaction<'_>,
    vault_id: Uuid,
    old: &[Leg],
    new: &[Leg],
) -> Result<()> {
    let mut targets: Vec<(Target, Uuid)> = Vec::new();
    for leg in old.iter().chain(new) {
        if !targets.contains(&(leg.kind, leg.id)) {
            targets.push((leg.kind, leg.id));
        }
    }
    for (kind, id) in targets {
        let before = amount_on(old, kind, id);
        let after = amount_on(new, kind, id);
        if before == after {
            continue;
        }
        match kind {
            Target::Wallet => super::adjust_wallet(tx, id, after - before)?,
            Target::Flow => {
                let mut flow = super::load_flow(tx, vault_id, id)?;
                flow.apply_leg_change(before, after)?;
                super::save_flow(tx, &flow)?;
            }
        }
    }
    Ok(())
}

fn amount_on(legs: &[Leg], kind: Target, id: Uuid) -> i64 {
    legs.iter()
        .filter(|leg| leg.kind == kind && leg.id == id)
        .map(|leg| leg.amount)
        .sum()
}

fn require_active_flow(tx: &Transaction<'_>, vault_id: Uuid, flow_id: Uuid) -> Result<()> {
    if super::load_flow(tx, vault_id, flow_id)?.archived {
        return Err(DomainError::InvalidCommand("flow is archived".to_string()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

/// What a leg points at.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Wallet,
    Flow,
}

impl Target {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Wallet => "wallet",
            Self::Flow => "flow",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "wallet" => Ok(Self::Wallet),
            "flow" => Ok(Self::Flow),
            other => Err(DomainError::Storage(format!(
                "unknown leg target '{other}'"
            ))),
        }
    }
}

#[derive(Clone, Copy)]
struct Leg {
    kind: Target,
    id: Uuid,
    /// Signed.
    amount: i64,
}

impl Leg {
    const fn new(kind: Target, id: Uuid, amount: i64) -> Self {
        Self { kind, id, amount }
    }
}

/// The stored transaction, before the patch.
struct Row {
    kind: TransactionKind,
    amount: i64,
    occurred_at: i64,
    occurred_offset: i32,
    category_id: Uuid,
    note: Option<String>,
    voided: bool,
    created_by: String,
    person: String,
}

fn load_transaction(tx: &Transaction<'_>, vault_id: Uuid, id: Uuid) -> Result<Row> {
    let row = tx
        .query_row(
            "SELECT kind, amount, occurred_at, occurred_offset, category_id, note, voided_at,
                    created_by, person
             FROM transactions WHERE id = ?1 AND vault_id = ?2",
            params![id, vault_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i32>(3)?,
                    r.get::<_, Uuid>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<i64>>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| DomainError::NotFound("transaction".to_string()))?;
    let (
        kind,
        amount,
        occurred_at,
        occurred_offset,
        category_id,
        note,
        voided_at,
        created_by,
        person,
    ) = row;
    Ok(Row {
        kind: TransactionKind::parse(&kind)?,
        amount,
        occurred_at,
        occurred_offset,
        category_id,
        note,
        voided: voided_at.is_some(),
        created_by,
        person,
    })
}

fn load_legs(tx: &Transaction<'_>, transaction_id: Uuid) -> Result<Vec<Leg>> {
    let mut stmt = tx.prepare(
        "SELECT target_kind, target_id, amount FROM legs WHERE transaction_id = ?1 ORDER BY ordinal",
    )?;
    let rows: Vec<(String, Uuid, i64)> = stmt
        .query_map(params![transaction_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    rows.into_iter()
        .map(|(kind, id, amount)| Ok(Leg::new(Target::parse(&kind)?, id, amount)))
        .collect()
}

/// The only leg of that kind; entries have exactly one wallet and one flow.
fn single_target(legs: &[Leg], kind: Target) -> Result<Uuid> {
    let mut found = legs.iter().filter(|leg| leg.kind == kind);
    match (found.next(), found.next()) {
        (Some(leg), None) => Ok(leg.id),
        _ => Err(DomainError::Storage(format!(
            "entry has no single {} leg",
            kind.as_str()
        ))),
    }
}

/// Source and destination of a transfer, in ordinal order.
fn endpoints(legs: &[Leg], kind: Target) -> Result<(Uuid, Uuid)> {
    match legs {
        [from, to] if from.kind == kind && to.kind == kind => Ok((from.id, to.id)),
        _ => Err(DomainError::Storage(
            "transfer legs are malformed".to_string(),
        )),
    }
}
