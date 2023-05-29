//! The module contains the `Entry` type representing an entry in cash flows and wallets.
//!
//! Both expenses and income are represented by `Entry` type.
use sea_orm::{entity::prelude::*, ActiveValue};
use serde::Serialize;
use uuid::Uuid;

/// Represent a movement, an entry in cash flows or wallets.
#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub id: String,
    pub amount: f64,
    pub category: String,
    pub note: String,
    pub cash_flow: Option<String>,
    pub wallet: Option<String>,
}

/// Type used to represent an entry in cash flows and wallets.
impl Entry {
    pub fn new(
        amount: f64,
        category: String,
        note: String,
        cash_flow: Option<String>,
        wallet: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            amount,
            category,
            note,
            cash_flow,
            wallet,
        }
    }
}

impl From<Model> for Entry {
    fn from(entry: Model) -> Self {
        Self {
            id: entry.id,
            amount: entry.amount,
            category: entry.category.unwrap(),
            note: entry.note.unwrap(),
            cash_flow: entry.cash_flow_id,
            wallet: entry.wallet_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "entries")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(column_type = "Double")]
    pub amount: f64,
    pub note: Option<String>,
    pub category: Option<String>,
    pub cash_flow_id: Option<String>,
    pub wallet_id: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::cash_flows::Entity",
        from = "Column::CashFlowId",
        to = "super::cash_flows::Column::Name",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    CashFlows,
    #[sea_orm(
        belongs_to = "super::wallets::Entity",
        from = "Column::WalletId",
        to = "super::wallets::Column::Name",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
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

impl From<&Entry> for ActiveModel {
    fn from(entry: &Entry) -> Self {
        Self {
            id: ActiveValue::Set(entry.id.clone()),
            amount: ActiveValue::Set(entry.amount),
            note: ActiveValue::Set(Some(entry.note.clone())),
            category: ActiveValue::Set(Some(entry.category.clone())),
            cash_flow_id: ActiveValue::Set(entry.cash_flow.clone()),
            wallet_id: ActiveValue::Set(entry.wallet.clone()),
        }
    }
}
