//! This module is the core of the application. The `Engine` struct handles cash
//! flows and wallets.
use self::cash_flows::CashFlow;
use std::collections::hash_map::Iter;
use std::collections::HashMap;

mod cash_flows;
mod entry;
pub mod errors;

/// Handle wallets and cash flow.
pub struct Engine {
    chash_flows: HashMap<String, Box<dyn CashFlow>>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            chash_flows: HashMap::new(),
        }
    }

    pub fn add_flow_entry(
        &mut self,
        cash_flow: String,
        amount: f64,
        category: String,
        note: String,
        let flow = self.chash_flows.get_mut(&cash_flow).unwrap();
    ) -> Result<uuid::Uuid, errors::EngineError> {
        flow.add_entry(amount, category, note)
    }

    pub fn new_flow(
        &mut self,
        name: String,
        balance: f64,
        max_balance: Option<f64>,
        hard_bounded: Option<bool>,
    ) {
        let flow: Box<dyn CashFlow> = match max_balance {
            Some(mxb) => match hard_bounded {
                Some(true) => Box::new(cash_flows::HardBounded::new(name.clone(), balance, mxb)),
                _ => Box::new(cash_flows::Bounded::new(name.clone(), balance, mxb)),
            },
            _ => Box::new(cash_flows::UnBounded::new(name.clone(), balance)),
        };

        self.chash_flows.insert(name, flow);
    }

    pub fn flow_iter(&self) -> Iter<String, Box<dyn CashFlow>> {
        self.chash_flows.iter()
    }
}
