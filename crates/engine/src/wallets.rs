//! Wallets.

use sea_orm::entity::{ActiveValue, prelude::*};
use uuid::Uuid;

use crate::{Currency, EngineError, ResultEngine, util::{ensure_vault_currency, model_currency}};

/// A wallet.
///
/// A wallet is a representation of a real wallet, a bank account or anything
/// else where money are kept. It is not a representation of a credit card.
#[derive(Debug)]
pub struct Wallet {
    /// Stable identifier for this wallet.
    ///
    /// This is a UUID generated once and persisted in the database, so the
    /// wallet can be renamed without breaking references.
    pub id: Uuid,
    pub name: String,
    pub balance: i64,
    pub currency: Currency,
    pub archived: bool,
}

impl Wallet {
    pub fn new(name: String, balance: i64, currency: Currency) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            balance,
            currency,
            archived: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "wallets")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    pub balance: i64,
    pub currency: String,
    pub archived: bool,
    pub vault_id: Uuid,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::vault::Entity",
        from = "Column::VaultId",
        to = "super::vault::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    Vaults,
}

impl Related<super::vault::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Vaults.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

/// Convert a storage model into a domain `Wallet`, validating currency.
impl TryFrom<(Model, Currency)> for Wallet {
    type Error = EngineError;

    fn try_from((model, vault_currency): (Model, Currency)) -> ResultEngine<Self> {
        let currency = model_currency(&model.currency)?;
        ensure_vault_currency(vault_currency, currency)?;
        Ok(Self {
            id: model.id,
            name: model.name,
            balance: model.balance,
            currency,
            archived: model.archived,
        })
    }
}

impl From<&Wallet> for ActiveModel {
    fn from(value: &Wallet) -> Self {
        Self {
            id: ActiveValue::Set(value.id),
            name: ActiveValue::Set(value.name.clone()),
            balance: ActiveValue::Set(value.balance),
            currency: ActiveValue::Set(value.currency.code().to_string()),
            archived: ActiveValue::Set(value.archived),
            vault_id: ActiveValue::NotSet,
        }
    }
}
