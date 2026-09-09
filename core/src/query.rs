//! Read side: snapshot, categories, transaction pages, log access, replay.

use chrono::{DateTime, FixedOffset, Offset, Utc};
use rusqlite::{OptionalExtension, params, params_from_iter, types::Value};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    Command, CommandEnvelope, CommandRecord, Core, Currency, DomainError, FlowMode, Result,
    TransactionKind,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletView {
    pub id: Uuid,
    pub name: String,
    pub balance: i64,
    pub archived: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowView {
    pub id: Uuid,
    pub name: String,
    pub balance: i64,
    pub mode: FlowMode,
    pub income_total: Option<i64>,
    pub allow_negative: bool,
    pub archived: bool,
    pub is_unallocated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryView {
    pub id: Uuid,
    pub name: String,
    pub is_system: bool,
    pub archived: bool,
}

/// Everything the UI needs to render a vault's accounts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultSnapshot {
    pub id: Uuid,
    pub name: String,
    pub currency: Currency,
    /// Sorted by name, archived included.
    pub wallets: Vec<WalletView>,
    /// Unallocated first, then by name, archived included.
    pub flows: Vec<FlowView>,
    pub unallocated_flow_id: Uuid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum LegTarget {
    Wallet { wallet_id: Uuid },
    Flow { flow_id: Uuid },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegView {
    pub target: LegTarget,
    /// Signed.
    pub amount: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionView {
    pub id: Uuid,
    pub kind: TransactionKind,
    pub occurred_at: DateTime<FixedOffset>,
    /// Absolute value; see `legs` for signs.
    pub amount: i64,
    pub category_id: Uuid,
    pub category: String,
    pub note: Option<String>,
    pub voided: bool,
    pub legs: Vec<LegView>,
}

/// Filter for [`Core::list_transactions`]. Defaults hide voided rows and
/// transfers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionFilter {
    /// Inclusive.
    pub from: Option<DateTime<Utc>>,
    /// Exclusive.
    pub to: Option<DateTime<Utc>>,
    /// Allow-list; `None` = all kinds (minus transfers unless
    /// `include_transfers`).
    pub kinds: Option<Vec<TransactionKind>>,
    pub include_voided: bool,
    pub include_transfers: bool,
    /// Only transactions with a leg on this wallet.
    pub wallet_id: Option<Uuid>,
    /// Only transactions with a leg on this flow.
    pub flow_id: Option<Uuid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub items: Vec<TransactionView>,
    /// Opaque; pass back to get the next (older) page.
    pub next_cursor: Option<String>,
}

impl Core {
    pub fn snapshot(&self, vault_id: Uuid) -> Result<VaultSnapshot> {
        let (name, currency): (String, String) = self
            .conn
            .query_row(
                "SELECT name, currency FROM vaults WHERE id = ?1",
                params![vault_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| DomainError::NotFound("vault".to_string()))?;
        let currency = Currency::try_from(currency.as_str())?;

        let mut stmt = self.conn.prepare(
            "SELECT id, name, balance, archived FROM wallets WHERE vault_id = ?1 ORDER BY lower(name)",
        )?;
        let wallets = stmt
            .query_map(params![vault_id], |r| {
                Ok(WalletView {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    balance: r.get(2)?,
                    archived: r.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut stmt = self.conn.prepare(
            "SELECT id, name, system_kind, balance, cap, income_total, allow_negative, archived
             FROM flows WHERE vault_id = ?1
             ORDER BY (system_kind IS NULL), lower(name)",
        )?;
        let flows = stmt
            .query_map(params![vault_id], |r| {
                let cap: Option<i64> = r.get(4)?;
                let income_total: Option<i64> = r.get(5)?;
                let mode = match (cap, income_total) {
                    (None, _) => FlowMode::Unlimited,
                    (Some(cap), None) => FlowMode::NetCapped { cap },
                    (Some(cap), Some(_)) => FlowMode::IncomeCapped { cap },
                };
                Ok(FlowView {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    is_unallocated: r.get::<_, Option<String>>(2)?.as_deref()
                        == Some("unallocated"),
                    balance: r.get(3)?,
                    mode,
                    income_total,
                    allow_negative: r.get(6)?,
                    archived: r.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let unallocated_flow_id = flows
            .iter()
            .find(|f| f.is_unallocated)
            .map(|f| f.id)
            .ok_or_else(|| DomainError::InvalidFlow("missing Unallocated flow".to_string()))?;

        Ok(VaultSnapshot {
            id: vault_id,
            name,
            currency,
            wallets,
            flows,
            unallocated_flow_id,
        })
    }

    pub fn categories(&self, vault_id: Uuid, include_archived: bool) -> Result<Vec<CategoryView>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, is_system, archived FROM categories
             WHERE vault_id = ?1 AND (?2 OR archived = 0)
             ORDER BY is_system DESC, lower(name)",
        )?;
        let rows = stmt
            .query_map(params![vault_id, include_archived], |r| {
                Ok(CategoryView {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    is_system: r.get(2)?,
                    archived: r.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Newest first, keyset-paginated on `(occurred_at, id)`.
    pub fn list_transactions(
        &self,
        vault_id: Uuid,
        filter: &TransactionFilter,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Page> {
        if let (Some(from), Some(to)) = (filter.from, filter.to)
            && from >= to
        {
            return Err(DomainError::InvalidCommand(
                "invalid range: from must be < to".to_string(),
            ));
        }
        if matches!(&filter.kinds, Some(k) if k.is_empty()) {
            return Err(DomainError::InvalidCommand(
                "kinds must not be empty".to_string(),
            ));
        }

        let mut sql = String::from(
            "SELECT t.id, t.kind, t.occurred_at, t.occurred_offset, t.amount, t.category_id, c.name, t.note, t.voided_at
             FROM transactions t JOIN categories c ON c.id = t.category_id
             WHERE t.vault_id = ?",
        );
        let mut args: Vec<Value> = vec![blob(vault_id)];

        for (kind, target) in [("wallet", filter.wallet_id), ("flow", filter.flow_id)] {
            if let Some(id) = target {
                sql.push_str(
                    " AND EXISTS (SELECT 1 FROM legs l WHERE l.transaction_id = t.id AND l.target_kind = ? AND l.target_id = ?)",
                );
                args.push(Value::Text(kind.to_string()));
                args.push(blob(id));
            }
        }
        if let Some(from) = filter.from {
            sql.push_str(" AND t.occurred_at >= ?");
            args.push(Value::Integer(from.timestamp()));
        }
        if let Some(to) = filter.to {
            sql.push_str(" AND t.occurred_at < ?");
            args.push(Value::Integer(to.timestamp()));
        }
        match &filter.kinds {
            Some(kinds) => {
                let marks = vec!["?"; kinds.len()].join(", ");
                sql.push_str(&format!(" AND t.kind IN ({marks})"));
                args.extend(kinds.iter().map(|k| Value::Text(k.as_str().to_string())));
            }
            None if !filter.include_transfers => {
                sql.push_str(" AND t.kind NOT IN ('transfer_wallet', 'transfer_flow')");
            }
            None => {}
        }
        if !filter.include_voided {
            sql.push_str(" AND t.voided_at IS NULL");
        }
        if let Some(cursor) = cursor {
            let (at, id) = parse_cursor(cursor)?;
            sql.push_str(" AND (t.occurred_at < ? OR (t.occurred_at = ? AND t.id < ?))");
            args.push(Value::Integer(at));
            args.push(Value::Integer(at));
            args.push(blob(id));
        }
        sql.push_str(" ORDER BY t.occurred_at DESC, t.id DESC LIMIT ?");
        args.push(Value::Integer(
            i64::try_from(limit).unwrap_or(i64::MAX).saturating_add(1),
        ));

        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows: Vec<(TransactionView, i64)> = stmt
            .query_map(params_from_iter(args.iter()), |r| {
                let at: i64 = r.get(2)?;
                let off: i32 = r.get(3)?;
                let kind: String = r.get(1)?;
                let voided_at: Option<i64> = r.get(8)?;
                Ok((
                    TransactionView {
                        id: r.get(0)?,
                        kind: TransactionKind::parse(&kind).unwrap_or(TransactionKind::Expense),
                        occurred_at: to_fixed(at, off),
                        amount: r.get(4)?,
                        category_id: r.get(5)?,
                        category: r.get(6)?,
                        note: r.get(7)?,
                        voided: voided_at.is_some(),
                        legs: Vec::new(),
                    },
                    at,
                ))
            })?
            .collect::<std::result::Result<_, _>>()?;

        let next_cursor = if rows.len() > limit {
            rows.truncate(limit);
            rows.last().map(|(t, at)| format!("{at}.{}", t.id.simple()))
        } else {
            None
        };

        let mut legs_stmt = self.conn.prepare(
            "SELECT target_kind, target_id, amount FROM legs WHERE transaction_id = ?1 ORDER BY ordinal",
        )?;
        let mut items = Vec::with_capacity(rows.len());
        for (mut view, _) in rows {
            view.legs = legs_stmt
                .query_map(params![view.id], |r| {
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
                .collect::<std::result::Result<_, _>>()?;
            items.push(view);
        }
        Ok(Page { items, next_cursor })
    }

    /// Log entries of a vault with `seq > since_seq`, in order.
    pub fn commands_since(&self, vault_id: Uuid, since_seq: i64) -> Result<Vec<CommandRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, vault_id, seq, author, payload, created_at, result_id
             FROM commands WHERE vault_id = ?1 AND seq > ?2 AND status = 'applied' ORDER BY seq",
        )?;
        let rows = stmt.query_map(params![vault_id, since_seq], |r| {
            Ok((
                r.get::<_, Uuid>(0)?,
                r.get::<_, Uuid>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, Option<Uuid>>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, vault_id, seq, author, payload, created_at, result_id) = row?;
            let command: Command = serde_json::from_str(&payload)?;
            out.push(CommandRecord {
                envelope: CommandEnvelope {
                    id,
                    vault_id,
                    author,
                    command,
                },
                seq,
                created_at,
                result_id,
            });
        }
        Ok(out)
    }
}

/// Re-execute a log into `target`. Ids and balances come out identical; only
/// `commands.created_at` differs.
pub fn replay(records: &[CommandRecord], target: &mut Core) -> Result<()> {
    for record in records {
        target.execute(record.envelope.clone())?;
    }
    Ok(())
}

fn blob(id: Uuid) -> Value {
    Value::Blob(id.as_bytes().to_vec())
}

fn to_fixed(secs: i64, offset_secs: i32) -> DateTime<FixedOffset> {
    let tz = FixedOffset::east_opt(offset_secs).unwrap_or_else(|| Utc.fix());
    DateTime::from_timestamp(secs, 0)
        .unwrap_or_default()
        .with_timezone(&tz)
}

fn parse_cursor(cursor: &str) -> Result<(i64, Uuid)> {
    let bad = || DomainError::InvalidCursor("invalid transactions cursor".to_string());
    let (at, id) = cursor.split_once('.').ok_or_else(bad)?;
    let at: i64 = at.parse().map_err(|_| bad())?;
    let id = Uuid::parse_str(id).map_err(|_| bad())?;
    Ok((at, id))
}
