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

/// A "soft" bounded by `max_balance` Cash flow.
///
/// The **sum** of income **and** expenses cannot exceed max_balance.
#[derive(Debug)]
struct Bounded {
    name: String,
    balance: f64,
    max_balance: f64,
    entries: Vec<Entry>,
}

impl Bounded {
    fn new(name: String, balance: f64, max_balance: f64) -> Self {
        Self {
            name,
            balance,
            max_balance,
            entries: Vec::new(),
        }
    }
}

impl CashFlow for Bounded {
    fn insert(&mut self, entry: Entry) -> Result<(), &str> {
        if entry.amount > 0f64 && self.balance + entry.amount > self.max_balance {
            Err("Max balance reached!")
        } else {
            self.balance += entry.amount;
            self.entries.push(entry);
            Ok(())
        }
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

    #[test]
    fn add_entry_bounded() {
        let mut flow = Bounded::new("Cash".to_string(), 0f64, 10f64);
        flow.add_entry(1.23, "Income".to_string(), "Weekly".to_string())
            .unwrap();
        let entry = &flow.entries[0];
        assert_eq!(flow.name, "Cash".to_string());
        assert_eq!(flow.balance, 1.23);
        assert_eq!(entry.amount, 1.23);
        assert_eq!(entry.category, "Income".to_string());
        assert_eq!(entry.note, "Weekly".to_string());
    }

    #[test]
    fn fail_add_entry_bounded() {
        let mut flow = Bounded::new("Cash".to_string(), 0f64, 3f64);
        match flow.add_entry(4.44, "Income".to_string(), "Weekly".to_string()) {
            Err(error) => assert_eq!(error, "Max balance reached!"),
            _ => panic!("Entry added"),
        }
        assert_eq!(flow.name, "Cash".to_string());
        assert_eq!(flow.balance, 0f64);
    }
}
