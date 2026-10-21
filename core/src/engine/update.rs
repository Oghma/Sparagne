//! Partial update of an existing transaction, and the legs every writer
//! shares. The kind never changes; only the fields the patch carries do. The
//! legs are rebuilt from scratch and every target they touch, old or new,
//! moves from its old leg to its new one through
//! [`Flow::apply_leg_change`](crate::Flow::apply_leg_change), so caps and
//! non-negativity are re-checked exactly as on create.

use rusqlite::{OptionalExtension, Transaction, params};
use uuid::Uuid;

use super::{check_amount, invalid, require_active_flow};

use crate::{
    CommandEnvelope, DomainError, Result, TargetKind, TransactionKind, TransactionPatch,
    command::explicit_person, query::LEGS_SQL,
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
        return Err(invalid("transaction is voided"));
    }
    if patch.is_empty() {
        return Err(invalid("nothing to update"));
    }
    let amount = patch.amount.unwrap_or(row.amount);
    check_amount(amount)?;

    let old_legs = load_legs(tx, transaction_id)?;
    let (new_legs, category_id) = match row.kind {
        TransactionKind::Income | TransactionKind::Expense | TransactionKind::Refund => {
            entry_legs(tx, env, patch, &row, &old_legs, amount)?
        }
        TransactionKind::TransferWallet => {
            transfer_legs(tx, env, patch, &row, &old_legs, TargetKind::Wallet, amount)?
        }
        TransactionKind::TransferFlow => {
            transfer_legs(tx, env, patch, &row, &old_legs, TargetKind::Flow, amount)?
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
    super::insert_legs(tx, transaction_id, &new_legs)
}

/// The new legs of an income, expense or refund, and its category.
fn entry_legs(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    patch: &TransactionPatch,
    row: &Row,
    old_legs: &[Leg],
    amount: i64,
) -> Result<([Leg; 2], Uuid)> {
    if patch.from_id.is_some() || patch.to_id.is_some() {
        return Err(invalid("from and to are only valid on transfers"));
    }
    let wallet_id = match patch.wallet_id {
        Some(id) => id,
        None => single_target(old_legs, TargetKind::Wallet)?,
    };
    let flow_id = match patch.flow_id {
        Some(id) => id,
        None => single_target(old_legs, TargetKind::Flow)?,
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
    Ok((Leg::entry(wallet_id, flow_id, signed), category_id))
}

/// The new legs of a transfer, whose category never changes.
fn transfer_legs(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    patch: &TransactionPatch,
    row: &Row,
    old_legs: &[Leg],
    kind: TargetKind,
    amount: i64,
) -> Result<([Leg; 2], Uuid)> {
    if patch.category.is_some() || patch.wallet_id.is_some() || patch.flow_id.is_some() {
        return Err(invalid(
            "category, wallet and flow are only valid on entries",
        ));
    }
    // A transfer moves money between two of the household's own pots: it is
    // nobody's in particular, so it stays on whoever recorded it.
    if patch.person.is_some() {
        return Err(invalid("person is only valid on entries"));
    }
    let (old_from, old_to) = endpoints(old_legs, kind)?;
    let from = patch.from_id.unwrap_or(old_from);
    let to = patch.to_id.unwrap_or(old_to);
    if from == to {
        return Err(invalid("from and to must differ"));
    }
    for id in [from, to] {
        match kind {
            TargetKind::Wallet => {
                super::resolve_wallet(tx, env.vault_id, Some(id))?;
            }
            TargetKind::Flow => require_active_flow(tx, env.vault_id, id)?,
        }
    }
    Ok((Leg::transfer(kind, from, to, amount), row.category_id))
}

/// Move every target of the old and new legs from its old amount to its new
/// one. Keyed by target, so swapping the endpoints of a transfer nets out
/// instead of double-counting.
fn apply_balance_changes(
    tx: &Transaction<'_>,
    vault_id: Uuid,
    old: &[Leg],
    new: &[Leg],
) -> Result<()> {
    let mut targets: Vec<(TargetKind, Uuid)> = Vec::new();
    for leg in old.iter().chain(new) {
        if !targets.contains(&(leg.kind, leg.id)) {
            targets.push((leg.kind, leg.id));
        }
    }
    for (kind, id) in targets {
        let before = amount_on(old, kind, id);
        let after = amount_on(new, kind, id);
        if before != after {
            super::move_leg(tx, vault_id, kind, id, before, after)?;
        }
    }
    Ok(())
}

fn amount_on(legs: &[Leg], kind: TargetKind, id: Uuid) -> i64 {
    legs.iter()
        .filter(|leg| leg.kind == kind && leg.id == id)
        .map(|leg| leg.amount)
        .sum()
}

impl TargetKind {
    /// `legs.target_kind` of this side.
    pub(crate) const fn as_str(self) -> &'static str {
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

/// One side of a transaction: a signed amount on a wallet or a flow.
#[derive(Clone, Copy)]
pub(super) struct Leg {
    pub(super) kind: TargetKind,
    pub(super) id: Uuid,
    pub(super) amount: i64,
}

impl Leg {
    const fn new(kind: TargetKind, id: Uuid, amount: i64) -> Self {
        Self { kind, id, amount }
    }

    /// Wallet leg (ordinal 0), then flow leg, the same signed amount on both.
    pub(super) const fn entry(wallet_id: Uuid, flow_id: Uuid, signed: i64) -> [Self; 2] {
        [
            Self::new(TargetKind::Wallet, wallet_id, signed),
            Self::new(TargetKind::Flow, flow_id, signed),
        ]
    }

    /// Source leg (ordinal 0), then destination leg.
    pub(super) const fn transfer(kind: TargetKind, from: Uuid, to: Uuid, amount: i64) -> [Self; 2] {
        [Self::new(kind, from, -amount), Self::new(kind, to, amount)]
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

pub(super) fn load_legs(tx: &Transaction<'_>, transaction_id: Uuid) -> Result<Vec<Leg>> {
    let mut stmt = tx.prepare(LEGS_SQL)?;
    let rows: Vec<(String, Uuid, i64)> = stmt
        .query_map(params![transaction_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    rows.into_iter()
        .map(|(kind, id, amount)| Ok(Leg::new(TargetKind::parse(&kind)?, id, amount)))
        .collect()
}

/// The only leg of that kind; entries have exactly one wallet and one flow.
fn single_target(legs: &[Leg], kind: TargetKind) -> Result<Uuid> {
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
fn endpoints(legs: &[Leg], kind: TargetKind) -> Result<(Uuid, Uuid)> {
    match legs {
        [from, to] if from.kind == kind && to.kind == kind => Ok((from.id, to.id)),
        _ => Err(DomainError::Storage(
            "transfer legs are malformed".to_string(),
        )),
    }
}
