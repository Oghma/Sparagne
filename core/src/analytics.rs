//! Aggregations behind the ledger's summary panel and the summary views
//! (`docs/v2/UI.md` §4).
//!
//! Every query takes a half-open `[from, to)` range in UTC: the app computes
//! the month boundaries with the system timezone, so the core never has to
//! know about calendars or offsets. Voided rows and transfers are always out;
//! `net_expense` follows `DISTILLATO_V1.md` §3.5 (`max(expense - refund, 0)`).
//!
//! [`Core::bucket_totals`] and [`Core::year_breakdown`] take the boundaries of
//! several consecutive ranges instead, so a whole year costs one query.

use chrono::{DateTime, FixedOffset, Utc};
use rusqlite::{params_from_iter, types::Value};
use uuid::Uuid;

use crate::{
    Core, DomainError, PeriodTotals, Result,
    category::OPENING_KEY,
    query::{blob, to_fixed},
};

/// Income, expense and refund on one envelope for one person.
///
/// Amounts come from the flow legs, not from `transactions.amount`, so a
/// future expense split across two envelopes lands on both.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FlowPersonTotals {
    pub flow_id: Uuid,
    /// `transactions.created_by`: the member of the vault, the PERSONA column.
    pub person: String,
    pub income: i64,
    pub expense: i64,
    pub refund: i64,
    /// `max(expense - refund, 0)`.
    pub net_expense: i64,
}

/// One row of the category breakdown.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CategoryTotals {
    pub category_id: Uuid,
    pub name: String,
    /// `true` for `Opening` and `Uncategorized`: the app localizes the name.
    pub is_system: bool,
    pub income: i64,
    pub expense: i64,
    pub refund: i64,
    /// `max(expense - refund, 0)`.
    pub net_expense: i64,
    /// Transactions behind the row, voided excluded.
    pub count: u32,
}

/// One line of "top uscite del mese".
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TopExpense {
    pub transaction_id: Uuid,
    pub occurred_at: DateTime<FixedOffset>,
    pub note: Option<String>,
    pub category: String,
    pub category_is_system: bool,
    pub person: String,
    pub amount: i64,
}

/// One person's movement inside one bucket of the year summary, split the way
/// the RIEPILOGO reads it (`docs/v2/UI.md` §2.2).
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct BucketPersonTotals {
    /// Index of the gap between consecutive `bounds`: 0 is `[b0, b1)`.
    pub bucket: u32,
    /// `transactions.created_by`.
    pub person: String,
    /// Income other than opening balances.
    pub income: i64,
    /// Opening balances of wallets (system category `Opening`), signed: an
    /// income counts positive, an expense negative.
    pub opening: i64,
    /// `max(expense - refund, 0)` on envelopes without a cap.
    pub cash_expense: i64,
    /// `max(expense - refund, 0)` on envelopes with a cap (the "fondi").
    pub fund_expense: i64,
}

/// The six sums [`BucketPersonTotals`] is folded from: expenses and refunds
/// stay apart until the end, because each group nets on its own.
#[derive(Default)]
struct YearSlot {
    income: i64,
    opening: i64,
    cash_expense: i64,
    cash_refund: i64,
    fund_expense: i64,
    fund_refund: i64,
}

/// Folds the three kind sums into [`PeriodTotals`].
fn totals(income: i64, expense: i64, refund: i64) -> PeriodTotals {
    PeriodTotals {
        income,
        expense,
        refund,
        net_expense: (expense - refund).max(0),
    }
}

/// Adds `amount` to the slot `kind` names, ignoring anything else.
fn accumulate(kind: &str, amount: i64, income: &mut i64, expense: &mut i64, refund: &mut i64) {
    match kind {
        "income" => *income += amount,
        "expense" => *expense += amount,
        "refund" => *refund += amount,
        _ => {}
    }
}

/// Rejects an empty or inverted range the same way the other read queries do.
fn check_range(from: DateTime<Utc>, to: DateTime<Utc>) -> Result<()> {
    if from >= to {
        return Err(DomainError::InvalidCommand(
            "invalid range: from must be < to".to_string(),
        ));
    }
    Ok(())
}

/// Rejects boundaries that cannot describe consecutive half-open buckets.
/// `query` names the caller so the message says which one refused.
fn check_bounds(bounds: &[DateTime<Utc>], query: &str) -> Result<()> {
    if bounds.len() < 2 {
        return Err(DomainError::InvalidCommand(format!(
            "{query} needs at least two boundaries"
        )));
    }
    if bounds.windows(2).any(|w| w[0] >= w[1]) {
        return Err(DomainError::InvalidCommand(format!(
            "{query} boundaries must be strictly increasing"
        )));
    }
    Ok(())
}

/// `WHERE` shared by every aggregation: one vault, live rows, entries only,
/// inside the range, optionally one person.
fn scope(
    vault_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    person: Option<&str>,
) -> (String, Vec<Value>) {
    let mut sql = String::from(
        " WHERE t.vault_id = ? AND t.voided_at IS NULL
            AND t.kind IN ('income', 'expense', 'refund')
            AND t.occurred_at >= ? AND t.occurred_at < ?",
    );
    let mut args = vec![
        blob(vault_id),
        Value::Integer(from.timestamp()),
        Value::Integer(to.timestamp()),
    ];
    if let Some(person) = person {
        sql.push_str(" AND t.created_by = ?");
        args.push(Value::Text(person.to_string()));
    }
    (sql, args)
}

impl Core {
    /// Distinct authors of live transactions, ordered case-insensitively.
    /// Feeds the `TUTTI / ELISA / MATTEO` segmented control.
    pub fn authors(&self, vault_id: Uuid) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT created_by FROM transactions
             WHERE vault_id = ?1 AND voided_at IS NULL
             ORDER BY lower(created_by)",
        )?;
        let rows = stmt
            .query_map([blob(vault_id)], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Envelope x person matrix over the range. Only pairs with movement are
    /// returned; the app fills the gaps with zeros.
    pub fn flow_person_totals(
        &self,
        vault_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<FlowPersonTotals>> {
        check_range(from, to)?;
        let (where_sql, args) = scope(vault_id, from, to, None);
        let sql = format!(
            "SELECT l.target_id, t.created_by, t.kind, SUM(ABS(l.amount))
             FROM transactions t
             JOIN legs l ON l.transaction_id = t.id AND l.target_kind = 'flow'
             {where_sql}
             GROUP BY l.target_id, t.created_by, t.kind
             ORDER BY lower(t.created_by)"
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(args.iter()), |r| {
                Ok((
                    r.get::<_, Uuid>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // Three kind rows collapse into one record per (flow, person); the
        // SQL order keeps the output stable for the tests and the UI.
        let mut out: Vec<FlowPersonTotals> = Vec::new();
        for (flow_id, person, kind, amount) in rows {
            let slot = match out
                .iter_mut()
                .find(|f| f.flow_id == flow_id && f.person == person)
            {
                Some(existing) => existing,
                None => {
                    out.push(FlowPersonTotals {
                        flow_id,
                        person,
                        income: 0,
                        expense: 0,
                        refund: 0,
                        net_expense: 0,
                    });
                    out.last_mut().unwrap_or_else(|| unreachable!())
                }
            };
            accumulate(
                &kind,
                amount,
                &mut slot.income,
                &mut slot.expense,
                &mut slot.refund,
            );
            slot.net_expense = (slot.expense - slot.refund).max(0);
        }
        Ok(out)
    }

    /// Category breakdown over the range, heaviest net expense first.
    pub fn category_totals(
        &self,
        vault_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        person: Option<String>,
    ) -> Result<Vec<CategoryTotals>> {
        check_range(from, to)?;
        let (where_sql, args) = scope(vault_id, from, to, person.as_deref());
        let sql = format!(
            "SELECT t.category_id, c.name, c.is_system, t.kind, SUM(t.amount), COUNT(*)
             FROM transactions t JOIN categories c ON c.id = t.category_id
             {where_sql}
             GROUP BY t.category_id, t.kind"
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(args.iter()), |r| {
                Ok((
                    r.get::<_, Uuid>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, u32>(5)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut out: Vec<CategoryTotals> = Vec::new();
        for (category_id, name, is_system, kind, amount, count) in rows {
            let slot = match out.iter_mut().find(|c| c.category_id == category_id) {
                Some(existing) => existing,
                None => {
                    out.push(CategoryTotals {
                        category_id,
                        name,
                        is_system,
                        income: 0,
                        expense: 0,
                        refund: 0,
                        net_expense: 0,
                        count: 0,
                    });
                    out.last_mut().unwrap_or_else(|| unreachable!())
                }
            };
            accumulate(
                &kind,
                amount,
                &mut slot.income,
                &mut slot.expense,
                &mut slot.refund,
            );
            slot.net_expense = (slot.expense - slot.refund).max(0);
            slot.count += count;
        }
        out.sort_by(|a, b| {
            b.net_expense
                .cmp(&a.net_expense)
                .then_with(|| b.income.cmp(&a.income))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(out)
    }

    /// Totals of the `bounds.len() - 1` consecutive ranges the boundaries
    /// describe: `[b0, b1)`, `[b1, b2)`, ... The app passes 13 month starts to
    /// get the twelve bars of a year, so leap years and DST stay in Swift.
    pub fn bucket_totals(
        &self,
        vault_id: Uuid,
        bounds: Vec<DateTime<Utc>>,
        person: Option<String>,
    ) -> Result<Vec<PeriodTotals>> {
        check_bounds(&bounds, "bucket_totals")?;

        let edges: Vec<i64> = bounds.iter().map(|b| b.timestamp()).collect();
        let (where_sql, args) = scope(
            vault_id,
            bounds[0],
            bounds[bounds.len() - 1],
            person.as_deref(),
        );
        let sql = format!("SELECT t.occurred_at, t.kind, t.amount FROM transactions t {where_sql}");

        // One pass over the range, bucketed in Rust: a year of a personal
        // ledger is a few thousand rows, and this keeps the SQL free of a
        // generated CASE ladder.
        let mut sums = vec![(0i64, 0i64, 0i64); edges.len() - 1];
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(args.iter()))?;
        while let Some(row) = rows.next()? {
            let at: i64 = row.get(0)?;
            let kind: String = row.get(1)?;
            let amount: i64 = row.get(2)?;
            // partition_point gives the first edge strictly greater than `at`;
            // minus one is the bucket that contains it.
            let bucket = edges.partition_point(|&edge| edge <= at).saturating_sub(1);
            if let Some((income, expense, refund)) = sums.get_mut(bucket) {
                accumulate(&kind, amount, income, expense, refund);
            }
        }
        Ok(sums
            .into_iter()
            .map(|(income, expense, refund)| totals(income, expense, refund))
            .collect())
    }

    /// Bucket x person breakdown behind the RIEPILOGO (`docs/v2/UI.md` §4).
    ///
    /// The app passes the epoch followed by thirteen month starts, so bucket 0
    /// is everything that happened before the year. Amounts come from the flow
    /// legs, like [`Core::flow_person_totals`], so a future split lands on both
    /// envelopes, and each leg is cash or fund depending on whether its
    /// envelope has a cap. Only pairs with movement come back; the app fills
    /// the gaps with zeros.
    pub fn year_breakdown(
        &self,
        vault_id: Uuid,
        bounds: Vec<DateTime<Utc>>,
    ) -> Result<Vec<BucketPersonTotals>> {
        check_bounds(&bounds, "year_breakdown")?;

        let edges: Vec<i64> = bounds.iter().map(|b| b.timestamp()).collect();
        let (where_sql, args) = scope(vault_id, bounds[0], bounds[bounds.len() - 1], None);
        let sql = format!(
            "SELECT t.occurred_at, t.created_by, t.kind, c.is_system, c.name_norm,
                    f.cap IS NULL, ABS(l.amount)
             FROM transactions t
             JOIN legs l ON l.transaction_id = t.id AND l.target_kind = 'flow'
             JOIN flows f ON f.id = l.target_id
             JOIN categories c ON c.id = t.category_id
             {where_sql}"
        );

        // Bucketed in Rust like bucket_totals: a year of a personal ledger is
        // a few thousand legs, and the SQL stays free of a CASE ladder.
        let mut slots: Vec<((usize, String), YearSlot)> = Vec::new();
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(args.iter()))?;
        while let Some(row) = rows.next()? {
            let at: i64 = row.get(0)?;
            let person: String = row.get(1)?;
            let kind: String = row.get(2)?;
            let is_system: bool = row.get(3)?;
            let category: String = row.get(4)?;
            let uncapped: bool = row.get(5)?;
            let amount: i64 = row.get(6)?;

            // partition_point gives the first edge strictly greater than
            // `at`; minus one is the bucket that contains it. The scope keeps
            // `at` inside the outer range, so the index is always a bucket.
            let bucket = edges.partition_point(|&edge| edge <= at).saturating_sub(1);
            if bucket >= edges.len() - 1 {
                continue;
            }
            let slot = match slots
                .iter_mut()
                .find(|((b, p), _)| *b == bucket && *p == person)
            {
                Some((_, existing)) => existing,
                None => {
                    slots.push(((bucket, person), YearSlot::default()));
                    match slots.last_mut() {
                        Some((_, slot)) => slot,
                        None => unreachable!("just pushed"),
                    }
                }
            };

            // A wallet's opening balance is an income (or an expense when
            // negative) on the Unallocated envelope: it is neither earned nor
            // spent, so it only ever feeds FONDO CASSA.
            if is_system && category == OPENING_KEY {
                match kind.as_str() {
                    "income" => slot.opening += amount,
                    "expense" => slot.opening -= amount,
                    _ => {}
                }
                continue;
            }
            match (kind.as_str(), uncapped) {
                ("income", _) => slot.income += amount,
                ("expense", true) => slot.cash_expense += amount,
                ("expense", false) => slot.fund_expense += amount,
                ("refund", true) => slot.cash_refund += amount,
                ("refund", false) => slot.fund_refund += amount,
                _ => {}
            }
        }

        let mut out: Vec<BucketPersonTotals> = slots
            .into_iter()
            .map(|((bucket, person), slot)| BucketPersonTotals {
                bucket: u32::try_from(bucket).unwrap_or(u32::MAX),
                person,
                income: slot.income,
                opening: slot.opening,
                cash_expense: (slot.cash_expense - slot.cash_refund).max(0),
                fund_expense: (slot.fund_expense - slot.fund_refund).max(0),
            })
            .collect();
        out.sort_by(|a, b| {
            a.bucket
                .cmp(&b.bucket)
                .then_with(|| a.person.to_lowercase().cmp(&b.person.to_lowercase()))
        });
        Ok(out)
    }

    /// The heaviest expenses of the range, largest first.
    pub fn top_expenses(
        &self,
        vault_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        person: Option<String>,
        limit: u32,
    ) -> Result<Vec<TopExpense>> {
        check_range(from, to)?;
        let (where_sql, mut args) = scope(vault_id, from, to, person.as_deref());
        let sql = format!(
            "SELECT t.id, t.occurred_at, t.occurred_offset, t.note, c.name, c.is_system,
                    t.created_by, t.amount
             FROM transactions t JOIN categories c ON c.id = t.category_id
             {where_sql} AND t.kind = 'expense'
             ORDER BY t.amount DESC, t.occurred_at DESC, t.id DESC
             LIMIT ?"
        );
        args.push(Value::Integer(i64::from(limit)));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(args.iter()), |r| {
                Ok(TopExpense {
                    transaction_id: r.get(0)?,
                    occurred_at: to_fixed(r.get(1)?, r.get(2)?),
                    note: r.get(3)?,
                    category: r.get(4)?,
                    category_is_system: r.get(5)?,
                    person: r.get(6)?,
                    amount: r.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
