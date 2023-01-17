//! This module is the core of the application. The `Engine` struct handles cash
//! flows and wallets.
use self::{cash_flows::CashFlow, errors::EngineError};
use std::collections::hash_map::Iter;
use std::collections::HashMap;

mod cash_flows;
mod entry;
pub mod errors;
mod sqlite3;

/// Handle wallets and cash flow.
pub struct Engine {
    chash_flows: HashMap<String, CashFlow>,
    database: SQLite3,
}

impl Engine {
    pub fn new(database: SQLite3) -> Self {
        Self {
            chash_flows: HashMap::new(),
            database,
        }
    }

    pub fn add_flow_entry(
        &mut self,
        flow_name: &String,
        amount: f64,
        category: String,
        note: String,
    ) -> Result<uuid::Uuid, errors::EngineError> {
        match self.chash_flows.get_mut(flow_name) {
            Some(flow) => flow.add_entry(amount, category, note),
            None => Err(EngineError::KeyNotFound(flow_name.clone())),
        }
    }

    pub fn delete_flow_entry(
        &mut self,
        flow_name: &String,
        entry_id: &uuid::Uuid,
    ) -> Result<(), errors::EngineError> {
        match self.chash_flows.get_mut(flow_name) {
            Some(flow) => flow.delete_entry(entry_id),
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
        self.chash_flows.insert(
            name.clone(),
            CashFlow::new(name, balance, max_balance, income_bounded),
        );

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
        entry_id: &uuid::Uuid,
        amount: f64,
        category: String,
        note: String,
    ) -> Result<(), errors::EngineError> {
        match self.chash_flows.get_mut(flow_name) {
            Some(flow) => flow.update_entry(entry_id, amount, category, note),
            None => Err(errors::EngineError::KeyNotFound(flow_name.clone())),
        }
    }

    pub fn delete_flow(&mut self, name: &String, archive: bool) -> Result<(), errors::EngineError> {
        match self.chash_flows.get_mut(name) {
            Some(flow) if archive => {
                flow.archive();
                Ok(())
            }
            Some(_) => {
                self.chash_flows.remove(name);
                Ok(())
            }
            None => Err(EngineError::KeyNotFound(name.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> (String, Engine) {
        let mut engine = Engine::new();
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
        let mut engine = Engine::new();
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
        let (_, mut engine) = engine();

        engine
            .new_flow(String::from("Cash"), 1f64, Some(10f64), None)
            .unwrap();
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
