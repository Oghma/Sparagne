//! A collection of types that implement `CashFlow` trait.
//!
//! The available cash flows are:
//! - [`UnBounded`]
//! - [`Bounded`]
//! - [`HardBounded`]
use super::entry::Entry;
use super::errors::EngineError;
pub use unbounded::UnBounded;

mod unbounded;

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
    fn update_entry(
        &mut self,
        id: &uuid::Uuid,
        balance: f64,
        category: String,
        note: String,
    ) -> Result<(), EngineError>;
    fn insert(&mut self, entry: Entry) -> Result<(), EngineError>;
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

    fn update_entry(
        &mut self,
        id: &uuid::Uuid,
        amount: f64,
        category: String,
        note: String,
    ) -> Result<(), EngineError> {
        match self.entries.iter().position(|entry| entry.id == *id) {
            Some(index) => {
                let entry = &mut self.entries[index];
                let new_balance = self.balance - entry.amount + amount;

                if new_balance > self.max_balance {
                    return Err(EngineError::MaxBalanceReached(self.name.clone()));
                }

                self.balance = new_balance;
                entry.amount = amount;
                entry.category = category;
                entry.note = note;

                Ok(())
            }
            None => Err(EngineError::KeyNotFound(id.to_string())),
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

    fn update_entry(
        &mut self,
        id: &uuid::Uuid,
        amount: f64,
        category: String,
        note: String,
    ) -> Result<(), EngineError> {
        match self.entries.iter().position(|entry| entry.id == *id) {
            Some(index) => {
                let entry = &mut self.entries[index];
                // Check if the entry or the update is an income and if the updates does not
                // exceed `max_balance`
                if entry.amount > 0f64 || amount > 0f64 {
                    if self.total_balance - entry.amount + amount > self.max_balance {
                        return Err(EngineError::MaxBalanceReached(self.name.clone()));
                    }
                    self.total_balance = self.total_balance - entry.amount + amount;
                }

                self.balance = self.balance - entry.amount + amount;
                entry.amount = amount;
                entry.category = category;
                entry.note = note;

                Ok(())
            }
            None => Err(EngineError::KeyNotFound(id.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounded_flow() -> Bounded {
        Bounded::new(String::from("Cash"), 0f64, 10f64)
    }

    fn hard_bounded_flow() -> HardBounded {
        HardBounded::new(String::from("Cash"), 0f64, 10f64)
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
    fn delete_entry_bounded() {
        let mut flow = bounded_flow();
        flow.add_entry(1.23, "Income".to_string(), "Weekly".to_string())
            .unwrap();
        let entry_id = flow.entries[0].id;
        flow.delete_entry(&entry_id).unwrap();

        assert_eq!(flow.balance, 0f64);
        assert_eq!(flow.entries.is_empty(), true)
    }

    #[test]
    fn update_entry_bounded() {
        let mut flow = bounded_flow();
        flow.add_entry(1.23, "Income".to_string(), "Weekly".to_string())
            .unwrap();
        let entry_id = flow.entries[0].id;

        flow.update_entry(
            &entry_id,
            10f64,
            String::from("Income"),
            String::from("Monthly"),
        )
        .unwrap();
        let entry = &flow.entries[0];

        assert_eq!(flow.balance, 10f64);
        assert_eq!(entry.amount, 10f64);
        assert_eq!(entry.category, String::from("Income"));
        assert_eq!(entry.note, String::from("Monthly"))
    }

    #[test]
    fn update_entry_hard_bounded() {
        let mut flow = hard_bounded_flow();
        flow.add_entry(1.23, "Income".to_string(), "Weekly".to_string())
            .unwrap();
        let entry_id = flow.entries[0].id;

        flow.update_entry(
            &entry_id,
            10f64,
            String::from("Income"),
            String::from("Monthly"),
        )
        .unwrap();
        let entry = &flow.entries[0];

        assert_eq!(flow.balance, 10f64);
        assert_eq!(entry.amount, 10f64);
        assert_eq!(entry.category, String::from("Income"));
        assert_eq!(entry.note, String::from("Monthly"))
    }

    #[test]
    #[should_panic(expected = "MaxBalanceReached(\"Cash\")")]
    fn fail_update_entry_bounded() {
        let mut flow = bounded_flow();
        flow.add_entry(1.23, "Income".to_string(), "Weekly".to_string())
            .unwrap();
        let entry_id = flow.entries[0].id;

        flow.update_entry(
            &entry_id,
            20f64,
            String::from("Income"),
            String::from("Monthly"),
        )
        .unwrap();
    }

    #[test]
    #[should_panic(expected = "MaxBalanceReached(\"Cash\")")]
    fn fail_update_entry_hard_bounded() {
        let mut flow = hard_bounded_flow();
        flow.add_entry(1.23, "Income".to_string(), "Weekly".to_string())
            .unwrap();
        let entry_id = flow.entries[0].id;

        flow.update_entry(
            &entry_id,
            20f64,
            String::from("Income"),
            String::from("Monthly"),
        )
        .unwrap();
    }

    #[test]
    #[should_panic(expected = "MaxBalanceReached(\"Cash\")")]
    fn fail_update_income_expense_switch() {
        let mut flow = hard_bounded_flow();
        flow.add_entry(-1.23, "Income".to_string(), "Weekly".to_string())
            .unwrap();
        let entry_id = flow.entries[0].id;

        flow.update_entry(
            &entry_id,
            20f64,
            String::from("Income"),
            String::from("Monthly"),
        )
        .unwrap();
    }
}
