//! Cash flows.

use sea_orm::entity::{ActiveValue, prelude::*};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    Currency, EngineError, ResultEngine,
    util::{ensure_vault_currency, validate_flow_mode_fields},
};

pub(crate) const UNALLOCATED_INTERNAL_NAME: &str = "unallocated";

/// Identifies special system-managed flows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "Text")]
pub enum SystemFlowKind {
    #[sea_orm(string_value = "unallocated")]
    Unallocated,
}

impl SystemFlowKind {
    /// Returns the string representation used in the database.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unallocated => UNALLOCATED_INTERNAL_NAME,
        }
    }
}

impl TryFrom<&str> for SystemFlowKind {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value == UNALLOCATED_INTERNAL_NAME {
            Ok(Self::Unallocated)
        } else {
            Err(())
        }
    }
}

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
    #[serde(skip)]
    pub system_kind: Option<SystemFlowKind>,
    pub balance: i64,
    pub max_balance: Option<i64>,
    pub income_balance: Option<i64>,
    pub currency: Currency,
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
        matches!(self.system_kind, Some(SystemFlowKind::Unallocated))
            || self.name.eq_ignore_ascii_case(UNALLOCATED_INTERNAL_NAME)
    }

    pub fn new(
        name: String,
        balance: i64,
        max_balance: Option<i64>,
        income_bounded: Option<bool>,
        currency: Currency,
    ) -> ResultEngine<Self> {
        if balance < 0 && !name.eq_ignore_ascii_case(UNALLOCATED_INTERNAL_NAME) {
            return Err(EngineError::InvalidFlow(
                "flow balance must be >= 0 (except Unallocated)".to_string(),
            ));
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
        validate_flow_mode_fields(&name, max_balance, income_balance)?;

        Ok(Self {
            id: Uuid::new_v4(),
            name,
            system_kind: None,
            balance,
            max_balance,
            income_balance,
            currency,
            archived: false,
        })
    }

    pub fn apply_leg_change(
        &mut self,
        old_amount_minor: i64,
        new_amount_minor: i64,
    ) -> ResultEngine<()> {
        let mode = self.mode();
        let is_unallocated = self.is_unallocated();
        let new_balance = self.balance - old_amount_minor + new_amount_minor;

        if !is_unallocated && new_balance < 0 {
            return Err(EngineError::InsufficientFunds(self.name.clone()));
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
                    + income_contribution_minor(new_amount_minor);
                if new_income_total > cap_minor {
                    return Err(EngineError::MaxBalanceReached(self.name.clone()));
                }
                self.income_balance = Some(new_income_total);
            }
        }

        self.balance = new_balance;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "cash_flows")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    pub system_kind: Option<SystemFlowKind>,
    pub balance: i64,
    pub max_balance: Option<i64>,
    pub income_balance: Option<i64>,
    pub currency: Currency,
    pub archived: bool,
    pub vault_id: Uuid,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::vault::Entity",
        from = "Column::VaultId",
        to = "super::vault::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    Vaults,
}

impl Related<super::vault::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Vaults.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

/// Convert a storage model into a domain `CashFlow`, validating invariants.
impl TryFrom<(Model, Currency)> for CashFlow {
    type Error = EngineError;

    fn try_from((model, vault_currency): (Model, Currency)) -> ResultEngine<Self> {
        ensure_vault_currency(vault_currency, model.currency)?;
        validate_flow_mode_fields(&model.name, model.max_balance, model.income_balance)?;
        Ok(Self {
            id: model.id,
            name: model.name,
            system_kind: model.system_kind,
            balance: model.balance,
            max_balance: model.max_balance,
            income_balance: model.income_balance,
            currency: model.currency,
            archived: model.archived,
        })
    }
}

impl From<&CashFlow> for ActiveModel {
    fn from(flow: &CashFlow) -> Self {
        Self {
            id: ActiveValue::Set(flow.id),
            name: ActiveValue::Set(flow.name.clone()),
            system_kind: ActiveValue::Set(flow.system_kind),
            balance: ActiveValue::Set(flow.balance),
            max_balance: ActiveValue::Set(flow.max_balance),
            income_balance: ActiveValue::Set(flow.income_balance),
            currency: ActiveValue::Set(flow.currency),
            archived: ActiveValue::Set(flow.archived),
            vault_id: ActiveValue::NotSet,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

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
        CashFlow::new(
            UNALLOCATED_INTERNAL_NAME.to_string(),
            0,
            None,
            None,
            Currency::Eur,
        )
        .unwrap()
    }

    #[test]
    fn apply_leg_change() {
        let mut flow = unbounded();
        flow.apply_leg_change(0, 123).unwrap();
        assert_eq!(flow.balance, 123);
    }

    #[test]
    fn apply_leg_change_remove() {
        let mut flow = unbounded();
        flow.apply_leg_change(0, 123).unwrap();
        flow.apply_leg_change(123, 0).unwrap();
        assert_eq!(flow.balance, 0);
    }

    #[test]
    fn apply_leg_change_update() {
        let mut flow = unbounded();
        flow.apply_leg_change(0, 123).unwrap();
        flow.apply_leg_change(123, 1000).unwrap();
        assert_eq!(flow.balance, 1000);
    }

    #[test]
    fn fail_net_capped_add_income_over_cap() {
        let mut flow = net_capped();
        let err = flow.apply_leg_change(0, 2044).unwrap_err();
        assert_eq!(err, EngineError::MaxBalanceReached("Cash".to_string()));
    }

    #[test]
    fn fail_net_capped_update_over_cap() {
        let mut flow = net_capped();
        flow.apply_leg_change(0, 123).unwrap();
        let err = flow.apply_leg_change(123, 2000).unwrap_err();
        assert_eq!(err, EngineError::MaxBalanceReached("Cash".to_string()));
    }

    #[test]
    fn fail_non_unallocated_negative_balance() {
        let mut flow = unbounded();
        let err = flow.apply_leg_change(0, -1).unwrap_err();
        assert_eq!(err, EngineError::InsufficientFunds("Cash".to_string()));
    }

    #[test]
    fn allow_unallocated_negative_balance() {
        let mut flow = unallocated();
        flow.apply_leg_change(0, -1).unwrap();
        assert_eq!(flow.balance, -1);
    }

    #[test]
    fn income_capped_tracks_income_total_on_update_and_delete() {
        let mut flow = income_capped();
        flow.apply_leg_change(0, 100).unwrap();
        flow.apply_leg_change(0, 100).unwrap();
        assert_eq!(flow.income_balance, Some(200));

        flow.apply_leg_change(100, -50).unwrap();
        assert_eq!(flow.income_balance, Some(100));

        flow.apply_leg_change(-50, 0).unwrap();
        assert_eq!(flow.income_balance, Some(100));
    }
}
