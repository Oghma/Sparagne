//! Category registry per vault.

use sea_orm::entity::prelude::*;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "categories")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub vault_id: Uuid,
    pub name: String,
    pub name_norm: String,
    pub archived: bool,
    pub is_system: bool,
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
    #[sea_orm(has_many = "super::category_aliases::Entity")]
    Aliases,
}

impl Related<super::vault::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Vault.def()
    }
}

impl Related<super::category_aliases::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Aliases.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
