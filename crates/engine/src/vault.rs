//! The `Vault` holds the user's wallets and cash flows. The user can have
//! multiple vaults.

use sea_orm::{ActiveValue, prelude::*};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{CashFlow, Currency, EngineError, Wallet};

/// Lightweight vault metadata used by APIs that don't need a full snapshot.
#[derive(Debug, Clone)]
pub struct VaultHeader {
    pub id: String,
    pub name: String,
    pub currency: Currency,
    /// Owner username for disambiguation in clients.
    pub owner: String,
}

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
}

impl From<Model> for VaultHeader {
    fn from(model: Model) -> Self {
        Self {
            id: model.id.to_string(),
            name: model.name,
            currency: model.currency,
            owner: model.user_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "vaults")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    pub user_id: String,
    pub currency: Currency,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::cash_flows::Entity")]
    CashFlows,
    #[sea_orm(has_many = "super::wallets::Entity")]
    Wallets,
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

impl ActiveModelBehavior for ActiveModel {}

impl TryFrom<&Vault> for ActiveModel {
    type Error = EngineError;

    fn try_from(value: &Vault) -> Result<Self, Self::Error> {
        let id = Uuid::parse_str(&value.id)
            .map_err(|_| EngineError::InvalidId("invalid vault id".to_string()))?;
        Ok(Self {
            id: sea_orm::ActiveValue::Set(id),
            name: ActiveValue::Set(value.name.clone()),
            user_id: ActiveValue::Set(value.user_id.clone()),
            currency: ActiveValue::Set(value.currency),
        })
    }
}
