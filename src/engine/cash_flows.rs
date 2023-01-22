//! A collection of types that implement `CashFlow` trait.
//!
//! The available cash flows are:
//! - [`UnBounded`]
//! - [`Bounded`]
//! - [`HardBounded`]
use super::entry::Entry;
use super::errors::EngineError;
pub use bounded::Bounded;
pub use hard_bounded::HardBounded;
pub use unbounded::UnBounded;

mod bounded;
mod hard_bounded;
mod unbounded;

/// A cash flow.
///
/// Based to the need, it is possibile to create an unlimited, bounded or income
/// bounded cash flow. Unlimited or unbounded means cash flow without an upper
/// bound.
///
/// Bounded means the cash flow has an upper bound and that takes into account
/// expenses **and** income. Let $max_balance$ the upper bound, the constraint
/// is: $incomes + expenses <= max_balance$
///
/// Income bounded means the cash flow has an upper bound that it ignores
/// expenses. **Only** incomes are checked. Let $max_balance$ the upper bound,
/// the constraint is $incomes <= max_balance$.
///
/// ** Examples
///
/// Suppose a cash flow with a max balance of 10 and a balance of 5. A new
/// expense with value 2 is inserted bringing the balance to 3.
///
/// With a bounded cash flow the constraint is $5 + -3 <= 10$ accepting an
/// income of maximum 7.
///
/// With a income bounded cash flow, the constraint is $5 <= 10$ accepting an
/// income of maximum 7.
#[derive(Debug)]
pub struct CashFlow {
    pub name: String,
    pub balance: f64,
    pub max_balance: Option<f64>,
    pub income_balance: Option<f64>,
    pub entries: Vec<Entry>,
    pub archived: bool,
}

impl CashFlow {
    pub fn new(
        name: String,
        balance: f64,
        max_balance: Option<f64>,
        income_bounded: Option<bool>,
    ) -> Self {
        let income_balance = match income_bounded {
            Some(true) => Some(max_balance.unwrap()),
            _ => None,
        };

        Self {
            name,
            balance,
            max_balance,
            income_balance,
            entries: Vec::new(),
            archived: false,
        }
    }

    pub fn add_entry(
        &mut self,
        balance: f64,
        category: String,
        note: String,
    ) -> Result<uuid::Uuid, EngineError> {
        let entry = Entry::new(balance, category, note);
        // If bounded, check constraints are respected
        if entry.amount > 0f64 {
            if let Some(bound) = self.max_balance {
                if let Some(income_balance) = self.income_balance {
                    if income_balance + entry.amount > bound {
                        return Err(EngineError::MaxBalanceReached(self.name.clone()));
                    }
                    self.income_balance = Some(income_balance + entry.amount);
                } else if self.balance + entry.amount > bound {
                    return Err(EngineError::MaxBalanceReached(self.name.clone()));
                }
            }
        }

        self.balance += entry.amount;
        let entry_id = entry.id;
        self.entries.push(entry);

        Ok(entry_id)
    }

    pub fn archive(&mut self) {
        self.archived = true;
    }

    pub fn delete_entry(&mut self, id: &uuid::Uuid) -> Result<(), EngineError> {
        match self.entries.iter().position(|entry| entry.id == *id) {
            Some(index) => {
                let entry = self.entries.remove(index);
                self.balance -= entry.amount;

                if entry.amount > 0f64 {
                    self.income_balance = self.income_balance.map(|balance| balance - entry.amount);
                }

                Ok(())
            }
            None => Err(EngineError::KeyNotFound(id.to_string())),
        }
    }

    pub fn update_entry(
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

                if let Some(bound) = self.max_balance {
                    if let Some(income_balance) = self.income_balance {
                        // Check if the entry or the update is an income and if
                        // the updates does not exceed `max_balance`
                        if entry.amount > 0f64 || amount > 0f64 {
                            if income_balance - entry.amount + amount > bound {
                                return Err(EngineError::MaxBalanceReached(self.name.clone()));
                            }
                        }
                    } else if new_balance > bound {
                        return Err(EngineError::MaxBalanceReached(self.name.clone()));
                    }
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

/// `CashFlow` trait. Base requirements for a `CashFlow`.
pub trait CashFlow {
    fn add_entry(
        &mut self,
        balance: f64,
        category: String,
        note: String,
    ) -> Result<uuid::Uuid, EngineError> {
        let entry = Entry::new(balance, category, note);
        self.insert(entry)
    }

    fn archive(&mut self);

    fn archived(&self) -> bool;

    fn delete_entry(&mut self, id: &uuid::Uuid) -> Result<(), EngineError>;

    fn update_entry(
        &mut self,
        id: &uuid::Uuid,
        balance: f64,
        category: String,
        note: String,
    ) -> Result<(), EngineError>;

    fn insert(&mut self, entry: Entry) -> Result<uuid::Uuid, EngineError>;
}
