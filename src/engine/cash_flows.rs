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
    fn delete_entry(&mut self, id: &uuid::Uuid) -> Result<(), EngineError>;
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

    fn delete_entry(&mut self, id: &uuid::Uuid) -> Result<(), EngineError> {
        match self.entries.iter().position(|entry| entry.id == *id) {
            Some(index) => {
                let entry = self.entries.remove(index);
                self.balance -= entry.amount;
                Ok(())
            }
            None => Err(EngineError::KeyNotFound(id.to_string())),
        }
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

    fn delete_entry(&mut self, id: &uuid::Uuid) -> Result<(), EngineError> {
        match self.entries.iter().position(|entry| entry.id == *id) {
            Some(index) => {
                let entry = self.entries.remove(index);
                self.balance -= entry.amount;
                Ok(())
            }
            None => Err(EngineError::KeyNotFound(id.to_string())),
        }
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

    fn delete_entry(&mut self, id: &uuid::Uuid) -> Result<(), EngineError> {
        match self.entries.iter().position(|entry| entry.id == *id) {
            Some(index) => {
                let entry = self.entries.remove(index);
                self.balance -= entry.amount;
                Ok(())
            }
            None => Err(EngineError::KeyNotFound(id.to_string())),
        }
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

    fn bounded_flow() -> Bounded {
        Bounded::new(String::from("Cash"), 0f64, 10f64)
    }

    fn unbounded_flow() -> UnBounded {
        UnBounded::new(String::from("Cash"), 0f64)
    }

    fn hard_bounded_flow() -> HardBounded {
        HardBounded::new(String::from("Cash"), 0f64, 10f64)
    }

    #[test]
    fn add_entry_unbounded() {
        let mut flow = unbounded_flow();
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
        let mut flow = bounded_flow();
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
    #[should_panic(expected = "MaxBalanceReached(\"Cash\")")]
    fn fail_add_entry_bounded() {
        let mut flow = bounded_flow();
        flow.add_entry(20.44, "Income".to_string(), "Weekly".to_string())
            .unwrap()
    }

    #[test]
    fn add_entry_hard_bounded() {
        let mut flow = hard_bounded_flow();
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
    #[should_panic(expected = "MaxBalanceReached(\"Cash\")")]
    fn fail_add_entry_hard_bounded() {
        let mut flow = hard_bounded_flow();
        flow.add_entry(20.44, "Income".to_string(), "Weekly".to_string())
            .unwrap()
    }

    #[test]
    fn check_bounded_archived() {
        let mut flow = bounded_flow();
        assert_eq!(flow.archived, false);
        flow.archive();
        assert_eq!(flow.archived, true);
    }

    #[test]
    fn check_unbounded_archived() {
        let mut flow = unbounded_flow();
        assert_eq!(flow.archived, false);
        flow.archive();
        assert_eq!(flow.archived, true);
    }

    #[test]
    fn check_hard_bounded_archived() {
        let mut flow = hard_bounded_flow();
        assert_eq!(flow.archived, false);
        flow.archive();
        assert_eq!(flow.archived, true);
    }

    #[test]
    fn delete_entry_hard_bounded() {
        let mut flow = hard_bounded_flow();
        flow.add_entry(1.23, "Income".to_string(), "Weekly".to_string())
            .unwrap();
        let entry_id = flow.entries[0].id;
        flow.delete_entry(&entry_id).unwrap();

        assert_eq!(flow.balance, 0f64);
        assert_eq!(flow.entries.is_empty(), true)
    }

    #[test]
    fn delete_entry_unbounded() {
        let mut flow = unbounded_flow();
        flow.add_entry(1.23, "Income".to_string(), "Weekly".to_string())
            .unwrap();
        let entry_id = flow.entries[0].id;
        flow.delete_entry(&entry_id).unwrap();

        assert_eq!(flow.balance, 0f64);
        assert_eq!(flow.entries.is_empty(), true)
    }

    #[test]
    fn delete_entry_bounded() {
        let mut flow = bounded_flow();
        flow.add_entry(1.23, "Income".to_string(), "Weekly".to_string())
            .unwrap();
        let entry_id = flow.entries[0].id;
        flow.delete_entry(&entry_id).unwrap();

        assert_eq!(flow.balance, 0f64);
        assert_eq!(flow.entries.is_empty(), true)
    }
}
