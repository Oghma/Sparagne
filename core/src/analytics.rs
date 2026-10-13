//! Aggregations behind the ledger's summary panel and the summary views.
//!
//! Every query takes a half-open `[from, to)` range in UTC: the app computes
//! the month boundaries with the system timezone, so the core never has to
//! know about calendars or offsets. Voided rows are always out, and so are
//! transfers except in [`Core::balance_breakdown`], which follows the money;
//! `net_expense` is `max(expense - refund, 0)`.
//!
//! A wallet's opening balance is posted as an income (an expense when
//! negative) on Unallocated with the system category `Opening`, but it is
//! neither earned nor spent: [`Core::year_breakdown`] reads it for FONDO CASSA,
//! and [`Core::flow_person_totals`], [`Core::category_totals`],
//! [`Core::category_bucket_totals`] and [`Core::top_expenses`] leave it out.
//! [`Core::bucket_totals`] keeps it, a sum by kind over the whole ledger like
//! `period_totals`, and so does [`Core::balance_breakdown`], where it is money
//! that is there.
//!
//! [`Core::bucket_totals`], [`Core::year_breakdown`],
//! [`Core::category_bucket_totals`] and [`Core::balance_breakdown`] take the
//! boundaries of several consecutive ranges instead, so a whole year costs one
//! query.

use std::collections::BTreeMap;

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
    /// `transactions.person`: who the rows are for, the PERSONA column.
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
    /// `transactions.person`.
    pub person: String,
    pub amount: i64,
}

/// One person's movement inside one bucket of the year summary, split the way
/// the RIEPILOGO reads it.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct BucketPersonTotals {
    /// Index of the gap between consecutive `bounds`: 0 is `[b0, b1)`.
    pub bucket: u32,
    /// `transactions.person`.
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

/// One category's net expense inside one bucket: the months x categories
/// grid of the RIEPILOGO.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct BucketCategoryTotals {
    /// Index of the gap between consecutive `bounds`: 0 is `[b0, b1)`.
    pub bucket: u32,
    pub category_id: Uuid,
    pub name: String,
    /// `true` for `Opening` and `Uncategorized`: the app localizes the name.
    pub is_system: bool,
    /// `max(expense - refund, 0)` inside the bucket.
    pub net_expense: i64,
    /// Expense and refund rows behind the figure, voided excluded.
    pub count: u32,
}

/// Which side of the legs a balance breakdown reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TargetKind {
    Wallet,
    Flow,
}

/// What one wallet or envelope gained or lost inside one bucket: the signed
/// sum of its legs, transfers included, voided rows excluded. Bucket 0 of the
/// RIEPILOGO's bounds is everything before January, so it is the balance the
/// year starts with.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct BucketTargetTotals {
    /// Index of the gap between consecutive `bounds`: 0 is `[b0, b1)`.
    pub bucket: u32,
    /// The wallet or the envelope, by `target`.
    pub target_id: Uuid,
    pub delta: i64,
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

/// What [`BucketCategoryTotals`] is folded from: expenses and refunds stay
/// apart until the end, so the bucket nets once.
#[derive(Default)]
struct CategorySlot {
    name: String,
    is_system: bool,
    expense: i64,
    refund: i64,
    count: u32,
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

/// The bucket `at` falls in: bucket `i` is `[edges[i], edges[i + 1])`, so a
/// row exactly on an edge belongs to the later bucket. `None` outside
/// `[edges[0], edges[last])`.
fn bucket_of(edges: &[i64], at: i64) -> Option<usize> {
    // partition_point gives the first edge strictly greater than `at`; minus
    // one is the bucket that contains it.
    let bucket = edges.partition_point(|&edge| edge <= at).checked_sub(1)?;
    (bucket + 1 < edges.len()).then_some(bucket)
}

/// Narrows a [`scope`] to rows other than wallets' opening balances, for the
/// queries that join `categories c`: an opening balance is neither earned nor
/// spent.
fn without_openings((mut sql, mut args): (String, Vec<Value>)) -> (String, Vec<Value>) {
    sql.push_str(" AND NOT (c.is_system AND c.name_norm = ?)");
    args.push(Value::Text(OPENING_KEY.to_string()));
    (sql, args)
}

/// `legs.target_kind` of the side a [`TargetKind`] names.
const fn leg_kind(target: TargetKind) -> &'static str {
    match target {
        TargetKind::Wallet => "wallet",
        TargetKind::Flow => "flow",
    }
}

/// `WHERE` shared by every aggregation: one vault, live rows, entries only,
/// inside the range, optionally one person: who the rows are for
/// (`transactions.person`), not who recorded them.
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
        sql.push_str(" AND t.person = ?");
        args.push(Value::Text(person.to_string()));
    }
    (sql, args)
}

impl Core {
    /// Distinct persons of live transactions, ordered case-insensitively.
    /// Feeds the `TUTTI / ELISA / MATTEO` segmented control.
    pub fn people(&self, vault_id: Uuid) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT person FROM transactions
             WHERE vault_id = ?1 AND voided_at IS NULL
             ORDER BY lower(person)",
        )?;
        let rows = stmt
            .query_map([blob(vault_id)], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Envelope x person matrix over the range. Only pairs with movement are
    /// returned; the app fills the gaps with zeros. Wallets' opening balances
    /// are left out: an opening balance is neither earned nor spent, and
    /// [`Core::year_breakdown`] reads it for FONDO CASSA.
    pub fn flow_person_totals(
        &self,
        vault_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<FlowPersonTotals>> {
        check_range(from, to)?;
        let (where_sql, args) = without_openings(scope(vault_id, from, to, None));
        let sql = format!(
            "SELECT l.target_id, t.person, t.kind, SUM(ABS(l.amount))
             FROM transactions t
             JOIN legs l ON l.transaction_id = t.id AND l.target_kind = 'flow'
             JOIN categories c ON c.id = t.category_id
             {where_sql}
             GROUP BY l.target_id, t.person, t.kind
             ORDER BY lower(t.person)"
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

    /// Category breakdown over the range, heaviest net expense first. Wallets'
    /// opening balances are left out, so `Opening` never has a row: an
    /// opening balance is neither earned nor spent, and
    /// [`Core::year_breakdown`] reads it for FONDO CASSA.
    pub fn category_totals(
        &self,
        vault_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        person: Option<String>,
    ) -> Result<Vec<CategoryTotals>> {
        check_range(from, to)?;
        let (where_sql, args) = without_openings(scope(vault_id, from, to, person.as_deref()));
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
            if let Some((income, expense, refund)) =
                bucket_of(&edges, at).and_then(|bucket| sums.get_mut(bucket))
            {
                accumulate(&kind, amount, income, expense, refund);
            }
        }
        Ok(sums
            .into_iter()
            .map(|(income, expense, refund)| totals(income, expense, refund))
            .collect())
    }

    /// Bucket x person breakdown behind the RIEPILOGO.
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
            "SELECT t.occurred_at, t.person, t.kind, c.is_system, c.name_norm,
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

            // The scope keeps `at` inside the outer range, so there is always
            // a bucket.
            let Some(bucket) = bucket_of(&edges, at) else {
                continue;
            };
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

    /// Bucket x category net expense over the ranges `bounds` describe, for
    /// one person (`transactions.person`) or everybody: the RIEPILOGO's
    /// months x categories grid.
    ///
    /// Expense and refund rows only, live, transfers out; each (bucket,
    /// category) pair nets on its own, `max(expense - refund, 0)`, and
    /// `count` is the expense and refund rows behind it. Buckets follow
    /// [`Core::year_breakdown`]: bucket `i` is `[bounds[i], bounds[i + 1])`,
    /// so a row exactly on a boundary belongs to the later one. Only pairs
    /// with at least one row come back, a refund alone among them at zero.
    /// Ordered by bucket, then heaviest first, then by name
    /// case-insensitively, as [`Core::category_totals`] sorts.
    ///
    /// Wallets' opening balances are left out, so `Opening` never has a row:
    /// an opening balance is neither earned nor spent, and
    /// [`Core::year_breakdown`] reads it for FONDO CASSA.
    pub fn category_bucket_totals(
        &self,
        vault_id: Uuid,
        bounds: Vec<DateTime<Utc>>,
        person: Option<String>,
    ) -> Result<Vec<BucketCategoryTotals>> {
        check_bounds(&bounds, "category_bucket_totals")?;

        let edges: Vec<i64> = bounds.iter().map(|b| b.timestamp()).collect();
        let (where_sql, args) = without_openings(scope(
            vault_id,
            bounds[0],
            bounds[bounds.len() - 1],
            person.as_deref(),
        ));
        let sql = format!(
            "SELECT t.occurred_at, t.category_id, c.name, c.is_system, t.kind, t.amount
             FROM transactions t JOIN categories c ON c.id = t.category_id
             {where_sql} AND t.kind IN ('expense', 'refund')"
        );

        // Bucketed in Rust like year_breakdown: a year of a personal ledger
        // is a few thousand rows, and the SQL stays free of a CASE ladder.
        let mut slots: BTreeMap<(usize, Uuid), CategorySlot> = BTreeMap::new();
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(args.iter()))?;
        while let Some(row) = rows.next()? {
            let Some(bucket) = bucket_of(&edges, row.get(0)?) else {
                continue;
            };
            let category_id: Uuid = row.get(1)?;
            let name: String = row.get(2)?;
            let is_system: bool = row.get(3)?;
            let kind: String = row.get(4)?;
            let amount: i64 = row.get(5)?;

            let slot = slots
                .entry((bucket, category_id))
                .or_insert_with(|| CategorySlot {
                    name,
                    is_system,
                    ..CategorySlot::default()
                });
            match kind.as_str() {
                "expense" => slot.expense += amount,
                "refund" => slot.refund += amount,
                _ => {}
            }
            slot.count += 1;
        }

        let mut out: Vec<BucketCategoryTotals> = slots
            .into_iter()
            .map(|((bucket, category_id), slot)| BucketCategoryTotals {
                bucket: u32::try_from(bucket).unwrap_or(u32::MAX),
                category_id,
                name: slot.name,
                is_system: slot.is_system,
                net_expense: (slot.expense - slot.refund).max(0),
                count: slot.count,
            })
            .collect();
        out.sort_by(|a, b| {
            a.bucket
                .cmp(&b.bucket)
                .then_with(|| b.net_expense.cmp(&a.net_expense))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.category_id.cmp(&b.category_id))
        });
        Ok(out)
    }

    /// Bucket x wallet (or x envelope) signed movement over the ranges
    /// `bounds` describe: the sum of the `target` legs of every live row of
    /// the vault, of any kind, transfers included, voided rows out.
    ///
    /// Buckets follow [`Core::year_breakdown`]: bucket `i` is `[bounds[i],
    /// bounds[i + 1])`, so a row exactly on a boundary belongs to the later
    /// one. A pair comes back when the target has at least one leg in the
    /// bucket, even if its legs net to zero there; an archived wallet or
    /// envelope with legs in the range is among them. Ordered by bucket, then
    /// by `target_id` byte by byte (`Uuid`'s order, and the order SQLite
    /// sorts the id's BLOB in).
    ///
    /// Summing one target's deltas over buckets `0..=m` gives its balance at
    /// the end of bucket `m`, at `bounds[m + 1]`, as long as `bounds[0]` is
    /// before the vault's first row (the app passes the epoch). On the
    /// envelope side, the deltas of every envelope in a bucket sum to
    /// `income + opening - cash_expense - fund_expense` of
    /// [`Core::year_breakdown`] for that bucket, summed over its persons:
    /// transfers between envelopes net to zero. That holds while nobody's
    /// refunds in the bucket exceed their expenses on the same group of
    /// envelopes, where `year_breakdown` floors at zero.
    pub fn balance_breakdown(
        &self,
        vault_id: Uuid,
        bounds: Vec<DateTime<Utc>>,
        target: TargetKind,
    ) -> Result<Vec<BucketTargetTotals>> {
        check_bounds(&bounds, "balance_breakdown")?;

        let edges: Vec<i64> = bounds.iter().map(|b| b.timestamp()).collect();
        // Not `scope`: it keeps entries only, and here a transfer moves money
        // like any other row.
        let sql = "SELECT t.occurred_at, l.target_id, l.amount
                   FROM transactions t
                   JOIN legs l ON l.transaction_id = t.id AND l.target_kind = ?1
                   WHERE t.vault_id = ?2 AND t.voided_at IS NULL
                     AND t.occurred_at >= ?3 AND t.occurred_at < ?4";
        let args = [
            Value::Text(leg_kind(target).to_string()),
            blob(vault_id),
            Value::Integer(bounds[0].timestamp()),
            Value::Integer(bounds[bounds.len() - 1].timestamp()),
        ];

        // Bucketed in Rust like year_breakdown; the map's key order is the
        // output order.
        let mut deltas: BTreeMap<(usize, Uuid), i64> = BTreeMap::new();
        let mut stmt = self.conn.prepare(sql)?;
        let mut rows = stmt.query(params_from_iter(args.iter()))?;
        while let Some(row) = rows.next()? {
            let Some(bucket) = bucket_of(&edges, row.get(0)?) else {
                continue;
            };
            let target_id: Uuid = row.get(1)?;
            let amount: i64 = row.get(2)?;
            *deltas.entry((bucket, target_id)).or_default() += amount;
        }

        Ok(deltas
            .into_iter()
            .map(|((bucket, target_id), delta)| BucketTargetTotals {
                bucket: u32::try_from(bucket).unwrap_or(u32::MAX),
                target_id,
                delta,
            })
            .collect())
    }

    /// The heaviest expenses of the range, largest first. The opening balance
    /// of a wallet opened below zero is left out: an opening balance is
    /// neither earned nor spent, and [`Core::year_breakdown`] reads it for
    /// FONDO CASSA.
    pub fn top_expenses(
        &self,
        vault_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        person: Option<String>,
        limit: u32,
    ) -> Result<Vec<TopExpense>> {
        check_range(from, to)?;
        // The opening's argument goes in before the LIMIT's: both are
        // positional.
        let (where_sql, mut args) = without_openings(scope(vault_id, from, to, person.as_deref()));
        let sql = format!(
            "SELECT t.id, t.occurred_at, t.occurred_offset, t.note, c.name, c.is_system,
                    t.person, t.amount
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
