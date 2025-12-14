//! The module contains the representation of a cash flow.
use chrono::{DateTime, Utc};

use sea_orm::entity::{ActiveValue, prelude::*};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{ResultEngine, entry::Entry, error::EngineError};
use crate::Currency;

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
/// Amounts are stored as integer minor units (`i64`), and formatted using the
/// vault currency.
///
/// Example with `EUR` (2 minor units):
/// max balance = 10.00 EUR (1000), current balance = 5.00 EUR (500).
/// A new expense of 2.00 EUR (200) brings the balance to 3.00 EUR (300).
///
/// With a bounded cash flow the constraint is $5 + -3 <= 10$ accepting an
/// income of maximum 7.
///
/// With a income bounded cash flow, the constraint is $5 <= 10$ accepting an
/// income of maximum 7.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CashFlow {
    /// Stable identifier for this cash flow.
    ///
    /// This is a UUID generated once and persisted in the database, so the cash
    /// flow can be renamed without breaking references.
    pub id: Uuid,
    pub name: String,
    pub balance: i64,
    pub max_balance: Option<i64>,
    pub income_balance: Option<i64>,
    pub currency: Currency,
    pub entries: Vec<Entry>,
    pub archived: bool,
}

impl CashFlow {
    pub fn new(
        name: String,
        balance: i64,
        max_balance: Option<i64>,
        income_bounded: Option<bool>,
        currency: Currency,
    ) -> Self {
        let income_balance = match income_bounded {
            Some(true) => Some(max_balance.unwrap()),
            _ => None,
        };

        Self {
            id: Uuid::new_v4(),
            name,
            balance,
            max_balance,
            income_balance,
            currency,
            entries: Vec::new(),
            archived: false,
        }
    }

    pub fn with_id(
        id: Uuid,
        name: String,
        balance: i64,
        max_balance: Option<i64>,
        income_bounded: Option<bool>,
        currency: Currency,
    ) -> Self {
        let income_balance = match income_bounded {
            Some(true) => Some(max_balance.unwrap()),
            _ => None,
        };

        Self {
            id,
            name,
            balance,
            max_balance,
            income_balance,
            currency,
            entries: Vec::new(),
            archived: false,
        }
    }

    pub fn add_entry(
        &mut self,
        amount_minor: i64,
        category: String,
        note: String,
        date: DateTime<Utc>,
    ) -> ResultEngine<&Entry> {
        let entry = Entry::new(amount_minor, self.currency, category, note, date);
        // If bounded, check constraints are respected
        if entry.amount_minor > 0
            && let Some(bound) = self.max_balance
        {
            if let Some(income_balance) = self.income_balance {
                if income_balance + entry.amount_minor > bound {
                    return Err(EngineError::MaxBalanceReached(self.name.clone()));
                }
                self.income_balance = Some(income_balance + entry.amount_minor);
            } else if self.balance + entry.amount_minor > bound {
                return Err(EngineError::MaxBalanceReached(self.name.clone()));
            }
        }

        self.balance += entry.amount_minor;
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
                self.balance -= entry.amount_minor;

                if entry.amount_minor > 0 {
                    self.income_balance = self
                        .income_balance
                        .map(|balance| balance - entry.amount_minor);
                }

                Ok(entry)
            }
            None => Err(EngineError::KeyNotFound(id.to_string())),
        }
    }

    pub fn update_entry(
        &mut self,
        id: &str,
        amount_minor: i64,
        category: String,
        note: String,
    ) -> ResultEngine<&Entry> {
        match self.entries.iter().position(|entry| entry.id == id) {
            Some(index) => {
                let entry = &mut self.entries[index];
                let new_balance = self.balance - entry.amount_minor + amount_minor;

                if let Some(bound) = self.max_balance {
                    if let Some(income_balance) = self.income_balance {
                        // Check if the entry or the update is an income and if
                        // the updates does not exceed `max_balance`
                        if (entry.amount_minor > 0 || amount_minor > 0)
                            && income_balance - entry.amount_minor + amount_minor > bound
                        {
                            return Err(EngineError::MaxBalanceReached(self.name.clone()));
                        }
                    } else if new_balance > bound {
                        return Err(EngineError::MaxBalanceReached(self.name.clone()));
                    }
                }

                self.balance = new_balance;

                entry.amount_minor = amount_minor;
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
    pub id: String,
    pub name: String,
    pub balance: i64,
    pub max_balance: Option<i64>,
    pub income_balance: Option<i64>,
    pub currency: String,
    pub archived: bool,
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
            id: ActiveValue::Set(flow.id.to_string()),
            name: ActiveValue::Set(flow.name.clone()),
            balance: ActiveValue::Set(flow.balance),
            max_balance: ActiveValue::Set(flow.max_balance),
            income_balance: ActiveValue::Set(flow.income_balance),
            currency: ActiveValue::Set(flow.currency.code().to_string()),
            archived: ActiveValue::Set(flow.archived),
            vault_id: ActiveValue::NotSet,
        }
    }
}

impl From<&mut CashFlow> for ActiveModel {
    fn from(flow: &mut CashFlow) -> Self {
        Self {
            id: ActiveValue::Set(flow.id.to_string()),
            name: ActiveValue::Set(flow.name.clone()),
            balance: ActiveValue::Set(flow.balance),
            max_balance: ActiveValue::Set(flow.max_balance),
            income_balance: ActiveValue::Set(flow.income_balance),
            currency: ActiveValue::Set(flow.currency.code().to_string()),
            archived: ActiveValue::Set(flow.archived),
            vault_id: ActiveValue::NotSet,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn bounded() -> CashFlow {
        CashFlow::new(
            String::from("Cash"),
            0,
            Some(1000),
            Some(true),
            Currency::Eur,
        )
    }

    fn unbounded() -> CashFlow {
        CashFlow::new(String::from("Cash"), 0, None, None, Currency::Eur)
    }

    #[test]
    fn add_entry() {
        let mut flow = unbounded();
        flow.add_entry(
            123,
            String::from("Income"),
            String::from("First"),
            Utc.timestamp_opt(0, 0).unwrap(),
        )
        .unwrap();
        let entry = &flow.entries[0];

        assert_eq!(flow.name, "Cash".to_string());
        assert_eq!(flow.balance, 123);
        assert_eq!(entry.amount_minor, 123);
        assert_eq!(entry.currency, Currency::Eur);
        assert_eq!(entry.category, "Income".to_string());
    }

    #[test]
    fn delete_entry() {
        let mut flow = unbounded();
        flow.add_entry(
            123,
            "Income".to_string(),
            "Weekly".to_string(),
            Utc.timestamp_opt(0, 0).unwrap(),
        )
        .unwrap();
        let entry_id = flow.entries[0].id.clone();
        flow.delete_entry(&entry_id).unwrap();

        assert_eq!(flow.balance, 0);
        assert!(flow.entries.is_empty())
    }

    #[test]
    fn update_entry() {
        let mut flow = unbounded();
        flow.add_entry(
            123,
            "Income".to_string(),
            "Weekly".to_string(),
            Utc.timestamp_opt(0, 0).unwrap(),
        )
        .unwrap();
        let entry_id = flow.entries[0].id.clone();

        flow.update_entry(
            &entry_id,
            1000,
            String::from("Income"),
            String::from("Monthly"),
        )
        .unwrap();
        let entry = &flow.entries[0];

        assert_eq!(flow.balance, 1000);
        assert_eq!(entry.amount_minor, 1000);
        assert_eq!(entry.category, String::from("Income"));
        assert_eq!(entry.note, String::from("Monthly"))
    }

    #[test]
    #[should_panic(expected = "MaxBalanceReached(\"Cash\")")]
    fn fail_add_entry() {
        let mut flow = bounded();
        flow.add_entry(
            2044,
            "Income".to_string(),
            "Weekly".to_string(),
            Utc.timestamp_opt(0, 0).unwrap(),
        )
        .unwrap();
    }

    #[test]
    #[should_panic(expected = "MaxBalanceReached(\"Cash\")")]
    fn fail_update_entry() {
        let mut flow = bounded();
        flow.add_entry(
            123,
            "Income".to_string(),
            "Weekly".to_string(),
            Utc.timestamp_opt(0, 0).unwrap(),
        )
        .unwrap();
        let entry_id = flow.entries[0].id.clone();

        flow.update_entry(
            &entry_id,
            2000,
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
            -123,
            "Income".to_string(),
            "Weekly".to_string(),
            Utc.timestamp_opt(0, 0).unwrap(),
        )
        .unwrap();
        let entry_id = flow.entries[0].id.clone();

        flow.update_entry(
            &entry_id,
            2000,
            String::from("Income"),
            String::from("Monthly"),
        )
        .unwrap();
    }
}
