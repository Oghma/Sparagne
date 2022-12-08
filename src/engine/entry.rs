//! The module contains the `Entry` type representing an entry in cash flows and wallets.
//!
//! Both expenses and income are represented by `Entry` type.
use uuid::Uuid;

#[derive(Debug)]
pub struct Entry {
    pub id: Uuid,
    pub amount: f64,
    pub category: String,
    pub note: String,
}

/// Type used to represent an entry in cash flows and wallets.
impl Entry {
    pub fn new(amount: f64, category: String, note: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            amount,
            category,
            note,
        }
    }
}
