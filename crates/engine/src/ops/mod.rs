use sea_orm::DatabaseConnection;

use crate::{EngineError, ResultEngine};

mod access;
mod balances;
mod flows;
mod memberships;
mod transactions;
mod vaults;
mod wallets;

pub use transactions::TransactionListFilter;

/// Run a block inside a DB transaction, committing on success and rolling back on error.
macro_rules! with_tx {
    ($self:expr, |$tx:ident| $body:expr) => {{
        let $tx = $self.database.begin().await?;
        let result = $body;
        match result {
            Ok(value) => {
                $tx.commit().await?;
                Ok(value)
            }
            Err(err) => Err(err),
        }
    }};
}

pub(crate) use with_tx;

#[derive(Debug)]
pub struct Engine {
    database: DatabaseConnection,
}

impl Engine {
    /// Return a builder for `Engine`. Help to build the struct.
    pub fn builder() -> EngineBuilder {
        EngineBuilder::default()
    }

}

fn normalize_required_name(value: &str, label: &str) -> ResultEngine<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EngineError::InvalidAmount(format!(
            "{label} name must not be empty"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalize_required_flow_name(value: &str) -> ResultEngine<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EngineError::InvalidFlow(
            "flow name must not be empty".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
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
    pub async fn build(self) -> ResultEngine<Engine> {
        Ok(Engine {
            database: self.database,
        })
    }
}
