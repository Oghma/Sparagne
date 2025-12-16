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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionKind {
    Income,
    Expense,
    TransferWallet,
    TransferFlow,
    Refund,
}

impl TransactionKind {
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
    type Error = EngineError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "income" => Ok(Self::Income),
            "expense" => Ok(Self::Expense),
            "transfer_wallet" => Ok(Self::TransferWallet),
            "transfer_flow" => Ok(Self::TransferFlow),
            "refund" => Ok(Self::Refund),
            other => Err(EngineError::InvalidAmount(format!(
                "invalid transaction kind: {other}"
            ))),
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
    pub fn new(
        vault_id: String,
        kind: TransactionKind,
        occurred_at: DateTime<Utc>,
        amount_minor: i64,
        currency: Currency,
        category: Option<String>,
        note: Option<String>,
        created_by: String,
    ) -> ResultEngine<Self> {
        if amount_minor <= 0 {
            return Err(EngineError::InvalidAmount(
                "amount_minor must be > 0".to_string(),
            ));
        }
        Ok(Self {
            id: Uuid::new_v4(),
            vault_id,
            kind,
            occurred_at,
            amount_minor,
            currency,
            category,
            note,
            created_by,
            voided_at: None,
            voided_by: None,
            refunded_transaction_id: None,
            legs: Vec::new(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "transactions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub vault_id: String,
    pub kind: String,
    pub occurred_at: DateTimeUtc,
    pub amount_minor: i64,
    pub currency: String,
    pub category: Option<String>,
    pub note: Option<String>,
    pub created_by: String,
    pub voided_at: Option<DateTimeUtc>,
    pub voided_by: Option<String>,
    pub refunded_transaction_id: Option<String>,
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
            id: ActiveValue::Set(tx.id.to_string()),
            vault_id: ActiveValue::Set(tx.vault_id.clone()),
            kind: ActiveValue::Set(tx.kind.as_str().to_string()),
            occurred_at: ActiveValue::Set(tx.occurred_at),
            amount_minor: ActiveValue::Set(tx.amount_minor),
            currency: ActiveValue::Set(tx.currency.code().to_string()),
            category: ActiveValue::Set(tx.category.clone()),
            note: ActiveValue::Set(tx.note.clone()),
            created_by: ActiveValue::Set(tx.created_by.clone()),
            voided_at: ActiveValue::Set(tx.voided_at),
            voided_by: ActiveValue::Set(tx.voided_by.clone()),
            refunded_transaction_id: ActiveValue::Set(
                tx.refunded_transaction_id.map(|id| id.to_string()),
            ),
        }
    }
}

impl TryFrom<Model> for Transaction {
    type Error = EngineError;

    fn try_from(model: Model) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&model.id)
                .map_err(|_| EngineError::KeyNotFound("transaction not exists".to_string()))?,
            vault_id: model.vault_id,
            kind: TransactionKind::try_from(model.kind.as_str())?,
            occurred_at: model.occurred_at,
            amount_minor: model.amount_minor,
            currency: Currency::try_from(model.currency.as_str()).unwrap_or_default(),
            category: model.category,
            note: model.note,
            created_by: model.created_by,
            voided_at: model.voided_at,
            voided_by: model.voided_by,
            refunded_transaction_id: model
                .refunded_transaction_id
                .and_then(|s| Uuid::parse_str(&s).ok()),
            legs: Vec::new(),
        })
    }
}
