//! The `Vault` holds the user's wallets and cash flows. The user can have
//! multiple vaults.

use sea_orm::{prelude::*, ActiveValue};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    cash_flows, cash_flows::CashFlow, entry, error::EngineError, wallets::Wallet, ResultEngine,
};

/// Holds wallets and cash flows
#[derive(Debug)]
pub struct Vault {
    pub id: Uuid,
    pub name: String,
    pub cash_flow: HashMap<String, CashFlow>,
    pub wallet: HashMap<Uuid, Wallet>,
    pub user_id: String,
}

impl Vault {
    pub fn new(name: String, user_id: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            cash_flow: HashMap::new(),
            wallet: HashMap::new(),
            user_id: user_id.to_string(),
        }
    }

    /// Add an income or expense entry
    pub fn add_entry(
        &mut self,
        wallet_id: Option<&Uuid>,
        flow_id: Option<&str>,
        amount: f64,
        category: String,
        note: String,
    ) -> ResultEngine<(Uuid, entry::ActiveModel)> {
        let entry;

        match (wallet_id, flow_id) {
            (Some(wid), Some(fid)) => {
                let Some(flow) = self.cash_flow.get_mut(fid) else {
                    return Err(EngineError::KeyNotFound(fid.to_string()));
                };
                entry = flow.add_entry(amount, category, note)?;

                let Some(wallet) = self.wallet.get_mut(wid) else {
                    return Err(EngineError::KeyNotFound(wid.to_string()));
                };
                wallet.insert_entry(entry);
            }
            (Some(wid), None) => {
                let Some(wallet) = self.wallet.get_mut(wid) else {
                    return Err(EngineError::KeyNotFound(wid.to_string()));
                };
                entry = wallet.add_entry(amount, category, note)?;
            }
            (None, Some(fid)) => {
                let Some(flow) = self.cash_flow.get_mut(fid) else {
                    return Err(EngineError::KeyNotFound(fid.to_string()));
                };
                entry = flow.add_entry(amount, category, note)?;
            }
            (None, None) => {
                return Err(EngineError::KeyNotFound(
                    "Missing wallet and cash flow ids".to_string(),
                ));
            }
        }

        let entry_id = entry.id;
        let entry: entry::ActiveModel = entry.into();
        Ok((entry_id, entry))
    }

    pub fn delete_entry(
        &mut self,
        wallet_id: Option<&Uuid>,
        flow_id: Option<&str>,
        entry_id: &Uuid,
    ) -> ResultEngine<()> {
        match (wallet_id, flow_id) {
            (Some(wid), Some(fid)) => {
                let Some(flow) = self.cash_flow.get_mut(fid) else {
                    return Err(EngineError::KeyNotFound(fid.to_string()));
                };
                flow.delete_entry(entry_id)?;

                let Some(wallet) = self.wallet.get_mut(wid) else {
                    return Err(EngineError::KeyNotFound(wid.to_string()));
                };
                wallet.delete_entry(entry_id)?;
            }
            (Some(wid), None) => {
                let Some(wallet) = self.wallet.get_mut(wid) else {
                    return Err(EngineError::KeyNotFound(wid.to_string()));
                };
                wallet.delete_entry(entry_id)?;
            }

            (None, Some(fid)) => {
                let Some(flow) = self.cash_flow.get_mut(fid) else {
                    return Err(EngineError::KeyNotFound(fid.to_string()));
                };
                flow.delete_entry(entry_id)?;
            }

            (None, None) => {
                return Err(EngineError::KeyNotFound(
                    "Missing wallet and cash flow ids".to_string(),
                ))
            }
        };

        Ok(())
    }

    pub fn new_flow(
        &mut self,
        name: String,
        balance: f64,
        max_balance: Option<f64>,
        income_bounded: Option<bool>,
    ) -> ResultEngine<(String, cash_flows::ActiveModel)> {
        if self.cash_flow.contains_key(&name) {
            return Err(EngineError::ExistingKey(name));
        }
        let flow = CashFlow::new(name.clone(), balance, max_balance, income_bounded);
        let flow_id = flow.name.clone();
        let flow_mdodel: cash_flows::ActiveModel = (&flow).into();
        self.cash_flow.insert(name, flow);

        Ok((flow_id, flow_mdodel))
    }

    pub fn iter_flow(&self) -> impl Iterator<Item = (&String, &CashFlow)> {
        self.cash_flow.iter().filter(|flow| !flow.1.archived)
    }

    pub fn iter_all_flow(&self) -> impl Iterator<Item = (&String, &CashFlow)> {
        self.cash_flow.iter()
    }

    pub fn update_entry(
        &mut self,
        wallet_id: Option<&Uuid>,
        flow_id: Option<&str>,
        entry_id: &Uuid,
        amount: f64,
        category: String,
        note: String,
    ) -> ResultEngine<entry::ActiveModel> {
        let entry;

        match (wallet_id, flow_id) {
            (Some(wid), Some(fid)) => {
                let Some(flow) = self.cash_flow.get_mut(fid) else {
                    return Err(EngineError::KeyNotFound(fid.to_string()));
                };
                entry = flow.update_entry(entry_id, amount, category.clone(), note.clone())?;

                let Some(wallet) = self.wallet.get_mut(wid) else {
                    return Err(EngineError::KeyNotFound(wid.to_string()));
                };
                wallet.update_entry(entry_id, amount, category, note)?;
            }
            (Some(wid), None) => {
                let Some(wallet) = self.wallet.get_mut(wid) else {
                    return Err(EngineError::KeyNotFound(wid.to_string()));
                };
                entry = wallet.update_entry(entry_id, amount, category, note)?;
            }
            (None, Some(fid)) => {
                let Some(flow) = self.cash_flow.get_mut(fid) else {
                    return Err(EngineError::KeyNotFound(fid.to_string()));
                };
                entry = flow.update_entry(entry_id, amount, category, note)?;
            }
            (None, None) => {
                return Err(EngineError::KeyNotFound(
                    "Missing wallet and cash flow ids".to_string(),
                ));
            }
        }

        let entry: entry::ActiveModel = entry.into();
        Ok(entry)
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
    pub id: Uuid,
    pub name: String,
    pub user_id: String,
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

impl From<&Vault> for ActiveModel {
    fn from(value: &Vault) -> Self {
        Self {
            id: sea_orm::ActiveValue::Set(value.id),
            name: ActiveValue::Set(value.name.clone()),
            user_id: ActiveValue::Set(value.user_id.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> (String, Vault) {
        let mut vault = Vault::new(String::from("Main"), "foo");
        vault
            .new_flow(String::from("Cash"), 1f64, None, None)
            .unwrap();
        (String::from("Cash"), vault)
    }

    #[test]
    fn add_entry() {
        let (flow_name, mut vault) = vault();
        vault
            .add_entry(
                None,
                Some(&flow_name),
                1.2,
                String::from("Income"),
                String::from(""),
            )
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "KeyNotFound(\"Foo\")")]
    fn fail_flow_entry() {
        let (_, mut vault) = vault();
        vault
            .add_entry(
                None,
                Some("Foo"),
                1.2,
                String::from("Income"),
                String::from(""),
            )
            .unwrap();
    }

    #[test]
    fn new_flows() {
        let mut vault = Vault::new(String::from("Main"), "foo");

        vault
            .new_flow(String::from("Cash"), 1f64, None, None)
            .unwrap();

        vault
            .new_flow(String::from("Cash1"), 1f64, Some(10f64), None)
            .unwrap();

        vault
            .new_flow(String::from("Cash2"), 1f64, Some(10f64), Some(true))
            .unwrap();

        assert!(!vault.cash_flow.is_empty());
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
            .add_entry(
                None,
                Some(&flow_name),
                1.2,
                String::from("Income"),
                String::from(""),
            )
            .unwrap();

        vault
            .delete_entry(None, Some(&flow_name), &entry_id)
            .unwrap();
    }

    #[test]
    fn update_entry() {
        let (flow_name, mut vault) = vault();

        let (entry_id, _) = vault
            .add_entry(
                None,
                Some(&flow_name),
                1.2,
                String::from("Income"),
                String::from(""),
            )
            .unwrap();

        vault
            .update_entry(
                None,
                Some(&flow_name),
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
        assert!(vault.cash_flow.is_empty());
    }
}
