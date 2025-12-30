//! Flow memberships scaffolding for future sharing.

use sea_orm::entity::prelude::*;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "flow_memberships")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub flow_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: String,
    pub role: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::cash_flows::Entity",
        from = "Column::FlowId",
        to = "super::cash_flows::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    CashFlows,
}

impl Related<super::cash_flows::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CashFlows.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
