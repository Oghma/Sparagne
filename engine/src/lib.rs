use std::collections::HashMap;

pub use cash_flows::CashFlow;
pub use error::EngineError;
use migration::{Migrator, MigratorTrait};
use sea_orm::{prelude::*, ActiveValue, Database};
use uuid::Uuid;
pub use vault::Vault;

mod cash_flows;
mod entry;
mod error;
mod vault;
mod wallets;

type ResultEngine<T> = Result<T, EngineError>;

#[derive(Debug)]
pub struct Engine {
    vaults: HashMap<Uuid, Vault>,
    database: DatabaseConnection,
}

impl Engine {
    ///Add a new income or an expense
    async fn add_entry(
        &mut self,
        balance: f64,
        category: &str,
        note: &str,
        vault_id: &Uuid,
        flow_id: &String,
    ) -> ResultEngine<Uuid> {
        match self.vaults.get_mut(vault_id) {
            Some(vault) => {
                let (entry_id, mut entry_model) = vault.add_flow_entry(
                    flow_id,
                    balance,
                    category.to_string(),
                    note.to_string(),
                )?;

                entry_model.cash_flow_id = ActiveValue::Set(Some(flow_id.clone()));
                entry_model.save(&self.database).await.unwrap();
                Ok(entry_id)
            }
            None => Err(EngineError::KeyNotFound(vault_id.to_string())),
        }
    }

    /// Delete a cash flow contained by a vault.
    async fn delete_cash_flow(
        &mut self,
        vault_id: &Uuid,
        name: &str,
        archive: bool,
    ) -> ResultEngine<()> {
        match self.vaults.get_mut(vault_id) {
            Some(vault) => {
                let mut flow_model = vault.delete_flow(&name.to_string(), archive)?;
                flow_model.vault_id = ActiveValue::Set(vault.id);

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
    async fn delete_entry(
        &mut self,
        vault_id: &Uuid,
        flow_id: &String,
        entry_id: &Uuid,
    ) -> ResultEngine<()> {
        match self.vaults.get_mut(vault_id) {
            Some(vault) => {
                vault.delete_flow_entry(flow_id, entry_id)?;
                entry::Entity::delete_by_id(entry_id.clone())
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
    async fn delete_vault(&mut self, vault_id: &Uuid) -> ResultEngine<()> {
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
    async fn new_vault(&mut self, name: &str) -> ResultEngine<Uuid> {
        let new_vault = Vault::new(name.to_string());
        let new_vault_id = new_vault.id.clone();
        let vault_entry: vault::ActiveModel = (&new_vault).into();

        vault_entry.save(&self.database).await.unwrap();
        self.vaults.insert(new_vault.id, new_vault);
        Ok(new_vault_id)
    }

    /// Add a new cash flow inside a vault.
    async fn new_cash_flow(
        &mut self,
        vault_id: &Uuid,
        name: &str,
        balance: f64,
        max_balance: Option<f64>,
        income_bounded: Option<bool>,
    ) -> ResultEngine<String> {
        match self.vaults.get_mut(vault_id) {
            Some(vault) => {
                let (id, mut flow) =
                    vault.new_flow(name.to_string(), balance, max_balance, income_bounded)?;
                flow.vault_id = ActiveValue::Set(vault.id);
                flow.save(&self.database).await.unwrap();
                Ok(id)
            }
            None => Err(EngineError::KeyNotFound(vault_id.to_string())),
        }
    }

    /// Update an income or an expense
    async fn update_entry(
        &mut self,
        vault_id: &Uuid,
        flow_id: &String,
        entry_id: &Uuid,
        amount: f64,
        category: &str,
        note: &str,
    ) -> ResultEngine<()> {
        match self.vaults.get_mut(vault_id) {
            Some(vault) => {
                let mut entry_model = vault.update_flow_entry(
                    flow_id,
                    entry_id,
                    amount,
                    category.to_string(),
                    note.to_string(),
                )?;
                entry_model.cash_flow_id = ActiveValue::Set(Some(flow_id.clone()));
                entry_model.save(&self.database).await.unwrap();

                Ok(())
            }
            None => Err(EngineError::KeyNotFound(vault_id.to_string())),
        }
    }
}
