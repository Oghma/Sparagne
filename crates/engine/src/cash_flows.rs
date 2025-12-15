//! The module contains the representation of a cash flow.
use chrono::{DateTime, Utc};

use sea_orm::entity::{ActiveValue, prelude::*};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{ResultEngine, entry::Entry, error::EngineError};
use crate::Currency;

/// How a cash flow enforces upper bounds.
///
/// Amounts are expressed in integer minor units (e.g. cents for EUR).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowMode {
    /// No upper bound (but still enforces non-negativity, except for
    /// Unallocated).
    Unlimited,
    /// The flow balance must not exceed `cap_minor`.
    NetCapped { cap_minor: i64 },
    /// The cumulative sum of all incomes must not exceed `cap_minor`.
    ///
    /// `income_total_minor` tracks the cumulative positive amounts and is
    /// independent from expenses.
    IncomeCapped {
        cap_minor: i64,
        income_total_minor: i64,
    },
}

fn income_contribution_minor(amount_minor: i64) -> i64 {
    amount_minor.max(0)
}

/// A cash flow.
///
/// A cash flow is a “bucket” that tracks how much money is allocated to a goal
/// (vacations, emergency fund, …).
///
/// A cash flow can be:
/// - `Unlimited`: no upper bound.
/// - `NetCapped`: balance must not exceed `cap_minor`.
/// - `IncomeCapped`: cumulative incomes must not exceed `cap_minor`.
///
/// **Non-negativity**
/// For normal flows, `balance` must never go below 0. The only exception is the
/// special “Unallocated” flow, identified by internal name `unallocated`, which
/// is allowed to go negative.
///
/// If a non-Unallocated flow is already negative due to legacy data, the engine
/// enters a recovery mode: only operations that increase the balance are
/// allowed until it reaches `>= 0`.
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
    pub fn mode(&self) -> FlowMode {
        match (self.max_balance, self.income_balance) {
            (None, _) => FlowMode::Unlimited,
            (Some(cap_minor), None) => FlowMode::NetCapped { cap_minor },
            (Some(cap_minor), Some(income_total_minor)) => FlowMode::IncomeCapped {
                cap_minor,
                income_total_minor,
            },
        }
    }

    pub fn is_unallocated(&self) -> bool {
        self.name.eq_ignore_ascii_case("unallocated")
    }

    pub fn new(
        name: String,
        balance: i64,
        max_balance: Option<i64>,
        income_bounded: Option<bool>,
        currency: Currency,
    ) -> ResultEngine<Self> {
        if balance < 0 && !name.eq_ignore_ascii_case("unallocated") {
            return Err(EngineError::InvalidFlow(
                "flow balance must be >= 0 (except Unallocated)".to_string(),
            ));
        }
        if let Some(cap_minor) = max_balance
            && cap_minor <= 0
        {
            return Err(EngineError::InvalidFlow("cap must be > 0".to_string()));
        }

        let income_balance = match income_bounded {
            Some(true) => {
                if max_balance.is_none() {
                    return Err(EngineError::InvalidFlow(
                        "income-capped flow requires a cap".to_string(),
                    ));
                }
                Some(0)
            }
            _ => None,
        };

        Ok(Self {
            id: Uuid::new_v4(),
            name,
            balance,
            max_balance,
            income_balance,
            currency,
            entries: Vec::new(),
            archived: false,
        })
    }

    pub fn with_id(
        id: Uuid,
        name: String,
        balance: i64,
        max_balance: Option<i64>,
        income_bounded: Option<bool>,
        currency: Currency,
    ) -> ResultEngine<Self> {
        if balance < 0 && !name.eq_ignore_ascii_case("unallocated") {
            return Err(EngineError::InvalidFlow(
                "flow balance must be >= 0 (except Unallocated)".to_string(),
            ));
        }
        if let Some(cap_minor) = max_balance
            && cap_minor <= 0
        {
            return Err(EngineError::InvalidFlow("cap must be > 0".to_string()));
        }

        let income_balance = match income_bounded {
            Some(true) => {
                if max_balance.is_none() {
                    return Err(EngineError::InvalidFlow(
                        "income-capped flow requires a cap".to_string(),
                    ));
                }
                Some(0)
            }
            _ => None,
        };

        Ok(Self {
            id,
            name,
            balance,
            max_balance,
            income_balance,
            currency,
            entries: Vec::new(),
            archived: false,
        })
    }

    pub fn add_entry(
        &mut self,
        amount_minor: i64,
        category: String,
        note: String,
        date: DateTime<Utc>,
    ) -> ResultEngine<&Entry> {
        let entry = Entry::new(amount_minor, self.currency, category, note, date);

        let new_balance = self.balance + entry.amount_minor;
        if !self.is_unallocated() {
            if self.balance >= 0 {
                if new_balance < 0 {
                    return Err(EngineError::InsufficientFunds(self.name.clone()));
                }
            } else if new_balance < self.balance {
                // Legacy recovery mode: allow only operations that move the balance towards >=
                // 0.
                return Err(EngineError::InsufficientFunds(self.name.clone()));
            }
        }

        match self.mode() {
            FlowMode::Unlimited => {}
            FlowMode::NetCapped { cap_minor } => {
                if new_balance > cap_minor {
                    return Err(EngineError::MaxBalanceReached(self.name.clone()));
                }
            }
            FlowMode::IncomeCapped {
                cap_minor,
                income_total_minor,
            } => {
                let new_income_total =
                    income_total_minor + income_contribution_minor(entry.amount_minor);
                if new_income_total > cap_minor {
                    return Err(EngineError::MaxBalanceReached(self.name.clone()));
                }
                self.income_balance = Some(new_income_total);
            }
        }

        self.balance = new_balance;
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
                let new_balance = self.balance - entry.amount_minor;
                if !self.is_unallocated() {
                    if self.balance >= 0 {
                        if new_balance < 0 {
                            return Err(EngineError::InsufficientFunds(self.name.clone()));
                        }
                    } else if new_balance < self.balance {
                        return Err(EngineError::InsufficientFunds(self.name.clone()));
                    }
                }
                self.balance = new_balance;

                if let FlowMode::IncomeCapped { .. } = self.mode()
                    && let Some(income_total_minor) = self.income_balance
                {
                    self.income_balance = Some(
                        income_total_minor - income_contribution_minor(entry.amount_minor),
                    );
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
                let old_amount_minor = self.entries[index].amount_minor;
                let mode = self.mode();
                let is_unallocated = self.is_unallocated();

                let new_balance = self.balance - old_amount_minor + amount_minor;

                if !is_unallocated {
                    if self.balance >= 0 {
                        if new_balance < 0 {
                            return Err(EngineError::InsufficientFunds(self.name.clone()));
                        }
                    } else if new_balance < self.balance {
                        return Err(EngineError::InsufficientFunds(self.name.clone()));
                    }
                }

                match mode {
                    FlowMode::Unlimited => {}
                    FlowMode::NetCapped { cap_minor } => {
                        if new_balance > cap_minor {
                            return Err(EngineError::MaxBalanceReached(self.name.clone()));
                        }
                    }
                    FlowMode::IncomeCapped {
                        cap_minor,
                        income_total_minor,
                    } => {
                        let new_income_total = income_total_minor
                            - income_contribution_minor(old_amount_minor)
                            + income_contribution_minor(amount_minor);
                        if new_income_total > cap_minor {
                            return Err(EngineError::MaxBalanceReached(self.name.clone()));
                        }
                        self.income_balance = Some(new_income_total);
                    }
                }

                self.balance = new_balance;

                let entry = &mut self.entries[index];
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

    fn net_capped() -> CashFlow {
        CashFlow::new(String::from("Cash"), 0, Some(1000), None, Currency::Eur).unwrap()
    }

    fn income_capped() -> CashFlow {
        CashFlow::new(
            String::from("Cash"),
            0,
            Some(1000),
            Some(true),
            Currency::Eur,
        )
        .unwrap()
    }

    fn unbounded() -> CashFlow {
        CashFlow::new(String::from("Cash"), 0, None, None, Currency::Eur).unwrap()
    }

    fn unallocated() -> CashFlow {
        CashFlow::new("unallocated".to_string(), 0, None, None, Currency::Eur).unwrap()
    }

    fn legacy_negative_flow() -> CashFlow {
        CashFlow {
            id: Uuid::new_v4(),
            name: "Cash".to_string(),
            balance: -10,
            max_balance: None,
            income_balance: None,
            currency: Currency::Eur,
            entries: Vec::new(),
            archived: false,
        }
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
    fn fail_net_capped_add_income_over_cap() {
        let mut flow = net_capped();
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
    fn fail_net_capped_update_over_cap() {
        let mut flow = net_capped();
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
    #[should_panic(expected = "InsufficientFunds(\"Cash\")")]
    fn fail_non_unallocated_negative_balance() {
        let mut flow = unbounded();
        flow.add_entry(
            -1,
            "Expense".to_string(),
            "Too much".to_string(),
            Utc.timestamp_opt(0, 0).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn allow_unallocated_negative_balance() {
        let mut flow = unallocated();
        flow.add_entry(
            -1,
            "Expense".to_string(),
            "Allowed".to_string(),
            Utc.timestamp_opt(0, 0).unwrap(),
        )
        .unwrap();
        assert_eq!(flow.balance, -1);
    }

    #[test]
    fn legacy_negative_flow_allows_recovery_incomes_only() {
        let mut flow = legacy_negative_flow();
        flow.add_entry(
            5,
            "Income".to_string(),
            "Recover".to_string(),
            Utc.timestamp_opt(0, 0).unwrap(),
        )
        .unwrap();
        assert_eq!(flow.balance, -5);

        flow.add_entry(
            -1,
            "Expense".to_string(),
            "Should fail".to_string(),
            Utc.timestamp_opt(0, 0).unwrap(),
        )
        .unwrap_err();
    }

    #[test]
    fn income_capped_tracks_income_total_on_update_and_delete() {
        let mut flow = income_capped();
        flow.add_entry(
            100,
            "Income".to_string(),
            "First".to_string(),
            Utc.timestamp_opt(0, 0).unwrap(),
        )
        .unwrap();
        flow.add_entry(
            100,
            "Income".to_string(),
            "Second".to_string(),
            Utc.timestamp_opt(0, 0).unwrap(),
        )
        .unwrap();
        assert_eq!(flow.income_balance, Some(200));

        let entry_id = flow.entries[0].id.clone();
        flow.update_entry(&entry_id, -50, "Expense".to_string(), "Switch".to_string())
            .unwrap();
        assert_eq!(flow.income_balance, Some(100));

        flow.delete_entry(&entry_id).unwrap();
        assert_eq!(flow.income_balance, Some(100));
    }
}
