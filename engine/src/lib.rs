use std::collections::HashMap;

pub use cash_flows::CashFlow;
pub use error::EngineError;
use sea_orm::{prelude::*, ActiveValue};
pub use vault::Vault;

mod cash_flows;
mod entry;
mod error;
mod vault;
mod wallets;

type ResultEngine<T> = Result<T, EngineError>;

#[derive(Debug)]
pub struct Engine {
    vaults: HashMap<String, Vault>,
    database: DatabaseConnection,
}

impl Engine {
    /// Return a builder for `Engine`. Help to build the struct.
    pub fn builder() -> EngineBuilder {
        EngineBuilder::default()
    }

    /// Add a new income or an expense
    #[allow(clippy::too_many_arguments)]
    pub async fn add_entry(
        &mut self,
        balance: f64,
        category: &str,
        note: &str,
        vault_id: &str,
        flow_id: Option<&str>,
        wallet_id: Option<&str>,
        user_id: &str,
    ) -> ResultEngine<String> {
        match self.vaults.get_mut(vault_id) {
            Some(vault) => {
                if vault.user_id != user_id {
                    return Err(EngineError::KeyNotFound("vault not exists".to_string()));
                }
                let (entry_id, mut entry_model) = vault.add_entry(
                    wallet_id,
                    flow_id,
                    balance,
                    category.to_string(),
                    note.to_string(),
                )?;
                if let Some(fid) = flow_id {
                    let flow_id = fid.to_string();
                    entry_model.cash_flow_id = ActiveValue::Set(Some(flow_id.clone()));

                    // Update cashflow balance
                    let flow = self.cash_flow(&flow_id, vault_id, user_id)?;
                    let mut flow_entry = cash_flows::ActiveModel::new();
                    flow_entry.name = ActiveValue::Set(flow.name.clone());
                    flow_entry.balance = ActiveValue::Set(flow.balance);

                    flow_entry.update(&self.database).await.unwrap();
                }
                if let Some(wid) = wallet_id {
                    entry_model.wallet_id = ActiveValue::Set(Some(wid.to_string()));
                }

                entry_model.insert(&self.database).await.unwrap();
                Ok(entry_id)
            }
            None => Err(EngineError::KeyNotFound(vault_id.to_string())),
        }
    }

    /// Return a [`CashFlow`]
    pub fn cash_flow(
        &self,
        cash_flow_id: &str,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<&CashFlow> {
        let vault = self.vault(Some(vault_id), None, user_id)?;

        vault
            .cash_flow
            .get(cash_flow_id)
            .ok_or(EngineError::KeyNotFound("cash_flow not exists".to_string()))
    }

    /// Delete a cash flow contained by a vault.
    pub async fn delete_cash_flow(
        &mut self,
        vault_id: &str,
        name: &str,
        archive: bool,
    ) -> ResultEngine<()> {
        match self.vaults.get_mut(vault_id) {
            Some(vault) => {
                let mut flow_model = vault.delete_flow(&name.to_string(), archive)?;
                flow_model.vault_id = ActiveValue::Set(vault.id.clone());

                if archive {
                    flow_model.archived = ActiveValue::Set(true);
                    flow_model.save(&self.database).await.unwrap();
                } else {
                    flow_model.delete(&self.database).await.unwrap();
                }
                Ok(())
            }
            None => Err(EngineError::KeyNotFound(vault_id.to_string())),
        }
    }

    /// Delete an income or an expense.
    pub async fn delete_entry(
        &mut self,
        vault_id: &str,
        flow_id: Option<&str>,
        wallet_id: Option<&str>,
        entry_id: &str,
    ) -> ResultEngine<()> {
        match self.vaults.get_mut(vault_id) {
            Some(vault) => {
                vault.delete_entry(wallet_id, flow_id, entry_id)?;
                entry::Entity::delete_by_id(entry_id)
                    .exec(&self.database)
                    .await
                    .unwrap();

                Ok(())
            }
            None => Err(EngineError::KeyNotFound(vault_id.to_string())),
        }
    }

    /// Delete or archive a vault
    /// TODO: Add `archive`
    pub async fn delete_vault(&mut self, vault_id: &str) -> ResultEngine<()> {
        match self.vaults.remove(vault_id) {
            Some(vault) => {
                let vault_model: vault::ActiveModel = (&vault).into();
                vault_model.delete(&self.database).await.unwrap();
                Ok(())
            }
            None => Err(EngineError::KeyNotFound(vault_id.to_string())),
        }
    }

    /// Add a new vault
    pub async fn new_vault(&mut self, name: &str, user_id: &str) -> ResultEngine<String> {
        let new_vault = Vault::new(name.to_string(), user_id);
        let new_vault_id = new_vault.id.clone();
        let vault_entry: vault::ActiveModel = (&new_vault).into();

        vault_entry.insert(&self.database).await.unwrap();
        self.vaults.insert(new_vault_id.clone(), new_vault);
        Ok(new_vault_id)
    }

    /// Add a new cash flow inside a vault.
    pub async fn new_cash_flow(
        &mut self,
        vault_id: &str,
        name: &str,
        balance: f64,
        max_balance: Option<f64>,
        income_bounded: Option<bool>,
    ) -> ResultEngine<String> {
        match self.vaults.get_mut(vault_id) {
            Some(vault) => {
                let (id, mut flow) =
                    vault.new_flow(name.to_string(), balance, max_balance, income_bounded)?;
                flow.vault_id = ActiveValue::Set(vault.id.clone());
                flow.insert(&self.database).await.unwrap();
                Ok(id)
            }
            None => Err(EngineError::KeyNotFound(vault_id.to_string())),
        }
    }

    /// Update an income or an expense
    #[allow(clippy::too_many_arguments)]
    pub async fn update_entry(
        &mut self,
        vault_id: &str,
        flow_id: Option<&str>,
        wallet_id: Option<&str>,
        entry_id: &str,
        amount: f64,
        category: &str,
        note: &str,
    ) -> ResultEngine<()> {
        match self.vaults.get_mut(vault_id) {
            Some(vault) => {
                let mut entry_model = vault.update_entry(
                    wallet_id,
                    flow_id,
                    entry_id,
                    amount,
                    category.to_string(),
                    note.to_string(),
                )?;

                if let Some(fid) = flow_id {
                    entry_model.cash_flow_id = ActiveValue::Set(Some(fid.to_string()));
                }
                if let Some(wid) = wallet_id {
                    entry_model.wallet_id = ActiveValue::Set(Some(wid.to_string()));
                }
                entry_model.save(&self.database).await.unwrap();

                Ok(())
            }
            None => Err(EngineError::KeyNotFound(vault_id.to_string())),
        }
    }

    /// Return a user `Vault`.
    pub fn vault(
        &self,
        vault_id: Option<&str>,
        vault_name: Option<String>,
        user_id: &str,
    ) -> ResultEngine<&Vault> {
        if vault_id.is_none() && vault_name.is_none() {
            return Err(EngineError::KeyNotFound(
                "missing vault id or name".to_string(),
            ));
        }

        let vault = if let Some(id) = vault_id {
            match self.vaults.get(id) {
                None => return Err(EngineError::KeyNotFound("vault not exists".to_string())),
                Some(vault) => {
                    if vault.user_id == user_id {
                        vault
                    } else {
                        return Err(EngineError::KeyNotFound("vault not exists".to_string()));
                    }
                }
            }
        } else {
            let name = vault_name.unwrap();

            match self
                .vaults
                .iter()
                .find(|(_, vault)| vault.name == name && vault.user_id == user_id)
            {
                Some((_, vault)) => vault,
                None => return Err(EngineError::KeyNotFound("vault not exists".to_string())),
            }
        };

        Ok(vault)
    }
}

/// The builder for `Engine`
#[derive(Default)]
pub struct EngineBuilder {
    database: DatabaseConnection,
}

impl EngineBuilder {
    /// Pass the required database
    pub fn database(mut self, db: DatabaseConnection) -> EngineBuilder {
        self.database = db;
        self
    }

    /// Construct `Engine`
    pub async fn build(self) -> Engine {
        let mut vaults = HashMap::new();

        let vaults_flows: Vec<(vault::Model, Vec<cash_flows::Model>)> = vault::Entity::find()
            .find_with_related(cash_flows::Entity)
            .all(&self.database)
            .await
            .unwrap();

        for vault_entry in vaults_flows {
            let mut flows = HashMap::new();

            for flow_entry in vault_entry.1 {
                // Fetch cash flow entries
                let entries: Vec<entry::Entry> = flow_entry
                    .find_related(entry::Entity)
                    .all(&self.database)
                    .await
                    .unwrap()
                    .into_iter()
                    .map(entry::Entry::from)
                    .collect();

                flows.insert(
                    flow_entry.name.clone(),
                    CashFlow {
                        name: flow_entry.name,
                        balance: flow_entry.balance,
                        max_balance: flow_entry.max_balance,
                        income_balance: flow_entry.income_balance,
                        entries,
                        archived: flow_entry.archived,
                    },
                );
            }

            vaults.insert(
                vault_entry.0.id.clone(),
                Vault {
                    id: vault_entry.0.id,
                    name: vault_entry.0.name,
                    cash_flow: flows,
                    wallet: HashMap::new(),
                    user_id: vault_entry.0.user_id,
                },
            );
        }

        Engine {
            vaults,
            database: self.database,
        }
    }
}
