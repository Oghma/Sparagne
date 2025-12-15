//! The `Vault` holds the user's wallets and cash flows. The user can have
//! multiple vaults.

use chrono::{DateTime, Utc};
use sea_orm::{ActiveValue, prelude::*};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    Currency, ResultEngine, cash_flows, cash_flows::CashFlow, entry, error::EngineError,
    wallets::Wallet,
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

    /// Add an income or expense entry
    pub fn add_entry(
        &mut self,
        wallet_id: Option<Uuid>,
        flow_id: Option<Uuid>,
        amount_minor: i64,
        category: String,
        note: String,
        date: DateTime<Utc>,
    ) -> ResultEngine<(String, entry::ActiveModel)> {
        let entry;

        match (wallet_id, flow_id) {
            (Some(wid), Some(fid)) => {
                let Some(flow) = self.cash_flow.get_mut(&fid) else {
                    return Err(EngineError::KeyNotFound(fid.to_string()));
                };
                if flow.currency != self.currency {
                    return Err(EngineError::CurrencyMismatch(
                        "flow currency mismatch".to_string(),
                    ));
                }
                entry = flow.add_entry(amount_minor, category, note, date)?;

                let Some(wallet) = self.wallet.get_mut(&wid) else {
                    return Err(EngineError::KeyNotFound(wid.to_string()));
                };
                if wallet.currency != self.currency {
                    return Err(EngineError::CurrencyMismatch(
                        "wallet currency mismatch".to_string(),
                    ));
                }
                wallet.insert_entry(entry);
            }
            (Some(wid), None) => {
                let Some(wallet) = self.wallet.get_mut(&wid) else {
                    return Err(EngineError::KeyNotFound(wid.to_string()));
                };
                if wallet.currency != self.currency {
                    return Err(EngineError::CurrencyMismatch(
                        "wallet currency mismatch".to_string(),
                    ));
                }
                entry = wallet.add_entry(amount_minor, category, note, date)?;
            }
            (None, Some(fid)) => {
                let Some(flow) = self.cash_flow.get_mut(&fid) else {
                    return Err(EngineError::KeyNotFound(fid.to_string()));
                };
                if flow.currency != self.currency {
                    return Err(EngineError::CurrencyMismatch(
                        "flow currency mismatch".to_string(),
                    ));
                }
                entry = flow.add_entry(amount_minor, category, note, date)?;
            }
            (None, None) => {
                return Err(EngineError::KeyNotFound(
                    "Missing wallet and cash flow ids".to_string(),
                ));
            }
        }

        let entry_id = entry.id.clone();
        let entry: entry::ActiveModel = entry.into();
        Ok((entry_id, entry))
    }

    /// Delete an income or expense
    pub fn delete_entry(
        &mut self,
        wallet_id: Option<Uuid>,
        flow_id: Option<Uuid>,
        entry_id: &str,
    ) -> ResultEngine<()> {
        if wallet_id.is_none() && flow_id.is_none() {
            return Err(EngineError::KeyNotFound(
                "Missing wallet and cash flow ids".to_string(),
            ));
        }

        if let Some(fid) = flow_id {
            let Some(flow) = self.cash_flow.get_mut(&fid) else {
                return Err(EngineError::KeyNotFound(fid.to_string()));
            };
            flow.delete_entry(entry_id)?;
        }

        if let Some(wid) = wallet_id {
            let Some(wallet) = self.wallet.get_mut(&wid) else {
                return Err(EngineError::KeyNotFound(wid.to_string()));
            };
            wallet.delete_entry(entry_id)?;
        }

        Ok(())
    }

    pub fn new_flow(
        &mut self,
        name: String,
        balance: i64,
        max_balance: Option<i64>,
        income_bounded: Option<bool>,
    ) -> ResultEngine<(Uuid, cash_flows::ActiveModel)> {
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

    pub fn update_entry(
        &mut self,
        wallet_id: Option<Uuid>,
        flow_id: Option<Uuid>,
        entry_id: &str,
        amount_minor: i64,
        category: String,
        note: String,
    ) -> ResultEngine<entry::ActiveModel> {
        let entry;

        match (wallet_id, flow_id) {
            (Some(wid), Some(fid)) => {
                let Some(flow) = self.cash_flow.get_mut(&fid) else {
                    return Err(EngineError::KeyNotFound(fid.to_string()));
                };
                if flow.currency != self.currency {
                    return Err(EngineError::CurrencyMismatch(
                        "flow currency mismatch".to_string(),
                    ));
                }
                entry =
                    flow.update_entry(entry_id, amount_minor, category.clone(), note.clone())?;

                let Some(wallet) = self.wallet.get_mut(&wid) else {
                    return Err(EngineError::KeyNotFound(wid.to_string()));
                };
                if wallet.currency != self.currency {
                    return Err(EngineError::CurrencyMismatch(
                        "wallet currency mismatch".to_string(),
                    ));
                }
                wallet.update_entry(entry_id, amount_minor, category, note)?;
            }
            (Some(wid), None) => {
                let Some(wallet) = self.wallet.get_mut(&wid) else {
                    return Err(EngineError::KeyNotFound(wid.to_string()));
                };
                if wallet.currency != self.currency {
                    return Err(EngineError::CurrencyMismatch(
                        "wallet currency mismatch".to_string(),
                    ));
                }
                entry = wallet.update_entry(entry_id, amount_minor, category, note)?;
            }
            (None, Some(fid)) => {
                let Some(flow) = self.cash_flow.get_mut(&fid) else {
                    return Err(EngineError::KeyNotFound(fid.to_string()));
                };
                if flow.currency != self.currency {
                    return Err(EngineError::CurrencyMismatch(
                        "flow currency mismatch".to_string(),
                    ));
                }
                entry = flow.update_entry(entry_id, amount_minor, category, note)?;
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
        flow_id: &Uuid,
        archive: bool,
    ) -> ResultEngine<cash_flows::ActiveModel> {
        match (self.cash_flow.get_mut(flow_id), archive) {
            (Some(flow), true) => {
                flow.archive();
                Ok(flow.into())
            }
            (Some(flow), false) => {
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
    #[sea_orm(has_many = "super::entry::Entity")]
    Entries,
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

impl Related<super::entry::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Entries.def()
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
    use chrono::{TimeZone, Utc};

    use super::*;

    fn vault() -> (Uuid, Vault) {
        let mut vault = Vault::new(String::from("Main"), "foo");
        let (flow_id, _) = vault
            .new_flow(String::from("Cash"), 100, None, None)
            .unwrap();
        (flow_id, vault)
    }

    #[test]
    fn add_entry() {
        let (flow_name, mut vault) = vault();
        vault
            .add_entry(
                None,
                Some(flow_name),
                120,
                String::from("Income"),
                String::from(""),
                Utc.timestamp_opt(0, 0).unwrap(),
            )
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "KeyNotFound")]
    fn fail_flow_entry() {
        let (_, mut vault) = vault();
        vault
            .add_entry(
                None,
                Some(Uuid::new_v4()),
                120,
                String::from("Income"),
                String::from(""),
                Utc.timestamp_opt(0, 0).unwrap(),
            )
            .unwrap();
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
    #[should_panic(expected = "ExistingKey(\"Cash\")")]
    fn fail_add_same_flow() {
        let (_, mut vault) = vault();
        vault
            .new_flow("Cash".to_string(), 100, Some(1000), None)
            .unwrap();
    }

    #[test]
    fn delete_entry() {
        let (flow_name, mut vault) = vault();

        let (entry_id, _) = vault
            .add_entry(
                None,
                Some(flow_name),
                120,
                String::from("Income"),
                String::from(""),
                Utc.timestamp_opt(0, 0).unwrap(),
            )
            .unwrap();

        vault
            .delete_entry(None, Some(flow_name), &entry_id)
            .unwrap();
    }

    #[test]
    fn update_entry() {
        let (flow_name, mut vault) = vault();

        let (entry_id, _) = vault
            .add_entry(
                None,
                Some(flow_name),
                120,
                String::from("Income"),
                String::from(""),
                Utc.timestamp_opt(0, 0).unwrap(),
            )
            .unwrap();

        vault
            .update_entry(
                None,
                Some(flow_name),
                &entry_id,
                -500,
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
