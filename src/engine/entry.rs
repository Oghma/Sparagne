//! The module contains the `Entry` type representing an entry in cash flows and wallets.
//!
//! Both expenses and income are represented by `Entry` type.
use sea_orm::entity::prelude::*;
use uuid::Uuid;

/// Represent a movement, an entry in cash flows or wallets.
#[derive(Debug)]
pub struct Entry {
    pub id: String,
    pub amount: f64,
    pub category: String,
    pub note: String,
    pub cash_flow: String,
}

/// Type used to represent an entry in cash flows and wallets.
impl Entry {
    pub fn new(amount: f64, category: String, note: String, cash_flow: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            amount,
            category,
            note,
            cash_flow,
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
    pub cash_flow_id: String,
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
}

impl Related<super::cash_flows::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CashFlows.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
