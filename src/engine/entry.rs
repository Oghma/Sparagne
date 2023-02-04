//! The module contains the `Entry` type representing an entry in cash flows and wallets.
//!
//! Both expenses and income are represented by `Entry` type.
use uuid::Uuid;

use super::sqlite3::Queryable;

/// Represent a movement, an entry in cash flows or wallets.
#[derive(Debug)]
pub struct Entry {
    pub id: String,
    pub amount: f64,
    pub category: String,
    pub note: String,
    pub cash_flow: String,
}

/// Type used to represent an entry in cash flows and wallets.
impl Entry {
    pub fn new(amount: f64, category: String, note: String, cash_flow: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            amount,
            category,
            note,
            cash_flow,
        }
    }
}

impl Queryable for Entry {
    fn table() -> &'static str
    where
        Self: Sized,
    {
        "Entries"
    }

    fn keys() -> Vec<&'static str>
    where
        Self: Sized,
    {
        vec!["id", "amount", "category", "note", "cashFlow"]
    }

    fn values(&self) -> Vec<&dyn rusqlite::ToSql> {
        vec![
            &self.id,
            &self.amount,
            &self.category,
            &self.note,
            &self.cash_flow,
        ]
    }

    fn from_row(row: &rusqlite::Row) -> Self {
        Self {
            id: row.get(0).unwrap(),
            amount: row.get(1).unwrap(),
            category: row.get(2).unwrap(),
            note: row.get(3).unwrap(),
            cash_flow: row.get(4).unwrap(),
        }
    }
}
