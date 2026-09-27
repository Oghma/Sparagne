//! Engine: wallet, flow and category management commands, plus the read
//! side that supports them (merge preview, alias list, similar names).

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    CategoryView, CommandEnvelope, Core, DomainError, FlowMode, Result,
    category::{normalize_category_display, normalize_category_key, validate_category_name},
    flow::UNALLOCATED_NAME,
};

// ---------------------------------------------------------------------------
// Wallets
// ---------------------------------------------------------------------------

pub(super) fn rename_wallet(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    wallet_id: Uuid,
    name: &str,
) -> Result<()> {
    load_wallet(tx, env.vault_id, wallet_id)?;
    let name = super::normalize_name(name, "wallet")?;
    let taken: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM wallets
         WHERE vault_id = ?1 AND lower(name) = lower(?2) AND id <> ?3)",
        params![env.vault_id, name, wallet_id],
        |r| r.get(0),
    )?;
    if taken {
        return Err(DomainError::AlreadyExists(name));
    }
    tx.execute(
        "UPDATE wallets SET name = ?1 WHERE id = ?2",
        params![name, wallet_id],
    )?;
    Ok(())
}

pub(super) fn set_wallet_archived(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    wallet_id: Uuid,
    archived: bool,
) -> Result<()> {
    let (balance, current) = load_wallet(tx, env.vault_id, wallet_id)?;
    if current == archived {
        return Err(already_in_state("wallet", archived));
    }
    if archived && balance != 0 {
        return Err(DomainError::InvalidCommand(
            "wallet has a non-zero balance".to_string(),
        ));
    }
    tx.execute(
        "UPDATE wallets SET archived = ?1 WHERE id = ?2",
        params![archived, wallet_id],
    )?;
    Ok(())
}

/// `(balance, archived)` of a wallet of the vault.
fn load_wallet(tx: &Transaction<'_>, vault_id: Uuid, wallet_id: Uuid) -> Result<(i64, bool)> {
    tx.query_row(
        "SELECT balance, archived FROM wallets WHERE id = ?1 AND vault_id = ?2",
        params![wallet_id, vault_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()?
    .ok_or_else(|| DomainError::NotFound("wallet".to_string()))
}

fn already_in_state(what: &str, archived: bool) -> DomainError {
    let state = if archived { "archived" } else { "active" };
    DomainError::InvalidCommand(format!("{what} is already {state}"))
}

// ---------------------------------------------------------------------------
// Flows
// ---------------------------------------------------------------------------

pub(super) fn update_flow(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    flow_id: Uuid,
    name: Option<&str>,
    mode: Option<FlowMode>,
    allow_negative: Option<bool>,
) -> Result<()> {
    if name.is_none() && mode.is_none() && allow_negative.is_none() {
        return Err(DomainError::InvalidCommand("nothing to update".to_string()));
    }
    let flow = super::load_flow(tx, env.vault_id, flow_id)?;
    if flow.is_unallocated {
        return Err(DomainError::InvalidFlow(
            "the Unallocated flow cannot be updated".to_string(),
        ));
    }

    let new_name = match name {
        None => flow.name.clone(),
        Some(raw) => {
            let candidate = super::normalize_name(raw, "flow")?;
            if candidate.eq_ignore_ascii_case(UNALLOCATED_NAME) {
                return Err(DomainError::InvalidFlow(
                    "flow name is reserved".to_string(),
                ));
            }
            let taken: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM flows
                 WHERE vault_id = ?1 AND lower(name) = lower(?2) AND id <> ?3)",
                params![env.vault_id, candidate, flow_id],
                |r| r.get(0),
            )?;
            if taken {
                return Err(DomainError::AlreadyExists(candidate));
            }
            candidate
        }
    };

    let (cap, income_total) = match mode {
        None => (flow.cap, flow.income_total),
        Some(mode) => {
            mode.validate()?;
            match mode {
                FlowMode::Unlimited => (None, None),
                FlowMode::NetCapped { cap } => {
                    if flow.balance > cap {
                        return Err(DomainError::MaxBalanceReached(flow.name.clone()));
                    }
                    (Some(cap), None)
                }
                FlowMode::IncomeCapped { cap } => {
                    let total = recompute_income_total(tx, env.vault_id, flow_id)?;
                    if total > cap {
                        return Err(DomainError::MaxBalanceReached(flow.name.clone()));
                    }
                    (Some(cap), Some(total))
                }
            }
        }
    };

    let allow_negative = match allow_negative {
        None => flow.allow_negative,
        Some(false) => {
            if flow.balance < 0 {
                return Err(DomainError::InsufficientFunds(flow.name.clone()));
            }
            false
        }
        Some(true) => true,
    };

    tx.execute(
        "UPDATE flows SET name = ?1, cap = ?2, income_total = ?3, allow_negative = ?4 WHERE id = ?5",
        params![new_name, cap, income_total, allow_negative, flow_id],
    )?;
    Ok(())
}

pub(super) fn set_flow_archived(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    flow_id: Uuid,
    archived: bool,
) -> Result<()> {
    let flow = super::load_flow(tx, env.vault_id, flow_id)?;
    if flow.is_unallocated {
        return Err(DomainError::InvalidFlow(
            "the Unallocated flow cannot be archived".to_string(),
        ));
    }
    if flow.archived == archived {
        return Err(already_in_state("flow", archived));
    }
    if archived && flow.balance != 0 {
        return Err(DomainError::InvalidFlow(
            "flow has a non-zero balance".to_string(),
        ));
    }
    tx.execute(
        "UPDATE flows SET archived = ?1 WHERE id = ?2",
        params![archived, flow_id],
    )?;
    Ok(())
}

/// Cumulative income of a flow: positive legs of its non-voided transactions.
fn recompute_income_total(tx: &Transaction<'_>, vault_id: Uuid, flow_id: Uuid) -> Result<i64> {
    Ok(tx.query_row(
        "SELECT COALESCE(SUM(l.amount), 0) FROM legs l
         JOIN transactions t ON t.id = l.transaction_id
         WHERE l.target_kind = 'flow' AND l.target_id = ?1 AND l.amount > 0
           AND t.vault_id = ?2 AND t.voided_at IS NULL",
        params![flow_id, vault_id],
        |r| r.get(0),
    )?)
}

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

pub(super) fn rename_category(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    category_id: Uuid,
    name: &str,
) -> Result<()> {
    let category = load_category(tx, env.vault_id, category_id)?;
    if category.is_system {
        return Err(DomainError::InvalidName("system category".to_string()));
    }
    let (display, key) = validate_category_name(name)?;
    let taken: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM categories
                       WHERE vault_id = ?1 AND name_norm = ?2 AND id <> ?3)
             OR EXISTS(SELECT 1 FROM category_aliases
                       WHERE vault_id = ?1 AND alias_norm = ?2 AND category_id <> ?3)",
        params![env.vault_id, key, category_id],
        |r| r.get(0),
    )?;
    if taken {
        return Err(DomainError::AlreadyExists(display));
    }
    // The new name may be one of this category's own aliases; the alias then
    // becomes the name and the row goes away.
    tx.execute(
        "DELETE FROM category_aliases WHERE vault_id = ?1 AND category_id = ?2 AND alias_norm = ?3",
        params![env.vault_id, category_id, key],
    )?;
    tx.execute(
        "UPDATE categories SET name = ?1, name_norm = ?2 WHERE id = ?3",
        params![display, key, category_id],
    )?;
    Ok(())
}

pub(super) fn set_category_archived(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    category_id: Uuid,
    archived: bool,
) -> Result<()> {
    let category = load_category(tx, env.vault_id, category_id)?;
    if category.is_system {
        return Err(DomainError::InvalidName("system category".to_string()));
    }
    if category.archived == archived {
        return Err(already_in_state("category", archived));
    }
    tx.execute(
        "UPDATE categories SET archived = ?1 WHERE id = ?2",
        params![archived, category_id],
    )?;
    Ok(())
}

/// Returns the alias id (derived from the command id).
pub(super) fn add_alias(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    category_id: Uuid,
    alias: &str,
) -> Result<Uuid> {
    let category = load_category(tx, env.vault_id, category_id)?;
    if category.is_system {
        return Err(DomainError::InvalidName("system category".to_string()));
    }
    if category.archived {
        return Err(DomainError::InvalidName("category is archived".to_string()));
    }
    let display = normalize_category_display(alias)?;
    let key = normalize_category_key(&display)?;
    if super::category_key_taken(tx, env.vault_id, &key)? {
        return Err(DomainError::AlreadyExists(display));
    }
    tx.execute(
        "INSERT INTO category_aliases (id, vault_id, category_id, alias, alias_norm)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![env.id, env.vault_id, category_id, display, key],
    )?;
    Ok(env.id)
}

pub(super) fn remove_alias(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    category_id: Uuid,
    alias: &str,
) -> Result<()> {
    let key = normalize_category_key(alias)?;
    let removed = tx.execute(
        "DELETE FROM category_aliases WHERE vault_id = ?1 AND category_id = ?2 AND alias_norm = ?3",
        params![env.vault_id, category_id, key],
    )?;
    if removed == 0 {
        return Err(DomainError::NotFound("alias".to_string()));
    }
    Ok(())
}

pub(super) fn merge_category(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<()> {
    let conflicts = merge_conflicts(tx, env.vault_id, source_id, target_id)?;
    if !conflicts.is_empty() {
        let kinds = conflicts
            .iter()
            .map(|c| c.kind.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(DomainError::InvalidCommand(format!(
            "merge refused: {kinds}"
        )));
    }
    let source = load_category(tx, env.vault_id, source_id)?;
    let target = load_category(tx, env.vault_id, target_id)?;

    tx.execute(
        "UPDATE transactions SET category_id = ?1 WHERE vault_id = ?2 AND category_id = ?3",
        params![target_id, env.vault_id, source_id],
    )?;
    tx.execute(
        "UPDATE category_aliases SET category_id = ?1 WHERE vault_id = ?2 AND category_id = ?3",
        params![target_id, env.vault_id, source_id],
    )?;
    // The source name survives as an alias of the target, so the old name keeps
    // resolving. System categories take no aliases.
    if !target.is_system && source.name_norm != target.name_norm {
        tx.execute(
            "INSERT INTO category_aliases (id, vault_id, category_id, alias, alias_norm)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                super::derived_id(env.id, "alias"),
                env.vault_id,
                target_id,
                source.name,
                source.name_norm,
            ],
        )?;
    }
    // Nothing references the source any more: its key must become free.
    tx.execute("DELETE FROM categories WHERE id = ?1", params![source_id])?;
    Ok(())
}

/// A category row of the projection.
struct CategoryRow {
    name: String,
    name_norm: String,
    is_system: bool,
    archived: bool,
}

fn load_category(conn: &Connection, vault_id: Uuid, category_id: Uuid) -> Result<CategoryRow> {
    conn.query_row(
        "SELECT name, name_norm, is_system, archived FROM categories
         WHERE id = ?1 AND vault_id = ?2",
        params![category_id, vault_id],
        |r| {
            Ok(CategoryRow {
                name: r.get(0)?,
                name_norm: r.get(1)?,
                is_system: r.get(2)?,
                archived: r.get(3)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| DomainError::NotFound("category".to_string()))
}

// ---------------------------------------------------------------------------
// Read side
// ---------------------------------------------------------------------------

/// Why a merge cannot go through.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum MergeConflictKind {
    /// Source and target are the same category.
    SameCategory,
    /// System categories cannot be merged away.
    SourceSystem,
    /// The target is archived.
    TargetArchived,
}

impl MergeConflictKind {
    /// Stable snake_case tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SameCategory => "same_category",
            Self::SourceSystem => "source_system",
            Self::TargetArchived => "target_archived",
        }
    }
}

/// One reason a merge is refused, with the name of the offending category.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MergeConflict {
    pub kind: MergeConflictKind,
    pub value: String,
}

/// Outcome of [`Core::preview_merge`].
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MergePreview {
    pub ok: bool,
    pub conflicts: Vec<MergeConflict>,
}

/// An alias of a category.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AliasView {
    pub id: Uuid,
    pub category_id: Uuid,
    pub alias: String,
}

fn merge_conflicts(
    conn: &Connection,
    vault_id: Uuid,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<Vec<MergeConflict>> {
    let source = load_category(conn, vault_id, source_id)?;
    let target = load_category(conn, vault_id, target_id)?;
    let mut conflicts = Vec::new();
    if source_id == target_id {
        conflicts.push(MergeConflict {
            kind: MergeConflictKind::SameCategory,
            value: source.name.clone(),
        });
    }
    if source.is_system {
        conflicts.push(MergeConflict {
            kind: MergeConflictKind::SourceSystem,
            value: source.name.clone(),
        });
    }
    if target.archived {
        conflicts.push(MergeConflict {
            kind: MergeConflictKind::TargetArchived,
            value: target.name,
        });
    }
    Ok(conflicts)
}

impl Core {
    /// What `MergeCategory` would refuse, without changing anything.
    pub fn preview_merge(
        &self,
        vault_id: Uuid,
        source_id: Uuid,
        target_id: Uuid,
    ) -> Result<MergePreview> {
        let conflicts = merge_conflicts(&self.conn, vault_id, source_id, target_id)?;
        Ok(MergePreview {
            ok: conflicts.is_empty(),
            conflicts,
        })
    }

    /// Every alias of the vault, ordered by alias.
    pub fn aliases(&self, vault_id: Uuid) -> Result<Vec<AliasView>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, category_id, alias FROM category_aliases
             WHERE vault_id = ?1 ORDER BY lower(alias), alias",
        )?;
        let rows = stmt
            .query_map(params![vault_id], |r| {
                Ok(AliasView {
                    id: r.get(0)?,
                    category_id: r.get(1)?,
                    alias: r.get(2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Active, non-system categories whose key is close to `name`, nearest
    /// first. A suggestion: creating a similar name is never blocked.
    pub fn similar_categories(&self, vault_id: Uuid, name: &str) -> Result<Vec<CategoryView>> {
        let key = normalize_category_key(name)?;
        let threshold = if key.chars().count() <= 6 { 1 } else { 2 };
        let mut stmt = self.conn.prepare(
            "SELECT id, name, name_norm FROM categories
             WHERE vault_id = ?1 AND is_system = 0 AND archived = 0",
        )?;
        let rows = stmt
            .query_map(params![vault_id], |r| {
                Ok((
                    r.get::<_, Uuid>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut scored: Vec<(usize, String, CategoryView)> = rows
            .into_iter()
            .filter_map(|(id, name, name_norm)| {
                let distance = levenshtein(&key, &name_norm);
                (distance > 0 && distance <= threshold).then(|| {
                    (
                        distance,
                        name.to_lowercase(),
                        CategoryView {
                            id,
                            name,
                            is_system: false,
                            archived: false,
                        },
                    )
                })
            })
            .collect();
        scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        Ok(scored.into_iter().map(|(_, _, view)| view).collect())
    }
}

/// Edit distance over chars, ported from the v1 engine.
fn levenshtein(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }
    let mut costs: Vec<usize> = (0..=right.len()).collect();
    for (i, left_char) in left.iter().enumerate() {
        let mut last_cost = i;
        costs[0] = i + 1;
        for (j, right_char) in right.iter().enumerate() {
            let next_cost = costs[j + 1];
            let mut cost = if left_char == right_char {
                last_cost
            } else {
                last_cost + 1
            };
            cost = cost.min(costs[j] + 1).min(next_cost + 1);
            costs[j + 1] = cost;
            last_cost = next_cost;
        }
    }
    costs[right.len()]
}

#[cfg(test)]
mod tests {
    use super::levenshtein;

    #[test]
    fn distance_counts_edits() {
        assert_eq!(levenshtein("spesa", "spese"), 1);
        assert_eq!(levenshtein("spesa", "spesa"), 0);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }
}
