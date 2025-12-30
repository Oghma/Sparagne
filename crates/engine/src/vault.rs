//! The `Vault` holds the user's wallets and cash flows. The user can have
//! multiple vaults.

use sea_orm::{ActiveValue, prelude::*};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{CashFlow, Currency, Wallet};

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

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "vaults")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
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

impl From<&Vault> for ActiveModel {
    fn from(value: &Vault) -> Self {
        Self {
            id: sea_orm::ActiveValue::Set(
                Uuid::parse_str(&value.id).expect("Vault.id must be a valid UUID"),
            ),
            name: ActiveValue::Set(value.name.clone()),
            user_id: ActiveValue::Set(value.user_id.clone()),
            currency: ActiveValue::Set(value.currency.code().to_string()),
        }
    }
}
