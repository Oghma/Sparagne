//! Wallets.

use sea_orm::entity::{ActiveValue, prelude::*};
use uuid::Uuid;

use crate::Currency;

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

    pub fn with_id(id: Uuid, name: String, balance: i64, currency: Currency) -> Self {
        Self {
            id,
            name,
            balance,
            currency,
            archived: false,
        }
    }

    pub fn archive(&mut self) {
        self.archived = true;
    }

    pub fn apply_leg_change(&mut self, old_amount_minor: i64, new_amount_minor: i64) {
        self.balance = self.balance - old_amount_minor + new_amount_minor;
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "wallets")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub balance: i64,
    pub currency: String,
    pub archived: bool,
    pub vault_id: String,
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

impl From<&Wallet> for ActiveModel {
    fn from(value: &Wallet) -> Self {
        Self {
            id: ActiveValue::Set(value.id.to_string()),
            name: ActiveValue::Set(value.name.clone()),
            balance: ActiveValue::Set(value.balance),
            currency: ActiveValue::Set(value.currency.code().to_string()),
            archived: ActiveValue::Set(value.archived),
            vault_id: ActiveValue::NotSet,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wallet() -> Wallet {
        Wallet::new(String::from("Cash"), 0, Currency::Eur)
    }

    #[test]
    fn apply_leg_change() {
        let mut wallet = wallet();
        wallet.apply_leg_change(0, 1040);
        assert_eq!(wallet.balance, 1040);
    }

    #[test]
    fn apply_leg_change_update() {
        let mut wallet = wallet();
        wallet.apply_leg_change(0, 1040);
        wallet.apply_leg_change(1040, 1000);
        assert_eq!(wallet.balance, 1000);
    }
}
