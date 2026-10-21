//! App-facing read queries: vault listing, single-transaction lookup and
//! recent picker usage.

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use crate::{
    Core, Currency, DomainError, Result, TransactionView,
    query::{TRANSACTION_VIEW_COLUMNS, load_legs, transaction_view},
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
        self.read_vaults("ORDER BY lower(name), created_at, id", [])
    }

    /// One vault by id, `None` when this database does not hold it. Saves a
    /// caller with an id in hand from scanning [`Core::vaults`].
    pub fn vault(&self, vault_id: Uuid) -> Result<Option<VaultView>> {
        Ok(self.read_vaults("WHERE id = ?1", params![vault_id])?.pop())
    }

    fn read_vaults(&self, tail: &str, args: impl rusqlite::Params) -> Result<Vec<VaultView>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, name, currency, owner_user_id, created_at FROM vaults {tail}"
        ))?;
        let rows = stmt
            .query_map(args, |r| {
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
        let mut view = self
            .conn
            .query_row(
                &format!(
                    "SELECT {TRANSACTION_VIEW_COLUMNS}
                     FROM transactions t JOIN categories c ON c.id = t.category_id
                     WHERE t.vault_id = ?1 AND t.id = ?2"
                ),
                params![vault_id, transaction_id],
                transaction_view,
            )
            .optional()?
            .ok_or_else(|| DomainError::NotFound("transaction".to_string()))?;
        load_legs(&self.conn, std::slice::from_mut(&mut view))?;
        Ok(view)
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
