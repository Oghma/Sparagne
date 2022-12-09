//! A collection of types that implement `CashFlow` trait.
//!
//! The available cash flows are:
//! - [`UnBounded`]
//! - [`Bounded`]
//! - [`HardBounded`]
use crate::entry::Entry;

/// `CashFlow` trait. Base requirements for a `CashFlow`.
trait CashFlow {
    fn add_entry(&mut self, balance: f64, category: String, note: String) -> Result<(), &str> {
        let entry = Entry::new(balance, category, note);
        self.insert(entry)
    }

    fn insert(&mut self, entry: Entry) -> Result<(), &str>;
}

/// An unlimited Cash flow. It has no upper limit.
#[derive(Debug)]
struct UnBounded {
    name: String,
    balance: f64,
    entries: Vec<Entry>,
}

impl UnBounded {
    fn new(name: String, balance: f64) -> Self {
        Self {
            name,
            balance,
            entries: Vec::new(),
        }
    }
}

impl CashFlow for UnBounded {
    fn insert(&mut self, entry: Entry) -> Result<(), &str> {
        self.balance += entry.amount;
        self.entries.push(entry);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_entry_unbounded() {
        let mut flow = UnBounded::new("Cash".to_string(), 0f64);
        flow.add_entry(1.23, "Income".to_string(), "Weekly".to_string())
            .unwrap();
        let entry = &flow.entries[0];
        assert_eq!(flow.name, "Cash".to_string());
        assert_eq!(flow.balance, 1.23);
        assert_eq!(entry.amount, 1.23);
        assert_eq!(entry.category, "Income".to_string());
        assert_eq!(entry.note, "Weekly".to_string());
    }
}
