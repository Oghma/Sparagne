//! App-facing read queries: vault listing, single-transaction lookup and
//! recent picker usage.

use chrono::{DateTime, FixedOffset, Offset, Utc};
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use crate::{
    Core, Currency, DomainError, LegTarget, LegView, Result, TransactionKind, TransactionView,
};

/// One row of [`Core::vaults`].
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct VaultView {
    pub id: Uuid,
    pub name: String,
    pub currency: Currency,
    pub owner: String,
    pub created_at: i64,
}

/// Ids of the most recently used entities, most recent first. Feeds the
/// "recent" tier of picker ordering (default -> recent -> rest).
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RecentUsage {
    pub categories: Vec<Uuid>,
}

/// Sums of `transactions.amount` by kind over a `[from, to)` range, either
/// end open.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PeriodTotals {
    pub income: i64,
    pub expense: i64,
    pub refund: i64,
    /// `max(expense - refund, 0)`.
    pub net_expense: i64,
}

impl Core {
    /// Every vault in the database, ordered by name. Names are labels and may
    /// repeat: namesakes follow the order they were created in.
    pub fn vaults(&self) -> Result<Vec<VaultView>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, currency, owner_user_id, created_at FROM vaults
             ORDER BY lower(name), created_at, id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, Uuid>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, name, currency, owner, created_at)| {
                Ok(VaultView {
                    id,
                    name,
                    currency: Currency::try_from(currency.as_str())?,
                    owner,
                    created_at,
                })
            })
            .collect()
    }

    /// One transaction with its legs, voided included.
    pub fn transaction(&self, vault_id: Uuid, transaction_id: Uuid) -> Result<TransactionView> {
        let row = self
            .conn
            .query_row(
                "SELECT t.kind, t.occurred_at, t.occurred_offset, t.amount, t.category_id, c.name, c.is_system, t.note, t.voided_at, t.created_by, t.person
                 FROM transactions t JOIN categories c ON c.id = t.category_id
                 WHERE t.vault_id = ?1 AND t.id = ?2",
                params![vault_id, transaction_id],
                |r| {
                    let at: i64 = r.get(1)?;
                    let off: i32 = r.get(2)?;
                    let kind: String = r.get(0)?;
                    let voided_at: Option<i64> = r.get(8)?;
                    Ok(TransactionView {
                        id: transaction_id,
                        kind: TransactionKind::parse(&kind).unwrap_or(TransactionKind::Expense),
                        occurred_at: to_fixed(at, off),
                        amount: r.get(3)?,
                        category_id: r.get(4)?,
                        category: r.get(5)?,
                        category_is_system: r.get(6)?,
                        note: r.get(7)?,
                        person: r.get(10)?,
                        created_by: r.get(9)?,
                        voided: voided_at.is_some(),
                        wallet_id: None,
                        flow_id: None,
                        from_id: None,
                        to_id: None,
                        legs: Vec::new(),
                    })
                },
            )
            .optional()?
            .ok_or_else(|| DomainError::NotFound("transaction".to_string()))?;

        let mut legs_stmt = self.conn.prepare(
            "SELECT target_kind, target_id, amount FROM legs WHERE transaction_id = ?1 ORDER BY ordinal",
        )?;
        let legs = legs_stmt
            .query_map(params![transaction_id], |r| {
                let kind: String = r.get(0)?;
                let id: Uuid = r.get(1)?;
                let target = if kind == "wallet" {
                    LegTarget::Wallet { wallet_id: id }
                } else {
                    LegTarget::Flow { flow_id: id }
                };
                Ok(LegView {
                    target,
                    amount: r.get(2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let (wallet_id, flow_id, from_id, to_id) = crate::query::leg_shape(row.kind, &legs);
        Ok(TransactionView {
            legs,
            wallet_id,
            flow_id,
            from_id,
            to_id,
            ..row
        })
    }

    /// Ids of the most recently used categories among non-voided,
    /// non-transfer transactions with `occurred_at >= since`, at most `limit`.
    /// System categories are excluded.
    pub fn recent_usage(
        &self,
        vault_id: Uuid,
        since: DateTime<Utc>,
        limit: usize,
    ) -> Result<RecentUsage> {
        let since = since.timestamp();
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);

        let mut cat_stmt = self.conn.prepare(
            "SELECT t.category_id
             FROM transactions t
             WHERE t.vault_id = ?1 AND t.voided_at IS NULL
               AND t.kind NOT IN ('transfer_wallet', 'transfer_flow')
               AND t.occurred_at >= ?2
               AND EXISTS (SELECT 1 FROM categories c WHERE c.id = t.category_id AND c.is_system = 0)
             GROUP BY t.category_id
             ORDER BY MAX(t.occurred_at) DESC, MAX(t.id) DESC
             LIMIT ?3",
        )?;
        let categories = cat_stmt
            .query_map(params![vault_id, since, limit], |r| r.get(0))?
            .collect::<std::result::Result<Vec<Uuid>, _>>()?;

        Ok(RecentUsage { categories })
    }
}

fn to_fixed(secs: i64, offset_secs: i32) -> DateTime<FixedOffset> {
    let tz = FixedOffset::east_opt(offset_secs).unwrap_or_else(|| Utc.fix());
    DateTime::from_timestamp(secs, 0)
        .unwrap_or_default()
        .with_timezone(&tz)
}
