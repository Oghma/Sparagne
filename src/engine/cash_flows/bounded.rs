//! `Bounded` Cashflow type. The ideal type for cash flow that require an upper
//! bound.

use super::{CashFlow, EngineError, Entry};

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

#[cfg(test)]
mod tests {
    use super::*;

    fn bounded() -> Bounded {
        Bounded::new(String::from("Cash"), 0f64, 10f64)
    }

    #[test]
    fn add_entry() {
        let mut flow = bounded();
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
    fn fail_add_entry() {
        let mut flow = bounded();
        flow.add_entry(20.44, "Income".to_string(), "Weekly".to_string())
            .unwrap()
    }

    #[test]
    fn delete_entry() {
        let mut flow = bounded();
        flow.add_entry(1.23, "Income".to_string(), "Weekly".to_string())
            .unwrap();
        let entry_id = flow.entries[0].id;
        flow.delete_entry(&entry_id).unwrap();

        assert_eq!(flow.balance, 0f64);
        assert_eq!(flow.entries.is_empty(), true)
    }

    #[test]
    fn update_entry() {
        let mut flow = bounded();
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
    fn fail_update_entry() {
        let mut flow = bounded();
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
}
