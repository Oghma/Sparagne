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
    /// Return a builder for `Engine`. Help to build the struct.
    pub fn builder() -> EngineBuilder {
        EngineBuilder::default()
    }

    ///Add a new income or an expense
    async fn add_entry(
        &mut self,
        balance: f64,
        category: &str,
        note: &str,
        vault_id: &Uuid,
        flow_id: Option<&str>,
        wallet_id: Option<&Uuid>,
    ) -> ResultEngine<Uuid> {
        match self.vaults.get_mut(vault_id) {
            Some(vault) => {
                let (entry_id, mut entry_model) = vault.add_entry(
                    wallet_id,
                    flow_id,
                    balance,
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
        flow_id: Option<&str>,
        wallet_id: Option<&Uuid>,
        entry_id: &Uuid,
    ) -> ResultEngine<()> {
        match self.vaults.get_mut(vault_id) {
            Some(vault) => {
                vault.delete_entry(wallet_id, flow_id, entry_id)?;
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
        flow_id: Option<&str>,
        wallet_id: Option<&Uuid>,
        entry_id: &Uuid,
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
}

/// The builder for `Engine`
#[derive(Default)]
pub struct EngineBuilder {
    sqlite_path: String,
    initialize: bool,
}

impl EngineBuilder {
    /// Specifies a path to SQLite3 database
    ///
    /// NOTE: Only SQLite is supported
    pub fn sqlite(mut self, path: &str) -> EngineBuilder {
        self.sqlite_path = format!("sqlite:{}", path);
        self
    }

    /// Specifies to create an in-memory database
    pub fn memory(mut self) -> EngineBuilder {
        self.sqlite_path = String::from("sqlite::memory:");
        self
    }

    /// Specifies to initialize the database creating the schema
    ///
    /// By default, is `false`
    pub fn initialize(mut self) -> EngineBuilder {
        self.initialize = true;
        self
    }

    /// Construct `Engine`
    pub async fn build(self) -> Engine {
        let mut vaults = HashMap::new();
        let database = Database::connect(self.sqlite_path)
            .await
            .expect("Failed to connect to the database");

        if self.initialize {
            Migrator::up(&database, None).await.unwrap();
        }

        let vaults_flows: Vec<(vault::Model, Vec<cash_flows::Model>)> = vault::Entity::find()
            .find_with_related(cash_flows::Entity)
            .all(&database)
            .await
            .unwrap();

        for vault_entry in vaults_flows {
            let mut flows = HashMap::new();

            for flow_entry in vault_entry.1 {
                // Fetch cash flow entries
                let entries: Vec<entry::Entry> = flow_entry
                    .find_related(entry::Entity)
                    .all(&database)
                    .await
                    .unwrap()
                    .into_iter()
                    .map(|value| entry::Entry::from(value))
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
                vault_entry.0.id,
                Vault {
                    id: vault_entry.0.id,
                    name: vault_entry.0.name,
                    cash_flow: flows,
                    wallet: HashMap::new(),
                },
            );
        }

        Engine { vaults, database }
    }
}
