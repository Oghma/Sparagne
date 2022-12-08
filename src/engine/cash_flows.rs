//! A collection of types that implement `CashFlow` trait.
//!
//! The available cash flows are:
//! - [`UnBounded`]
//! - [`Bounded`]
//! - [`HardBounded`]
use super::entry::Entry;
use super::errors::EngineError;

/// `CashFlow` trait. Base requirements for a `CashFlow`.
pub trait CashFlow {
    fn add_entry(
        &mut self,
        balance: f64,
        category: String,
        note: String,
    ) -> Result<(), EngineError> {
        let entry = Entry::new(balance, category, note);
        self.insert(entry)
    }

    fn archive(&mut self);
    fn insert(&mut self, entry: Entry) -> Result<(), EngineError>;
}

/// An unlimited Cash flow. It has no upper limit.
#[derive(Debug)]
pub struct UnBounded {
    name: String,
    balance: f64,
    entries: Vec<Entry>,
    archived: bool,
}

impl UnBounded {
    pub fn new(name: String, balance: f64) -> Self {
        Self {
            name,
            balance,
            entries: Vec::new(),
            archived: false,
        }
    }
}

impl CashFlow for UnBounded {
    fn archive(&mut self) {
        self.archived = true
    }

    fn insert(&mut self, entry: Entry) -> Result<(), EngineError> {
        self.balance += entry.amount;
        self.entries.push(entry);
        Ok(())
    }
}

/// A "soft" bounded by `max_balance` Cash flow.
///
/// The **sum** of income **and** expenses cannot exceed max_balance.
#[derive(Debug)]
pub struct Bounded {
    name: String,
    balance: f64,
    max_balance: f64,
    entries: Vec<Entry>,
    archived: bool,
}

impl Bounded {
    pub fn new(name: String, balance: f64, max_balance: f64) -> Self {
        Self {
            name,
            balance,
            max_balance,
            entries: Vec::new(),
            archived: false,
        }
    }
}

impl CashFlow for Bounded {
    fn archive(&mut self) {
        self.archived = true
    }

    fn insert(&mut self, entry: Entry) -> Result<(), EngineError> {
        if entry.amount > 0f64 && self.balance + entry.amount > self.max_balance {
            Err(EngineError::MaxBalanceReached(self.name.clone()))
        } else {
            self.balance += entry.amount;
            self.entries.push(entry);
            Ok(())
        }
    }
}

/// An "hard" bounded by `max_balance` Cash Flow.
///
/// The **sum** of income cannot exceed max_balance. The expenses are ignored from the sum.
#[derive(Debug)]
pub struct HardBounded {
    name: String,
    balance: f64,
    max_balance: f64,
    total_balance: f64,
    entries: Vec<Entry>,
    archived: bool,
}

impl HardBounded {
    pub fn new(name: String, balance: f64, max_balance: f64) -> Self {
        Self {
            name,
            balance,
            max_balance,
            total_balance: balance,
            entries: Vec::new(),
            archived: false,
        }
    }
}

impl CashFlow for HardBounded {
    fn archive(&mut self) {
        self.archived = true
    }

    fn insert(&mut self, entry: Entry) -> Result<(), EngineError> {
        if entry.amount > 0f64 && self.total_balance + entry.amount > self.max_balance {
            Err(EngineError::MaxBalanceReached(self.name.clone()))
        } else {
            if entry.amount > 0f64 {
                self.total_balance += entry.amount;
            }
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
            Err(error) => assert_eq!(error, EngineError::MaxBalanceReached("Cash".to_string())),
            _ => panic!("Entry added"),
        }
        assert_eq!(flow.name, "Cash".to_string());
        assert_eq!(flow.balance, 0f64);
    }

    #[test]
    fn add_entry_hard_bounded() {
        let mut flow = HardBounded::new("Cash".to_string(), 0f64, 10f64);
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
    fn fail_add_entry_hard_bounded() {
        let mut flow = HardBounded::new("Cash".to_string(), 0f64, 3f64);
        match flow.add_entry(4.44, "Income".to_string(), "Weekly".to_string()) {
            Err(error) => assert_eq!(error, EngineError::MaxBalanceReached("Cash".to_string())),
            _ => panic!("Entry added"),
        }
        assert_eq!(flow.name, "Cash".to_string());
        assert_eq!(flow.balance, 0f64);
    }

    #[test]
    fn check_bounded_archived() {
        let mut flow = Bounded::new("Cash".to_string(), 0f64, 3f64);
        assert_eq!(flow.archived, false);
        flow.archive();
        assert_eq!(flow.archived, true);
    }

    #[test]
    fn check_unbounded_archived() {
        let mut flow = UnBounded::new("Cash".to_string(), 0f64);
        assert_eq!(flow.archived, false);
        flow.archive();
        assert_eq!(flow.archived, true);
    }

    #[test]
    fn check_hard_bounded_archived() {
        let mut flow = HardBounded::new("Cash".to_string(), 0f64, 3f64);
        assert_eq!(flow.archived, false);
        flow.archive();
        assert_eq!(flow.archived, true);
    }
}
