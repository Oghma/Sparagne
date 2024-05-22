//! The module contains `Wallet` struct and its implementation.

use std::time::Duration;

use sea_orm::entity::{prelude::*, ActiveValue};

use crate::{entry::Entry, EngineError, ResultEngine};

/// A wallet.
///
/// A wallet is a representation of a real wallet, a bank account or anything
/// else where money are kept. It is not a representation of a credit card.
#[derive(Debug)]
pub struct Wallet {
    pub name: String,
    pub balance: f64,
    pub entries: Vec<Entry>,
    pub archived: bool,
}

impl Wallet {
    pub fn new(name: String, balance: f64) -> Self {
        Self {
            name,
            balance,
            entries: Vec::new(),
            archived: false,
        }
    }

    pub fn add_entry(
        &mut self,
        balance: f64,
        category: String,
        note: String,
        date: Duration,
    ) -> ResultEngine<&Entry> {
        let entry = Entry::new(balance, category, note, date);
        self.balance += entry.amount;
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
                self.balance -= entry.amount;
                Ok(entry)
            }
            None => Err(EngineError::KeyNotFound(id.to_string())),
        }
    }

    /// Insert an existing `Entry` into the wallet.
    pub fn insert_entry(&mut self, entry: &Entry) {
        self.balance += entry.amount;
        self.entries.push(entry.clone());
    }

    pub fn update_entry(
        &mut self,
        id: &str,
        amount: f64,
        category: String,
        note: String,
    ) -> ResultEngine<&Entry> {
        match self.entries.iter().position(|entry| entry.id == id) {
            Some(index) => {
                let entry = &mut self.entries[index];
                self.balance = self.balance - entry.amount + amount;

                entry.amount = amount;
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
    pub name: String,
    #[sea_orm(column_type = "Double")]
    pub balance: f64,
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
            name: ActiveValue::Set(value.name.clone()),
            balance: ActiveValue::Set(value.balance),
            archived: ActiveValue::Set(value.archived),
            vault_id: ActiveValue::NotSet,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn wallet() -> Wallet {
        Wallet::new(String::from("Cash"), 0f64)
    }

    #[test]
    fn add_entry() {
        let mut wallet = wallet();
        wallet
            .add_entry(
                10.4,
                String::from("Income"),
                String::from("Hard work"),
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
            )
            .unwrap();
        let entry = &wallet.entries[0];

        assert_eq!(wallet.name, "Cash".to_string());
        assert_eq!(wallet.balance, 10.4);
        assert_eq!(entry.amount, 10.4);
        assert_eq!(entry.category, "Income".to_string());
        assert_eq!(entry.note, "Hard work".to_string());
    }

    #[test]
    fn update_entry() {
        let mut wallet = wallet();
        wallet
            .add_entry(
                10.4,
                String::from("Income"),
                String::from("Hard work"),
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
            )
            .unwrap();
        let entry_id = wallet.entries[0].id.clone();

        wallet
            .update_entry(
                &entry_id,
                10f64,
                String::from("Income"),
                String::from("Monthly"),
            )
            .unwrap();
        let entry = &wallet.entries[0];

        assert_eq!(wallet.balance, 10f64);
        assert_eq!(entry.amount, 10f64);
        assert_eq!(entry.category, String::from("Income"));
        assert_eq!(entry.note, String::from("Monthly"))
    }

    #[test]
    #[should_panic(expected = "KeyNotFound(\"6a8416ed-b8e6-4732-a591-bf55da9687e7\")")]
    fn fail_update_entry() {
        let mut wallet = wallet();
        wallet
            .add_entry(
                1.23,
                "Income".to_string(),
                "Weekly".to_string(),
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
            )
            .unwrap();

        wallet
            .update_entry(
                "6a8416ed-b8e6-4732-a591-bf55da9687e7",
                20f64,
                String::from("Income"),
                String::from("Monthly"),
            )
            .unwrap();
    }
}
