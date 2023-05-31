//! The `Vault` holds the user's wallets and cash flows. The user can have
//! multiple vaults.

use migration::{Migrator, MigratorTrait};
use sea_orm::{prelude::*, Database};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{cash_flows, cash_flows::CashFlow, entry, error::EngineError, ResultEngine};

/// Holds wallets and cash flows
#[derive(Debug)]
pub struct Vault {
    pub id: Uuid,
    cash_flows: HashMap<String, CashFlow>,
    database: DatabaseConnection,
}

impl Vault {
    pub async fn new(database: DatabaseConnection) -> Self {
        let mut cash_flow = HashMap::new();

        let cash_flows: Vec<cash_flows::Model> =
            cash_flows::Entity::find().all(&database).await.unwrap();
        let entries: Vec<entry::Model> = entry::Entity::find().all(&database).await.unwrap();

        for flow in cash_flows {
            cash_flow.insert(flow.name.clone(), flow.into());
        }

        for entry in entries {
            if let Some(id) = &entry.cash_flow_id {
                let flow: &mut CashFlow = cash_flow.get_mut(id).unwrap();
                flow.entries.push(entry.into());
            }
        }

        Self {
            id: Uuid::new_v4(),
            cash_flows: cash_flow,
            database,
        }
    }

    pub fn builder() -> VaultBuilder {
        VaultBuilder::default()
    }

    pub async fn add_flow_entry(
        &mut self,
        flow_name: &String,
        amount: f64,
        category: String,
        note: String,
    ) -> ResultEngine<String> {
        match self.cash_flows.get_mut(flow_name) {
            Some(flow) => {
                let entry = flow.add_entry(amount, category, note)?;
                let entry_insert: entry::ActiveModel = entry.into();
                entry_insert.insert(&self.database).await.unwrap();
                Ok(entry.id.clone())
            }
            None => Err(EngineError::KeyNotFound(flow_name.clone())),
        }
    }

    pub async fn delete_flow_entry(
        &mut self,
        flow_name: &String,
        entry_id: &String,
    ) -> ResultEngine<()> {
        match self.cash_flows.get_mut(flow_name) {
            Some(flow) => {
                flow.delete_entry(entry_id)?;
                entry::Entity::delete_by_id(entry_id)
                    .exec(&self.database)
                    .await
                    .unwrap();
                Ok(())
            }
            None => Err(EngineError::KeyNotFound(flow_name.clone())),
        }
    }

    pub async fn new_flow(
        &mut self,
        name: String,
        balance: f64,
        max_balance: Option<f64>,
        income_bounded: Option<bool>,
    ) -> ResultEngine<()> {
        if self.cash_flows.contains_key(&name) {
            return Err(EngineError::ExistingKey(name));
        }
        let flow = CashFlow::new(name.clone(), balance, max_balance, income_bounded);
        let flow_mdodel: cash_flows::ActiveModel = (&flow).into();
        flow_mdodel.insert(&self.database).await.unwrap();
        self.cash_flows.insert(name, flow);

        Ok(())
    }

    pub fn iter_flow(&self) -> impl Iterator<Item = (&String, &CashFlow)> {
        self.cash_flows.iter().filter(|flow| !flow.1.archived)
    }

    pub fn iter_all_flow(&self) -> impl Iterator<Item = (&String, &CashFlow)> {
        self.cash_flows.iter()
    }

    pub async fn update_flow_entry(
        &mut self,
        flow_name: &String,
        entry_id: &String,
        amount: f64,
        category: String,
        note: String,
    ) -> ResultEngine<()> {
        match self.cash_flows.get_mut(flow_name) {
            Some(flow) => {
                let entry = flow.update_entry(entry_id, amount, category, note)?;
                let entry_model: entry::ActiveModel = entry.into();
                entry_model.update(&self.database).await.unwrap();
                Ok(())
            }
            None => Err(EngineError::KeyNotFound(flow_name.clone())),
        }
    }

    pub async fn delete_flow(&mut self, name: &String, archive: bool) -> ResultEngine<()> {
        if let Some(flow) = self.cash_flows.get_mut(name) {
            if archive {
                flow.archive();
                let flow_model: cash_flows::ActiveModel = flow.into();
                flow_model.update(&self.database).await.unwrap();
            } else {
                // TODO: Handle better when wallets are implemented
                entry::Entity::delete_many()
                    .filter(entry::Column::CashFlowId.eq(name))
                    .exec(&self.database)
                    .await
                    .unwrap();

                let flow_mdodel: cash_flows::ActiveModel = flow.into();
                flow_mdodel.delete(&self.database).await.unwrap();
                self.cash_flows.remove(name);
            }
            return Ok(());
        }
        Err(EngineError::KeyNotFound(name.clone()))
    }
}

#[derive(Default)]
pub struct VaultBuilder {
    url: String,
    database_initialize: bool,
}

impl VaultBuilder {
    pub fn database(mut self, path: &str) -> VaultBuilder {
        self.url = format!("sqlite:{}", path);
        self
    }

    pub fn memory(mut self) -> VaultBuilder {
        self.url = "sqlite::memory:".to_string();
        self.database_initialize = true;
        self
    }

    pub fn database_initialize(mut self) -> VaultBuilder {
        self.database_initialize = true;
        self
    }

    pub async fn build(self) -> Vault {
        let database = Database::connect(self.url)
            .await
            .expect("Failed to create db");

        if self.database_initialize {
            Migrator::up(&database, None).await.unwrap();
        }

        Vault::new(database).await
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "vault")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: Option<Uuid>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::*;

    async fn vault() -> (String, Vault) {
        let mut vault = Vault::builder().memory().build().await;
        vault
            .new_flow(String::from("Cash"), 1f64, None, None)
            .await
            .unwrap();

        (String::from("Cash"), vault)
    }

    #[tokio::test]
    async fn add_flow_entry() {
        let (flow_name, mut vault) = vault().await;

        vault
            .add_flow_entry(&flow_name, 1.2, String::from("Income"), String::from(""))
            .await
            .unwrap();
    }

    #[tokio::test]
    #[should_panic(expected = "KeyNotFound(\"Foo\")")]
    async fn fail_flow_entry() {
        let (_, mut vault) = vault().await;
        vault
            .add_flow_entry(
                &String::from("Foo"),
                1.2,
                String::from("Income"),
                String::from(""),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn new_flows() {
        let mut vault = Vault::builder().memory().build().await;
        vault
            .new_flow(String::from("Cash"), 1f64, None, None)
            .await
            .unwrap();

        vault
            .new_flow(String::from("Cash1"), 1f64, Some(10f64), None)
            .await
            .unwrap();

        vault
            .new_flow(String::from("Cash2"), 1f64, Some(10f64), Some(true))
            .await
            .unwrap();

        assert_eq!(vault.cash_flows.is_empty(), false);
    }

    #[tokio::test]
    #[should_panic(expected = "ExistingKey(\"Cash\")")]
    async fn fail_add_same_flow() {
        let (flow_name, mut vault) = vault().await;
        vault
            .new_flow(flow_name, 1f64, Some(10f64), None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_entry() {
        let (flow_name, mut vault) = vault().await;

        let entry_id = vault
            .add_flow_entry(&flow_name, 1.2, String::from("Income"), String::from(""))
            .await
            .unwrap();

        vault
            .delete_flow_entry(&flow_name, &entry_id)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn update_entry() {
        let (flow_name, mut vault) = vault().await;

        let entry_id = vault
            .add_flow_entry(&flow_name, 1.2, String::from("Income"), String::from(""))
            .await
            .unwrap();

        vault
            .update_flow_entry(
                &flow_name,
                &entry_id,
                -5f64,
                String::from("Home"),
                String::from(""),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_flow() {
        let (flow_name, mut vault) = vault().await;
        vault.delete_flow(&flow_name, false).await.unwrap();
        assert_eq!(vault.cash_flows.is_empty(), true);
    }
}
