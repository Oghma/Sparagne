//! This module is the core of the application. The `Engine` struct handles cash
//! flows and wallets.
use migration::{Migrator, MigratorTrait};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, QueryFilter,
};
use std::collections::HashMap;

use self::{cash_flows::CashFlow, errors::EngineError};

mod cash_flows;
mod entry;
pub mod errors;

/// Handle wallets and cash flow.
#[derive(Debug)]
pub struct Engine {
    chash_flows: HashMap<String, CashFlow>,
    database: DatabaseConnection,
}

impl Engine {
    pub async fn new(database: DatabaseConnection) -> Self {
        let mut cash_flow = HashMap::new();

        let cash_flows: Vec<cash_flows::Model> =
            cash_flows::Entity::find().all(&database).await.unwrap();
        let entries: Vec<entry::Model> = entry::Entity::find().all(&database).await.unwrap();

        for flow in cash_flows {
            cash_flow.insert(flow.name.clone(), flow.into());
        }

        for entry in entries {
            let flow: &mut CashFlow = cash_flow.get_mut(&entry.cash_flow_id).unwrap();
            flow.entries.push(entry.into());
        }

        Self {
            chash_flows: cash_flow,
            database,
        }
    }

    pub fn builder() -> EngineBuilder {
        EngineBuilder::default()
    }

    pub async fn add_flow_entry(
        &mut self,
        flow_name: &String,
        amount: f64,
        category: String,
        note: String,
    ) -> Result<String, errors::EngineError> {
        match self.chash_flows.get_mut(flow_name) {
            Some(flow) => {
                let entry = flow.add_entry(amount, category, note)?;
                let entry_insert: entry::ActiveModel = entry.into();
                println!("{:?}", &entry_insert);
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
    ) -> Result<(), errors::EngineError> {
        match self.chash_flows.get_mut(flow_name) {
            Some(flow) => {
                flow.delete_entry(entry_id)?;
                entry::Entity::delete_by_id(entry_id)
                    .exec(&self.database)
                    .await
                    .unwrap();
                Ok(())
            }
            None => Err(errors::EngineError::KeyNotFound(flow_name.clone())),
        }
    }

    pub async fn new_flow(
        &mut self,
        name: String,
        balance: f64,
        max_balance: Option<f64>,
        income_bounded: Option<bool>,
    ) -> Result<(), errors::EngineError> {
        if self.chash_flows.contains_key(&name) {
            return Err(errors::EngineError::ExistingKey(name));
        }
        let flow = CashFlow::new(name.clone(), balance, max_balance, income_bounded);
        let flow_mdodel: cash_flows::ActiveModel = (&flow).into();
        println!("{:?}", &flow_mdodel);
        flow_mdodel.insert(&self.database).await.unwrap();

        let cash_flows: Vec<cash_flows::Model> = cash_flows::Entity::find()
            .all(&self.database)
            .await
            .unwrap();

        println!("{:?}", cash_flows);

        self.chash_flows.insert(name, flow);

        Ok(())
    }

    pub fn iter_flow(&self) -> impl Iterator<Item = (&String, &CashFlow)> {
        self.chash_flows.iter().filter(|flow| !flow.1.archived)
    }

    pub fn iter_all_flow(&self) -> impl Iterator<Item = (&String, &CashFlow)> {
        self.chash_flows.iter()
    }

    pub async fn update_flow_entry(
        &mut self,
        flow_name: &String,
        entry_id: &String,
        amount: f64,
        category: String,
        note: String,
    ) -> Result<(), errors::EngineError> {
        match self.chash_flows.get_mut(flow_name) {
            Some(flow) => {
                let entry = flow.update_entry(entry_id, amount, category, note)?;
                let entry_model: entry::ActiveModel = entry.into();
                entry_model.update(&self.database).await.unwrap();
                Ok(())
            }
            None => Err(errors::EngineError::KeyNotFound(flow_name.clone())),
        }
    }

    pub async fn delete_flow(
        &mut self,
        name: &String,
        archive: bool,
    ) -> Result<(), errors::EngineError> {
        if let Some(flow) = self.chash_flows.get_mut(name) {
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
                self.chash_flows.remove(name);
            }
            return Ok(());
        }
        Err(EngineError::KeyNotFound(name.clone()))
    }
}

#[derive(Default)]
pub struct EngineBuilder {
    url: String,
    database_initialize: bool,
}

impl EngineBuilder {
    pub fn database(mut self, path: &str) -> EngineBuilder {
        self.url = format!("sqlite:{}", path);
        self
    }

    pub fn memory(mut self) -> EngineBuilder {
        self.url = "sqlite::memory:".to_string();
        self.database_initialize = true;
        self
    }

    pub fn database_initialize(mut self) -> EngineBuilder {
        self.database_initialize = true;
        self
    }

    pub async fn build(self) -> Engine {
        let database = Database::connect(self.url)
            .await
            .expect("Failed to create db");

        if self.database_initialize {
            Migrator::up(&database, None).await.unwrap();
        }

        Engine::new(database).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn engine() -> (String, Engine) {
        let mut engine = Engine::builder().memory().build().await;
        engine
            .new_flow(String::from("Cash"), 1f64, None, None)
            .await
            .unwrap();

        (String::from("Cash"), engine)
    }

    #[tokio::test]
    async fn add_flow_entry() {
        let (flow_name, mut engine) = engine().await;

        engine
            .add_flow_entry(&flow_name, 1.2, String::from("Income"), String::from(""))
            .await
            .unwrap();
    }

    #[tokio::test]
    #[should_panic(expected = "KeyNotFound(\"Foo\")")]
    async fn fail_flow_entry() {
        let (_, mut engine) = engine().await;
        engine
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
        let mut engine = Engine::builder().memory().build().await;
        engine
            .new_flow(String::from("Cash"), 1f64, None, None)
            .await
            .unwrap();

        engine
            .new_flow(String::from("Cash1"), 1f64, Some(10f64), None)
            .await
            .unwrap();

        engine
            .new_flow(String::from("Cash2"), 1f64, Some(10f64), Some(true))
            .await
            .unwrap();

        assert_eq!(engine.chash_flows.is_empty(), false);
    }

    #[tokio::test]
    #[should_panic(expected = "ExistingKey(\"Cash\")")]
    async fn fail_add_same_flow() {
        let (flow_name, mut engine) = engine().await;
        engine
            .new_flow(flow_name, 1f64, Some(10f64), None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_entry() {
        let (flow_name, mut engine) = engine().await;

        let entry_id = engine
            .add_flow_entry(&flow_name, 1.2, String::from("Income"), String::from(""))
            .await
            .unwrap();

        engine
            .delete_flow_entry(&flow_name, &entry_id)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn update_entry() {
        let (flow_name, mut engine) = engine().await;

        let entry_id = engine
            .add_flow_entry(&flow_name, 1.2, String::from("Income"), String::from(""))
            .await
            .unwrap();

        engine
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
        let (flow_name, mut engine) = engine().await;
        engine.delete_flow(&flow_name, false).await.unwrap();
        assert_eq!(engine.chash_flows.is_empty(), true);
    }
}
