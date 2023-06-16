//! The `Vault` holds the user's wallets and cash flows. The user can have
//! multiple vaults.

use sea_orm::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;

use crate::{cash_flows, cash_flows::CashFlow, entry, error::EngineError, ResultEngine};

/// Holds wallets and cash flows
#[derive(Debug)]
pub struct Vault {
    pub id: Uuid,
    cash_flow: HashMap<String, CashFlow>,
}

impl Vault {
    pub fn new(flows: Vec<CashFlow>) -> Self {
        let cash_flow = flows
            .into_iter()
            .map(|entry| (entry.name.clone(), entry))
            .collect::<HashMap<_, _>>();

        Self {
            id: Uuid::new_v4(),
            cash_flow,
        }
    }

    pub fn add_flow_entry(
        &mut self,
        flow_name: &String,
        amount: f64,
        category: String,
        note: String,
    ) -> ResultEngine<(Uuid, entry::ActiveModel)> {
        match self.cash_flow.get_mut(flow_name) {
            Some(flow) => {
                let entry = flow.add_entry(amount, category, note)?;
                let entry_id = entry.id.clone();
                let entry: entry::ActiveModel = entry.into();

                Ok((entry_id, entry))
            }
            None => Err(EngineError::KeyNotFound(flow_name.clone())),
        }
    }

    pub fn delete_flow_entry(&mut self, flow_name: &String, entry_id: &Uuid) -> ResultEngine<()> {
        match self.cash_flow.get_mut(flow_name) {
            Some(flow) => {
                flow.delete_entry(entry_id)?;
                Ok(())
            }
            None => Err(EngineError::KeyNotFound(flow_name.clone())),
        }
    }

    pub fn new_flow(
        &mut self,
        name: String,
        balance: f64,
        max_balance: Option<f64>,
        income_bounded: Option<bool>,
    ) -> ResultEngine<cash_flows::ActiveModel> {
        if self.cash_flow.contains_key(&name) {
            return Err(EngineError::ExistingKey(name));
        }
        let flow = CashFlow::new(name.clone(), balance, max_balance, income_bounded);
        let flow_mdodel: cash_flows::ActiveModel = (&flow).into();
        self.cash_flow.insert(name, flow);

        Ok(flow_mdodel)
    }

    pub fn iter_flow(&self) -> impl Iterator<Item = (&String, &CashFlow)> {
        self.cash_flow.iter().filter(|flow| !flow.1.archived)
    }

    pub fn iter_all_flow(&self) -> impl Iterator<Item = (&String, &CashFlow)> {
        self.cash_flow.iter()
    }

    pub fn update_flow_entry(
        &mut self,
        flow_name: &String,
        entry_id: &Uuid,
        amount: f64,
        category: String,
        note: String,
    ) -> ResultEngine<entry::ActiveModel> {
        match self.cash_flow.get_mut(flow_name) {
            Some(flow) => {
                let entry = flow.update_entry(entry_id, amount, category, note)?;
                let entry: entry::ActiveModel = entry.into();

                Ok(entry)
            }
            None => Err(EngineError::KeyNotFound(flow_name.clone())),
        }
    }

    pub fn delete_flow(
        &mut self,
        name: &String,
        archive: bool,
    ) -> ResultEngine<cash_flows::ActiveModel> {
        match (self.cash_flow.get_mut(name), archive) {
            (Some(flow), true) => {
                flow.archive();
                Ok(flow.into())
            }
            (Some(flow), false) => {
                let flow: cash_flows::ActiveModel = flow.into();
                self.cash_flow.remove(name);
                Ok(flow)
            }
            (None, _) => Err(EngineError::KeyNotFound(name.clone())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "vaults")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: Option<Uuid>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::cash_flows::Entity")]
    CashFlows,
}

impl Related<super::cash_flows::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CashFlows.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> (String, Vault) {
        let vault = Vault::new(vec![CashFlow::new(String::from("Cash"), 1f64, None, None)]);
        (String::from("Cash"), vault)
    }

    #[test]
    fn add_flow_entry() {
        let (flow_name, mut vault) = vault();
        vault
            .add_flow_entry(&flow_name, 1.2, String::from("Income"), String::from(""))
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "KeyNotFound(\"Foo\")")]
    fn fail_flow_entry() {
        let (_, mut vault) = vault();
        vault
            .add_flow_entry(
                &String::from("Foo"),
                1.2,
                String::from("Income"),
                String::from(""),
            )
            .unwrap();
    }

    #[test]
    fn new_flows() {
        let mut vault = Vault::new(vec![]);

        vault
            .new_flow(String::from("Cash"), 1f64, None, None)
            .unwrap();

        vault
            .new_flow(String::from("Cash1"), 1f64, Some(10f64), None)
            .unwrap();

        vault
            .new_flow(String::from("Cash2"), 1f64, Some(10f64), Some(true))
            .unwrap();

        assert_eq!(vault.cash_flow.is_empty(), false);
    }

    #[test]
    #[should_panic(expected = "ExistingKey(\"Cash\")")]
    fn fail_add_same_flow() {
        let (flow_name, mut vault) = vault();
        vault.new_flow(flow_name, 1f64, Some(10f64), None).unwrap();
    }

    #[test]
    fn delete_entry() {
        let (flow_name, mut vault) = vault();

        let (entry_id, _) = vault
            .add_flow_entry(&flow_name, 1.2, String::from("Income"), String::from(""))
            .unwrap();

        vault.delete_flow_entry(&flow_name, &entry_id).unwrap();
    }

    #[test]
    fn update_entry() {
        let (flow_name, mut vault) = vault();

        let (entry_id, _) = vault
            .add_flow_entry(&flow_name, 1.2, String::from("Income"), String::from(""))
            .unwrap();

        vault
            .update_flow_entry(
                &flow_name,
                &entry_id,
                -5f64,
                String::from("Home"),
                String::from(""),
            )
            .unwrap();
    }

    #[test]
    fn delete_flow() {
        let (flow_name, mut vault) = vault();
        vault.delete_flow(&flow_name, false).unwrap();
        assert_eq!(vault.cash_flow.is_empty(), true);
    }
}
