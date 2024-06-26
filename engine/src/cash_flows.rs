//! The module contains the representation of a cash flow.
//!
use std::time::Duration;

use sea_orm::entity::{prelude::*, ActiveValue};
use serde::{Deserialize, Serialize};

use super::entry::Entry;
use super::error::EngineError;
use super::ResultEngine;

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
#[derive(Clone, Debug, Serialize, Deserialize)]
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
        date: Duration,
    ) -> ResultEngine<&Entry> {
        let entry = Entry::new(balance, category, note, date);
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
        self.entries.push(entry);

        Ok(&self.entries[self.entries.len() - 1])
    }

    pub fn archive(&mut self) {
        self.archived = true;
    }

    pub fn delete_entry(&mut self, id: &str) -> ResultEngine<Entry> {
        match self.entries.iter().position(|entry| entry.id == id) {
            Some(index) => {
                let entry = self.entries.remove(index);
                self.balance -= entry.amount;

                if entry.amount > 0f64 {
                    self.income_balance = self.income_balance.map(|balance| balance - entry.amount);
                }

                Ok(entry)
            }
            None => Err(EngineError::KeyNotFound(id.to_string())),
        }
    }

    pub fn update_entry(
        &mut self,
        id: &str,
        amount: f64,
        category: String,
        note: String,
    ) -> ResultEngine<&Entry> {
        match self.entries.iter().position(|entry| entry.id == id) {
            Some(index) => {
                let entry = &mut self.entries[index];
                let new_balance = self.balance - entry.amount + amount;

                if let Some(bound) = self.max_balance {
                    if let Some(income_balance) = self.income_balance {
                        // Check if the entry or the update is an income and if
                        // the updates does not exceed `max_balance`
                        if (entry.amount > 0f64 || amount > 0f64)
                            && income_balance - entry.amount + amount > bound
                        {
                            return Err(EngineError::MaxBalanceReached(self.name.clone()));
                        }
                    } else if new_balance > bound {
                        return Err(EngineError::MaxBalanceReached(self.name.clone()));
                    }
                }

                self.balance = new_balance;

                entry.amount = amount;
                entry.category = category;
                entry.note = note;

                Ok(entry)
            }
            None => Err(EngineError::KeyNotFound(id.to_string())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "cash_flows")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub name: String,
    #[sea_orm(column_type = "Double")]
    pub balance: f64,
    #[sea_orm(column_type = "Double", nullable)]
    pub max_balance: Option<f64>,
    #[sea_orm(column_type = "Double", nullable)]
    pub income_balance: Option<f64>,
    pub archived: bool,
    #[sea_orm(primary_key, auto_increment = false)]
    pub vault_id: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::entry::Entity")]
    Entries,
    #[sea_orm(
        belongs_to = "super::vault::Entity",
        from = "Column::VaultId",
        to = "super::vault::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    Vaults,
}

impl Related<super::entry::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Entries.def()
    }
}

impl Related<super::vault::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Vaults.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

impl From<&CashFlow> for ActiveModel {
    fn from(flow: &CashFlow) -> Self {
        Self {
            name: ActiveValue::Set(flow.name.clone()),
            balance: ActiveValue::Set(flow.balance),
            max_balance: ActiveValue::Set(flow.max_balance),
            income_balance: ActiveValue::Set(flow.income_balance),
            archived: ActiveValue::Set(flow.archived),
            vault_id: ActiveValue::NotSet,
        }
    }
}

impl From<&mut CashFlow> for ActiveModel {
    fn from(flow: &mut CashFlow) -> Self {
        Self {
            name: ActiveValue::Set(flow.name.clone()),
            balance: ActiveValue::Set(flow.balance),
            max_balance: ActiveValue::Set(flow.max_balance),
            income_balance: ActiveValue::Set(flow.income_balance),
            archived: ActiveValue::Set(flow.archived),
            vault_id: ActiveValue::NotSet,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn bounded() -> CashFlow {
        CashFlow::new(String::from("Cash"), 0f64, Some(10.0), Some(true))
    }

    fn unbounded() -> CashFlow {
        CashFlow::new(String::from("Cash"), 0f64, None, None)
    }

    #[test]
    fn add_entry() {
        let mut flow = unbounded();
        flow.add_entry(
            1.23,
            String::from("Income"),
            String::from("First"),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
        )
        .unwrap();
        let entry = &flow.entries[0];

        assert_eq!(flow.name, "Cash".to_string());
        assert_eq!(flow.balance, 1.23);
        assert_eq!(entry.amount, 1.23);
        assert_eq!(entry.category, "Income".to_string());
    }

    #[test]
    fn delete_entry() {
        let mut flow = unbounded();
        flow.add_entry(
            1.23,
            "Income".to_string(),
            "Weekly".to_string(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
        )
        .unwrap();
        let entry_id = flow.entries[0].id.clone();
        flow.delete_entry(&entry_id).unwrap();

        assert_eq!(flow.balance, 0f64);
        assert!(flow.entries.is_empty())
    }

    #[test]
    fn update_entry() {
        let mut flow = unbounded();
        flow.add_entry(
            1.23,
            "Income".to_string(),
            "Weekly".to_string(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
        )
        .unwrap();
        let entry_id = flow.entries[0].id.clone();

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
    fn fail_add_entry() {
        let mut flow = bounded();
        flow.add_entry(
            20.44,
            "Income".to_string(),
            "Weekly".to_string(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
        )
        .unwrap();
    }

    #[test]
    #[should_panic(expected = "MaxBalanceReached(\"Cash\")")]
    fn fail_update_entry() {
        let mut flow = bounded();
        flow.add_entry(
            1.23,
            "Income".to_string(),
            "Weekly".to_string(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
        )
        .unwrap();
        let entry_id = flow.entries[0].id.clone();

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
        let mut flow = bounded();
        flow.add_entry(
            -1.23,
            "Income".to_string(),
            "Weekly".to_string(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
        )
        .unwrap();
        let entry_id = flow.entries[0].id.clone();

        flow.update_entry(
            &entry_id,
            20f64,
            String::from("Income"),
            String::from("Monthly"),
        )
        .unwrap();
    }
}
