//! Transaction primitives.
//!
//! A `Transaction` is an atomic event that changes balances via one or more
//! `Leg`s.

use chrono::{DateTime, Utc};
use sea_orm::{ActiveValue, entity::prelude::*};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Currency, EngineError, ResultEngine};

use super::legs;

#[derive(Clone, Debug)]
pub struct TransactionNew {
    pub vault_id: String,
    pub kind: TransactionKind,
    pub occurred_at: DateTime<Utc>,
    pub amount_minor: i64,
    pub currency: Currency,
    pub category: Option<String>,
    pub note: Option<String>,
    pub created_by: String,
    pub idempotency_key: Option<String>,
    pub refunded_transaction_id: Option<Uuid>,
}

/// The type of a financial transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "Text")]
#[serde(rename_all = "snake_case")]
pub enum TransactionKind {
    #[sea_orm(string_value = "income")]
    Income,
    #[sea_orm(string_value = "expense")]
    Expense,
    #[sea_orm(string_value = "transfer_wallet")]
    TransferWallet,
    #[sea_orm(string_value = "transfer_flow")]
    TransferFlow,
    #[sea_orm(string_value = "refund")]
    Refund,
}

impl TransactionKind {
    /// Returns the string representation used in the database.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Income => "income",
            Self::Expense => "expense",
            Self::TransferWallet => "transfer_wallet",
            Self::TransferFlow => "transfer_flow",
            Self::Refund => "refund",
        }
    }
}

impl TryFrom<&str> for TransactionKind {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "income" => Ok(Self::Income),
            "expense" => Ok(Self::Expense),
            "transfer_wallet" => Ok(Self::TransferWallet),
            "transfer_flow" => Ok(Self::TransferFlow),
            "refund" => Ok(Self::Refund),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    pub id: Uuid,
    pub vault_id: String,
    pub kind: TransactionKind,
    pub occurred_at: DateTime<Utc>,
    pub amount_minor: i64,
    pub idempotency_key: Option<String>,
    pub currency: Currency,
    pub category: Option<String>,
    pub note: Option<String>,
    pub created_by: String,
    pub voided_at: Option<DateTime<Utc>>,
    pub voided_by: Option<String>,
    pub refunded_transaction_id: Option<Uuid>,
    pub legs: Vec<legs::Leg>,
}

impl Transaction {
    pub fn new(input: TransactionNew) -> ResultEngine<Self> {
        if input.amount_minor <= 0 {
            return Err(EngineError::InvalidAmount(
                "amount_minor must be > 0".to_string(),
            ));
        }
        if let Some(key) = &input.idempotency_key
            && key.trim().is_empty()
        {
            return Err(EngineError::InvalidAmount(
                "idempotency_key must not be empty".to_string(),
            ));
        }
        Ok(Self {
            id: Uuid::new_v4(),
            vault_id: input.vault_id,
            kind: input.kind,
            occurred_at: input.occurred_at,
            amount_minor: input.amount_minor,
            idempotency_key: input.idempotency_key,
            currency: input.currency,
            category: input.category,
            note: input.note,
            created_by: input.created_by,
            voided_at: None,
            voided_by: None,
            refunded_transaction_id: input.refunded_transaction_id,
            legs: Vec::new(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "transactions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub vault_id: Uuid,
    pub kind: TransactionKind,
    pub occurred_at: DateTimeUtc,
    pub amount_minor: i64,
    pub idempotency_key: Option<String>,
    pub currency: String,
    pub category: Option<String>,
    pub note: Option<String>,
    pub created_by: String,
    pub voided_at: Option<DateTimeUtc>,
    pub voided_by: Option<String>,
    pub refunded_transaction_id: Option<Uuid>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::legs::Entity")]
    Legs,
}

impl Related<super::legs::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Legs.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

impl From<&Transaction> for ActiveModel {
    fn from(tx: &Transaction) -> Self {
        Self {
            id: ActiveValue::Set(tx.id),
            vault_id: ActiveValue::Set(
                Uuid::parse_str(&tx.vault_id).expect("Transaction.vault_id must be a valid UUID"),
            ),
            kind: ActiveValue::Set(tx.kind),
            occurred_at: ActiveValue::Set(tx.occurred_at),
            amount_minor: ActiveValue::Set(tx.amount_minor),
            idempotency_key: ActiveValue::Set(tx.idempotency_key.clone()),
            currency: ActiveValue::Set(tx.currency.code().to_string()),
            category: ActiveValue::Set(tx.category.clone()),
            note: ActiveValue::Set(tx.note.clone()),
            created_by: ActiveValue::Set(tx.created_by.clone()),
            voided_at: ActiveValue::Set(tx.voided_at),
            voided_by: ActiveValue::Set(tx.voided_by.clone()),
            refunded_transaction_id: ActiveValue::Set(tx.refunded_transaction_id),
        }
    }
}

impl TryFrom<Model> for Transaction {
    type Error = EngineError;

    fn try_from(model: Model) -> Result<Self, Self::Error> {
        Ok(Self {
            id: model.id,
            vault_id: model.vault_id.to_string(),
            kind: model.kind,
            occurred_at: model.occurred_at,
            amount_minor: model.amount_minor,
            idempotency_key: model.idempotency_key,
            currency: Currency::try_from(model.currency.as_str()).unwrap_or_default(),
            category: model.category,
            note: model.note,
            created_by: model.created_by,
            voided_at: model.voided_at,
            voided_by: model.voided_by,
            refunded_transaction_id: model.refunded_transaction_id,
            legs: Vec::new(),
        })
    }
}
