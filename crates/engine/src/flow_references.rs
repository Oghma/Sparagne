//! Flow references for cross-vault sharing.
//!
//! A flow reference allows a flow from one vault to appear in another vault,
//! enabling cross-vault sharing without duplicating flow data.

use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "flow_references")]
pub struct Model {
    /// Reference ID
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// The vault where this reference appears
    pub vault_id: Uuid,
    /// The flow being referenced (lives in a different vault)
    pub target_flow_id: Uuid,
    /// Optional display name override for this vault
    pub display_name: Option<String>,
    /// When this reference was created
    pub created_at: DateTime<Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::vault::Entity",
        from = "Column::VaultId",
        to = "super::vault::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Vault,
    #[sea_orm(
        belongs_to = "super::cash_flows::Entity",
        from = "Column::TargetFlowId",
        to = "super::cash_flows::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    CashFlow,
}

impl Related<super::vault::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Vault.def()
    }
}

impl Related<super::cash_flows::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CashFlow.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
