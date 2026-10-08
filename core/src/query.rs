//! Read side: snapshot, categories, transaction pages, log access, replay.

use chrono::{DateTime, FixedOffset, Offset, Utc};
use rusqlite::{OptionalExtension, params, params_from_iter, types::Value};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    CommandEnvelope, CommandRecord, Core, Currency, DomainError, FlowMode, Result, TransactionKind,
    VaultView,
};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct WalletView {
    pub id: Uuid,
    pub name: String,
    pub balance: i64,
    pub archived: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
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

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CategoryView {
    pub id: Uuid,
    pub name: String,
    pub is_system: bool,
    pub archived: bool,
}

/// Everything the UI needs to render a vault's accounts.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum LegTarget {
    Wallet { wallet_id: Uuid },
    Flow { flow_id: Uuid },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegView {
    pub target: LegTarget,
    /// Signed.
    pub amount: i64,
}

/// One transaction as the table shows it.
///
/// `legs` is the raw shape; `wallet_id`, `flow_id`, `from_id` and `to_id` are
/// the same information already sorted out by kind, so the app never has to
/// bucket the legs itself. Entries fill `wallet_id` and `flow_id`; transfers
/// fill `from_id` with the negative leg and `to_id` with the positive one.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TransactionView {
    pub id: Uuid,
    pub kind: TransactionKind,
    pub occurred_at: DateTime<FixedOffset>,
    /// Absolute value; see `legs` for signs.
    pub amount: i64,
    pub category_id: Uuid,
    pub category: String,
    /// `true` for `Opening` and `Uncategorized`: the app localizes the name.
    pub category_is_system: bool,
    pub note: Option<String>,
    /// Who the row is for, the PERSONA column: the person the command named,
    /// else its author. A transfer is always its author's, and an opening
    /// balance starts as its author's.
    pub person: String,
    /// Who recorded it: the author of the command that created the row, the
    /// account username once the vault is shared. Equal to `person` unless
    /// someone recorded the row on another member's behalf.
    pub created_by: String,
    pub voided: bool,
    /// Entries only: the wallet the money moved on.
    pub wallet_id: Option<Uuid>,
    /// Entries only: the envelope the money moved on.
    pub flow_id: Option<Uuid>,
    /// Transfers only: the source, a wallet or a flow matching the kind.
    pub from_id: Option<Uuid>,
    /// Transfers only: the destination.
    pub to_id: Option<Uuid>,
    pub legs: Vec<LegView>,
}

/// Splits the legs of a transaction into the ids [`TransactionView`] exposes:
/// `(wallet_id, flow_id, from_id, to_id)`.
pub(crate) fn leg_shape(
    kind: TransactionKind,
    legs: &[LegView],
) -> (Option<Uuid>, Option<Uuid>, Option<Uuid>, Option<Uuid>) {
    let wallet = |leg: &LegView| match leg.target {
        LegTarget::Wallet { wallet_id } => Some(wallet_id),
        LegTarget::Flow { .. } => None,
    };
    let flow = |leg: &LegView| match leg.target {
        LegTarget::Flow { flow_id } => Some(flow_id),
        LegTarget::Wallet { .. } => None,
    };
    if kind.is_transfer() {
        // Source = the negative leg, destination = the positive one; ordinal
        // order is the fallback for the degenerate zero-amount case.
        let ids: Vec<Uuid> = legs
            .iter()
            .filter_map(|leg| wallet(leg).or_else(|| flow(leg)))
            .collect();
        let from = legs
            .iter()
            .position(|leg| leg.amount < 0)
            .or(Some(0))
            .and_then(|i| ids.get(i).copied());
        let to = legs
            .iter()
            .position(|leg| leg.amount > 0)
            .or(Some(1))
            .and_then(|i| ids.get(i).copied());
        (None, None, from, to)
    } else {
        (
            legs.iter().find_map(wallet),
            legs.iter().find_map(flow),
            None,
            None,
        )
    }
}

/// Filter for [`Core::list_transactions`]. Defaults hide voided rows and
/// transfers.
#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct TransactionFilter {
    /// Inclusive.
    #[uniffi(default = None)]
    pub from: Option<DateTime<Utc>>,
    /// Exclusive.
    #[uniffi(default = None)]
    pub to: Option<DateTime<Utc>>,
    /// Allow-list; `None` = all kinds (minus transfers unless
    /// `include_transfers`).
    #[uniffi(default = None)]
    pub kinds: Option<Vec<TransactionKind>>,
    #[uniffi(default = false)]
    pub include_voided: bool,
    #[uniffi(default = false)]
    pub include_transfers: bool,
    /// Only transactions with a leg on this wallet.
    #[uniffi(default = None)]
    pub wallet_id: Option<Uuid>,
    /// Only transactions with a leg on this flow.
    #[uniffi(default = None)]
    pub flow_id: Option<Uuid>,
    /// Case-insensitive substring on note or category name.
    #[uniffi(default = None)]
    pub text: Option<String>,
    /// Exact `person`: the PERSONA filter of the ledger.
    /// `None` = everybody.
    #[uniffi(default = None)]
    pub person: Option<String>,
    /// Oldest first, the reading order of the ledger. Cursors keep working:
    /// they simply walk forward instead of backward.
    #[uniffi(default = false)]
    pub ascending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Page {
    pub items: Vec<TransactionView>,
    /// Opaque; pass back to get the next (older) page.
    pub next_cursor: Option<String>,
}

impl Core {
    /// One vault by id, `None` when this database does not hold it. Saves a
    /// caller with an id in hand from scanning [`Core::vaults`].
    pub fn vault(&self, vault_id: Uuid) -> Result<Option<VaultView>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, name, currency, owner_user_id, created_at FROM vaults WHERE id = ?1",
                params![vault_id],
                |r| {
                    Ok((
                        r.get::<_, Uuid>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(id, name, currency, owner, created_at)| {
            Ok(VaultView {
                id,
                name,
                currency: Currency::try_from(currency.as_str())?,
                owner,
                created_at,
            })
        })
        .transpose()
    }

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
            "SELECT t.id, t.kind, t.occurred_at, t.occurred_offset, t.amount, t.category_id, c.name, c.is_system, t.note, t.voided_at, t.created_by, t.person
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
        if let Some(text) = filter.text.as_deref() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                let escaped = trimmed
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_");
                sql.push_str(
                    " AND (t.note LIKE ? ESCAPE '\\' COLLATE NOCASE OR c.name LIKE ? ESCAPE '\\' COLLATE NOCASE)",
                );
                let pattern = format!("%{escaped}%");
                args.push(Value::Text(pattern.clone()));
                args.push(Value::Text(pattern));
            }
        }
        if let Some(person) = filter.person.as_deref() {
            sql.push_str(" AND t.person = ?");
            args.push(Value::Text(person.to_string()));
        }
        if let Some(cursor) = cursor {
            let (at, id) = parse_cursor(cursor)?;
            let cmp = if filter.ascending { '>' } else { '<' };
            sql.push_str(&format!(
                " AND (t.occurred_at {cmp} ? OR (t.occurred_at = ? AND t.id {cmp} ?))"
            ));
            args.push(Value::Integer(at));
            args.push(Value::Integer(at));
            args.push(blob(id));
        }
        let direction = if filter.ascending { "ASC" } else { "DESC" };
        sql.push_str(&format!(
            " ORDER BY t.occurred_at {direction}, t.id {direction} LIMIT ?"
        ));
        args.push(Value::Integer(
            i64::try_from(limit).unwrap_or(i64::MAX).saturating_add(1),
        ));

        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows: Vec<(TransactionView, i64)> = stmt
            .query_map(params_from_iter(args.iter()), |r| {
                let at: i64 = r.get(2)?;
                let off: i32 = r.get(3)?;
                let kind: String = r.get(1)?;
                let voided_at: Option<i64> = r.get(9)?;
                Ok((
                    TransactionView {
                        id: r.get(0)?,
                        kind: TransactionKind::parse(&kind).unwrap_or(TransactionKind::Expense),
                        occurred_at: to_fixed(at, off),
                        amount: r.get(4)?,
                        category_id: r.get(5)?,
                        category: r.get(6)?,
                        category_is_system: r.get(7)?,
                        note: r.get(8)?,
                        person: r.get(11)?,
                        created_by: r.get(10)?,
                        voided: voided_at.is_some(),
                        wallet_id: None,
                        flow_id: None,
                        from_id: None,
                        to_id: None,
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
            let (wallet_id, flow_id, from_id, to_id) = leg_shape(view.kind, &view.legs);
            view.wallet_id = wallet_id;
            view.flow_id = flow_id;
            view.from_id = from_id;
            view.to_id = to_id;
            items.push(view);
        }
        Ok(Page { items, next_cursor })
    }

    /// Log entries of a vault with `seq > since_seq`, in order.
    pub fn commands_since(&self, vault_id: Uuid, since_seq: i64) -> Result<Vec<CommandRecord>> {
        log_records(
            &self.conn,
            "vault_id = ?1 AND seq > ?2 AND status = 'applied'",
            params![vault_id, since_seq],
        )
    }
}

/// Log rows matching a `WHERE` clause, ordered by local seq.
///
/// Takes a bare connection so the sync rebase can read the log from inside the
/// transaction that is rewriting it.
pub(crate) fn log_records(
    conn: &rusqlite::Connection,
    where_clause: &str,
    args: impl rusqlite::Params,
) -> Result<Vec<CommandRecord>> {
    let sql = format!(
        "SELECT id, vault_id, seq, author, payload, created_at, result_id, server_seq
         FROM commands WHERE {where_clause} ORDER BY seq"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(args, |r| {
            Ok(LogRowFields {
                id: r.get(0)?,
                vault_id: r.get(1)?,
                seq: r.get(2)?,
                author: r.get(3)?,
                payload: r.get(4)?,
                created_at: r.get(5)?,
                result_id: r.get(6)?,
                server_seq: r.get(7)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|row| {
            Ok(CommandRecord {
                envelope: CommandEnvelope {
                    id: row.id,
                    vault_id: row.vault_id,
                    author: row.author,
                    command: serde_json::from_str(&row.payload)?,
                },
                seq: row.seq,
                created_at: row.created_at,
                result_id: row.result_id,
                server_seq: row.server_seq,
            })
        })
        .collect()
}

/// One `commands` row as SQLite hands it over, before the payload is parsed.
struct LogRowFields {
    id: Uuid,
    vault_id: Uuid,
    seq: i64,
    author: String,
    payload: String,
    created_at: i64,
    result_id: Option<Uuid>,
    server_seq: Option<i64>,
}

/// Re-execute a log into `target`. Ids and balances come out identical; only
/// `commands.created_at` differs.
pub fn replay(records: &[CommandRecord], target: &mut Core) -> Result<()> {
    for record in records {
        target.execute(record.envelope.clone())?;
    }
    Ok(())
}

pub(crate) fn blob(id: Uuid) -> Value {
    Value::Blob(id.as_bytes().to_vec())
}

pub(crate) fn to_fixed(secs: i64, offset_secs: i32) -> DateTime<FixedOffset> {
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
