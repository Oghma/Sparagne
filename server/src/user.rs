//! The module contains the definition of a user and its

use sea_orm::{entity::prelude::*, ActiveValue};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub username: String,
    pub password: String,
    pub telegram_id: Option<String>,
    pub pair_code: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// Struct for pairing user
#[derive(Deserialize, Serialize)]
pub struct PairUser {
    pub code: String,
    pub telegram_id: String,
}
