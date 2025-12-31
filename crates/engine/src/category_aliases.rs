//! Category aliases per vault.

use sea_orm::entity::prelude::*;
use uuid::Uuid;

/// Alias entry exposed to clients.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryAlias {
    pub id: Uuid,
    pub alias: String,
    pub category_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "category_aliases")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub vault_id: Uuid,
    pub category_id: Uuid,
    pub alias: String,
    pub alias_norm: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::categories::Entity",
        from = "Column::CategoryId",
        to = "super::categories::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Categories,
    #[sea_orm(
        belongs_to = "super::vault::Entity",
        from = "Column::VaultId",
        to = "super::vault::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Vault,
}

impl Related<super::categories::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Categories.def()
    }
}

impl Related<super::vault::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Vault.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for CategoryAlias {
    fn from(model: Model) -> Self {
        Self {
            id: model.id,
            alias: model.alias,
            category_id: model.category_id,
        }
    }
}
