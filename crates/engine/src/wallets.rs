//! The module contains `Wallet` struct and its implementation.

use chrono::{DateTime, Utc};

use sea_orm::entity::{ActiveValue, prelude::*};
use uuid::Uuid;

use crate::{Currency, EngineError, ResultEngine, entry::Entry};

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
    pub entries: Vec<Entry>,
    pub archived: bool,
}

impl Wallet {
    pub fn new(name: String, balance: i64, currency: Currency) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            balance,
            currency,
            entries: Vec::new(),
            archived: false,
        }
    }

    pub fn with_id(id: Uuid, name: String, balance: i64, currency: Currency) -> Self {
        Self {
            id,
            name,
            balance,
            currency,
            entries: Vec::new(),
            archived: false,
        }
    }

    pub fn add_entry(
        &mut self,
        amount_minor: i64,
        category: String,
        note: String,
        date: DateTime<Utc>,
    ) -> ResultEngine<&Entry> {
        let entry = Entry::new(amount_minor, self.currency, category, note, date);
        self.balance += entry.amount_minor;
        self.entries.push(entry);

        Ok(&self.entries[self.entries.len() - 1])
    }

    pub fn archive(&mut self) {
        self.archived = true;
    }

    pub fn delete_entry(&mut self, id: &str) -> ResultEngine<Entry> {
        match self.entries.iter().position(|entry| entry.id == id) {
            Some(index) => {
                let entry = self.entries.remove(index);
                self.balance -= entry.amount_minor;
                Ok(entry)
            }
            None => Err(EngineError::KeyNotFound(id.to_string())),
        }
    }

    /// Insert an existing `Entry` into the wallet.
    pub fn insert_entry(&mut self, entry: &Entry) {
        self.balance += entry.amount_minor;
        self.entries.push(entry.clone());
    }

    pub fn update_entry(
        &mut self,
        id: &str,
        amount_minor: i64,
        category: String,
        note: String,
    ) -> ResultEngine<&Entry> {
        match self.entries.iter().position(|entry| entry.id == id) {
            Some(index) => {
                let entry = &mut self.entries[index];
                self.balance = self.balance - entry.amount_minor + amount_minor;

                entry.amount_minor = amount_minor;
                entry.category = category;
                entry.note = note;

                Ok(entry)
            }
            None => Err(EngineError::KeyNotFound(id.to_string())),
        }
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
    #[sea_orm(has_many = "super::entry::Entity")]
    Entries,
    #[sea_orm(
        belongs_to = "super::vault::Entity",
        from = "Column::VaultId",
        to = "super::vault::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    Vaults,
}

impl Related<super::entry::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Entries.def()
    }
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
    use chrono::{TimeZone, Utc};

    use super::*;

    fn wallet() -> Wallet {
        Wallet::new(String::from("Cash"), 0, Currency::Eur)
    }

    #[test]
    fn add_entry() {
        let mut wallet = wallet();
        wallet
            .add_entry(
                1040,
                String::from("Income"),
                String::from("Hard work"),
                Utc.timestamp_opt(0, 0).unwrap(),
            )
            .unwrap();
        let entry = &wallet.entries[0];

        assert_eq!(wallet.name, "Cash".to_string());
        assert_eq!(wallet.balance, 1040);
        assert_eq!(entry.amount_minor, 1040);
        assert_eq!(entry.currency, Currency::Eur);
        assert_eq!(entry.category, "Income".to_string());
        assert_eq!(entry.note, "Hard work".to_string());
    }

    #[test]
    fn update_entry() {
        let mut wallet = wallet();
        wallet
            .add_entry(
                1040,
                String::from("Income"),
                String::from("Hard work"),
                Utc.timestamp_opt(0, 0).unwrap(),
            )
            .unwrap();
        let entry_id = wallet.entries[0].id.clone();

        wallet
            .update_entry(
                &entry_id,
                1000,
                String::from("Income"),
                String::from("Monthly"),
            )
            .unwrap();
        let entry = &wallet.entries[0];

        assert_eq!(wallet.balance, 1000);
        assert_eq!(entry.amount_minor, 1000);
        assert_eq!(entry.category, String::from("Income"));
        assert_eq!(entry.note, String::from("Monthly"))
    }

    #[test]
    #[should_panic(expected = "KeyNotFound(\"6a8416ed-b8e6-4732-a591-bf55da9687e7\")")]
    fn fail_update_entry() {
        let mut wallet = wallet();
        wallet
            .add_entry(
                123,
                "Income".to_string(),
                "Weekly".to_string(),
                Utc.timestamp_opt(0, 0).unwrap(),
            )
            .unwrap();

        wallet
            .update_entry(
                "6a8416ed-b8e6-4732-a591-bf55da9687e7",
                2000,
                String::from("Income"),
                String::from("Monthly"),
            )
            .unwrap();
    }
}
