//! The module contains the `Entry` type representing an entry in cash flows and
//! wallets.
//!
//! Both expenses and income are represented by `Entry` type.
use core::fmt;

use chrono::{DateTime, Utc};
use sea_orm::{ActiveValue, entity::prelude::*};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Currency, Money};

/// Represent a movement, an entry in cash flows or wallets.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub amount_minor: i64,
    pub currency: Currency,
    pub category: String,
    pub note: String,
    pub date: DateTime<Utc>,
}

/// Type used to represent an entry in cash flows and wallets.
impl Entry {
    pub fn new(
        amount_minor: i64,
        currency: Currency,
        category: String,
        note: String,
        date: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            amount_minor,
            currency,
            category,
            note,
            date,
        }
    }
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {}",
            Money::new(self.amount_minor).format(self.currency),
            self.category,
            self.note
        )
    }
}

impl From<Model> for Entry {
    fn from(entry: Model) -> Self {
        Self {
            id: entry.id,
            amount_minor: entry.amount,
            currency: Currency::try_from(entry.currency.as_str()).unwrap_or_default(),
            category: entry.category.unwrap(),
            note: entry.note.unwrap(),
            date: entry.date,
        }
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "entries")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub amount: i64,
    pub currency: String,
    pub note: Option<String>,
    pub category: Option<String>,
    pub date: DateTimeUtc,
    pub vault_id: String,
    pub cash_flow_id: Option<String>,
    pub wallet_id: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::cash_flows::Entity",
        from = "Column::CashFlowId",
        to = "super::cash_flows::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    CashFlows,
    #[sea_orm(
        belongs_to = "super::wallets::Entity",
        from = "Column::WalletId",
        to = "super::wallets::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    Wallets,
    #[sea_orm(
        belongs_to = "super::vault::Entity",
        from = "Column::VaultId",
        to = "super::vault::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    Vaults,
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

impl Related<super::vault::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Vaults.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

impl From<&Entry> for ActiveModel {
    fn from(entry: &Entry) -> Self {
        Self {
            id: ActiveValue::Set(entry.id.clone()),
            amount: ActiveValue::Set(entry.amount_minor),
            currency: ActiveValue::Set(entry.currency.code().to_string()),
            note: ActiveValue::Set(Some(entry.note.clone())),
            category: ActiveValue::Set(Some(entry.category.clone())),
            date: ActiveValue::Set(entry.date),
            vault_id: ActiveValue::NotSet,
            cash_flow_id: ActiveValue::NotSet,
            wallet_id: ActiveValue::NotSet,
        }
    }
}
