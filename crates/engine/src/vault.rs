//! The `Vault` holds the user's wallets and cash flows. The user can have
//! multiple vaults.

use sea_orm::{ActiveValue, prelude::*};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    Currency, ResultEngine, cash_flows, cash_flows::CashFlow, error::EngineError, wallets::Wallet,
};

/// Holds wallets and cash flows
#[derive(Debug)]
pub struct Vault {
    pub id: String,
    pub name: String,
    pub cash_flow: HashMap<Uuid, CashFlow>,
    pub wallet: HashMap<Uuid, Wallet>,
    pub user_id: String,
    pub currency: Currency,
}

impl Vault {
    pub fn new(name: String, user_id: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            cash_flow: HashMap::new(),
            wallet: HashMap::new(),
            user_id: user_id.to_string(),
            currency: Currency::Eur,
        }
    }

    pub fn unallocated_flow_id(&self) -> ResultEngine<Uuid> {
        self.cash_flow
            .iter()
            .find_map(|(id, flow)| flow.is_unallocated().then_some(*id))
            .ok_or_else(|| EngineError::InvalidFlow("missing Unallocated flow".to_string()))
    }

    pub fn new_flow(
        &mut self,
        name: String,
        balance: i64,
        max_balance: Option<i64>,
        income_bounded: Option<bool>,
    ) -> ResultEngine<(Uuid, cash_flows::ActiveModel)> {
        if name.eq_ignore_ascii_case(cash_flows::UNALLOCATED_INTERNAL_NAME) {
            return Err(EngineError::InvalidFlow(
                "flow name is reserved".to_string(),
            ));
        }
        if self.cash_flow.values().any(|flow| flow.name == name) {
            return Err(EngineError::ExistingKey(name));
        }
        let flow = CashFlow::new(
            name.clone(),
            balance,
            max_balance,
            income_bounded,
            self.currency,
        )?;
        let flow_id = flow.id;
        let flow_mdodel: cash_flows::ActiveModel = (&flow).into();
        self.cash_flow.insert(flow_id, flow);

        Ok((flow_id, flow_mdodel))
    }

    pub fn iter_flow(&self) -> impl Iterator<Item = (&Uuid, &CashFlow)> {
        self.cash_flow.iter().filter(|flow| !flow.1.archived)
    }

    pub fn iter_all_flow(&self) -> impl Iterator<Item = (&Uuid, &CashFlow)> {
        self.cash_flow.iter()
    }

    pub fn delete_flow(
        &mut self,
        flow_id: &Uuid,
        archive: bool,
    ) -> ResultEngine<cash_flows::ActiveModel> {
        match (self.cash_flow.get_mut(flow_id), archive) {
            (Some(flow), true) => {
                if flow.is_unallocated() {
                    return Err(EngineError::InvalidFlow(
                        "cannot archive Unallocated".to_string(),
                    ));
                }
                flow.archive();
                Ok(flow.into())
            }
            (Some(flow), false) => {
                if flow.is_unallocated() {
                    return Err(EngineError::InvalidFlow(
                        "cannot delete Unallocated".to_string(),
                    ));
                }
                let flow: cash_flows::ActiveModel = flow.into();
                self.cash_flow.remove(flow_id);
                Ok(flow)
            }
            (None, _) => Err(EngineError::KeyNotFound(flow_id.to_string())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "vaults")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub user_id: String,
    pub currency: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::cash_flows::Entity")]
    CashFlows,
    #[sea_orm(has_many = "super::wallets::Entity")]
    Wallets,
}

impl Related<super::cash_flows::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CashFlows.def()
    }
}

impl Related<super::wallets::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Wallets.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

impl From<&Vault> for ActiveModel {
    fn from(value: &Vault) -> Self {
        Self {
            id: sea_orm::ActiveValue::Set(value.id.clone()),
            name: ActiveValue::Set(value.name.clone()),
            user_id: ActiveValue::Set(value.user_id.clone()),
            currency: ActiveValue::Set(value.currency.code().to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> (Uuid, Vault) {
        let mut vault = Vault::new(String::from("Main"), "foo");
        let (flow_id, _) = vault
            .new_flow(String::from("Cash"), 100, None, None)
            .unwrap();
        (flow_id, vault)
    }

    #[test]
    fn new_flows() {
        let mut vault = Vault::new(String::from("Main"), "foo");

        vault
            .new_flow(String::from("Cash"), 100, None, None)
            .unwrap();

        vault
            .new_flow(String::from("Cash1"), 100, Some(1000), None)
            .unwrap();

        vault
            .new_flow(String::from("Cash2"), 100, Some(1000), Some(true))
            .unwrap();

        assert!(!vault.cash_flow.is_empty());
    }

    #[test]
    #[should_panic(expected = "InvalidFlow(\"flow name is reserved\")")]
    fn fail_add_reserved_unallocated_name() {
        let mut vault = Vault::new(String::from("Main"), "foo");
        vault
            .new_flow("unallocated".to_string(), 0, None, None)
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "InvalidFlow(\"cannot delete Unallocated\")")]
    fn fail_delete_unallocated() {
        let mut vault = Vault::new(String::from("Main"), "foo");
        let mut flow =
            CashFlow::new("unallocated".to_string(), 0, None, None, vault.currency).unwrap();
        flow.system_kind = Some(cash_flows::SystemFlowKind::Unallocated);
        let id = flow.id;
        vault.cash_flow.insert(id, flow);

        vault.delete_flow(&id, false).unwrap();
    }

    #[test]
    #[should_panic(expected = "ExistingKey(\"Cash\")")]
    fn fail_add_same_flow() {
        let (_, mut vault) = vault();
        vault
            .new_flow("Cash".to_string(), 100, Some(1000), None)
            .unwrap();
    }

    #[test]
    fn delete_flow() {
        let (flow_name, mut vault) = vault();
        vault.delete_flow(&flow_name, false).unwrap();
        assert!(vault.cash_flow.is_empty());
    }
}
