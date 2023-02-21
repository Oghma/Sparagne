//! This module is the core of the application. The `Engine` struct handles cash
//! flows and wallets.
use std::collections::HashMap;

use self::entry::Entry;
use self::sqlite3::SQLite3;
use self::{cash_flows::CashFlow, errors::EngineError};

mod cash_flows;
mod entry;
pub mod errors;
mod sqlite3;

/// Handle wallets and cash flow.
#[derive(Debug)]
pub struct Engine {
    chash_flows: HashMap<String, CashFlow>,
    database: SQLite3,
}

impl Engine {
    pub fn new(database: SQLite3) -> Self {
        let mut cash_flow = HashMap::new();

        let cash_flows = database.select::<CashFlow, ()>(None, None, None);
        for flow in cash_flows {
            cash_flow.insert(flow.name.clone(), flow);
        }

        let entries = database.select::<Entry, ()>(None, None, None);
        for entry in entries {
            let flow = cash_flow.get_mut(&entry.cash_flow).unwrap();
            flow.entries.push(entry);
        }

        Self {
            chash_flows: cash_flow,
            database,
        }
    }

    pub fn builder() -> EngineBuilder {
        EngineBuilder::default()
    }

    pub fn add_flow_entry(
        &mut self,
        flow_name: &String,
        amount: f64,
        category: String,
        note: String,
    ) -> Result<String, errors::EngineError> {
        match self.chash_flows.get_mut(flow_name) {
            Some(flow) => flow.add_entry(amount, category, note).and_then(|entry| {
                self.database.insert(entry);
                Ok(entry.id.clone())
            }),
            None => Err(EngineError::KeyNotFound(flow_name.clone())),
        }
    }

    pub fn delete_flow_entry(
        &mut self,
        flow_name: &String,
        entry_id: &String,
    ) -> Result<(), errors::EngineError> {
        match self.chash_flows.get_mut(flow_name) {
            Some(flow) => flow.delete_entry(entry_id).and_then(|entry| {
                self.database
                    .delete(&entry)
                    .update::<CashFlow>(flow, "name", &flow.name);
                Ok(())
            }),
            None => Err(errors::EngineError::KeyNotFound(flow_name.clone())),
        }
    }

    pub fn new_flow(
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
        self.database.insert(&flow);
        self.chash_flows.insert(name, flow);

        Ok(())
    }

    pub fn iter_flow(&self) -> impl Iterator<Item = (&String, &CashFlow)> {
        self.chash_flows.iter().filter(|flow| !flow.1.archived)
    }

    pub fn iter_all_flow(&self) -> impl Iterator<Item = (&String, &CashFlow)> {
        self.chash_flows.iter()
    }

    pub fn update_flow_entry(
        &mut self,
        flow_name: &String,
        entry_id: &String,
        amount: f64,
        category: String,
        note: String,
    ) -> Result<(), errors::EngineError> {
        if let Some(flow) = self.chash_flows.get_mut(flow_name) {
            if let Ok(entry) = flow.update_entry(entry_id, amount, category, note) {
                self.database
                    .update::<Entry>(entry, "id", &entry.id)
                    .update::<CashFlow>(flow, "name", flow_name);
                return Ok(());
            }
        }
        Err(errors::EngineError::KeyNotFound(flow_name.clone()))
    }

    pub fn delete_flow(&mut self, name: &String, archive: bool) -> Result<(), errors::EngineError> {
        if let Some(flow) = self.chash_flows.get_mut(name) {
            if archive {
                flow.archive();
                self.database.update::<CashFlow>(flow, "name", name);
            } else {
                for entry in &flow.entries {
                    self.database.delete(entry);
                }
                self.database.delete(flow);
                self.chash_flows.remove(name);
            }
            return Ok(());
        }
        Err(EngineError::KeyNotFound(name.clone()))
    }
}

#[derive(Default)]
pub struct EngineBuilder {
    sqlite3_path: Option<String>,
    sqlite3_memory: Option<bool>,
}

impl EngineBuilder {
    pub fn database(mut self, path: &String) -> EngineBuilder {
        self.sqlite3_path = Some(path.clone());
        self
    }

    pub fn memory(mut self) -> EngineBuilder {
        self.sqlite3_memory = Some(true);
        self
    }

    pub fn build(self) -> Engine {
        let database = SQLite3::new(
            self.sqlite3_path.as_ref().map(|s| &**s),
            self.sqlite3_memory,
        );
        Engine::new(database)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> (String, Engine) {
        let mut engine = Engine::builder().memory().build();
        engine
            .new_flow(String::from("Cash"), 1f64, None, None)
            .unwrap();

        (String::from("Cash"), engine)
    }

    #[test]
    fn add_flow_entry() {
        let (flow_name, mut engine) = engine();

        engine
            .add_flow_entry(&flow_name, 1.2, String::from("Income"), String::from(""))
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "KeyNotFound(\"Foo\")")]
    fn fail_flow_entry() {
        let (_, mut engine) = engine();
        engine
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
        let mut engine = Engine::builder().memory().build();
        engine
            .new_flow(String::from("Cash"), 1f64, None, None)
            .unwrap();

        engine
            .new_flow(String::from("Cash1"), 1f64, Some(10f64), None)
            .unwrap();

        engine
            .new_flow(String::from("Cash2"), 1f64, Some(10f64), Some(true))
            .unwrap();

        assert_eq!(engine.chash_flows.is_empty(), false);
    }

    #[test]
    #[should_panic(expected = "ExistingKey(\"Cash\")")]
    fn fail_add_same_flow() {
        let (flow_name, mut engine) = engine();
        engine.new_flow(flow_name, 1f64, Some(10f64), None).unwrap();
    }

    #[test]
    fn delete_entry() {
        let (flow_name, mut engine) = engine();

        let entry_id = engine
            .add_flow_entry(&flow_name, 1.2, String::from("Income"), String::from(""))
            .unwrap();

        engine.delete_flow_entry(&flow_name, &entry_id).unwrap();
    }

    #[test]
    fn update_entry() {
        let (flow_name, mut engine) = engine();

        let entry_id = engine
            .add_flow_entry(&flow_name, 1.2, String::from("Income"), String::from(""))
            .unwrap();

        engine
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
        let (flow_name, mut engine) = engine();
        engine.delete_flow(&flow_name, false).unwrap();
        assert_eq!(engine.chash_flows.is_empty(), true);
    }
}
