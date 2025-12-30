//! Transaction legs.
//!
//! A [`Leg`] is a single balance change applied to a target (a wallet or a
//! cash flow) as part of a [`Transaction`](crate::Transaction).
//!
//! Amounts are stored as signed integer **minor units** (e.g. cents for EUR):
//! - positive values increase the target balance
//! - negative values decrease the target balance
//!
//! In the engine, *every* change to balances happens via legs.

use sea_orm::{ActiveValue, entity::prelude::*};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Currency, EngineError};

/// The type of target a leg affects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "Text")]
#[serde(rename_all = "snake_case")]
pub enum LegTargetKind {
    #[sea_orm(string_value = "wallet")]
    Wallet,
    #[sea_orm(string_value = "flow")]
    Flow,
}

impl LegTargetKind {
    /// Returns the string representation used in the database.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wallet => "wallet",
            Self::Flow => "flow",
        }
    }
}

impl TryFrom<&str> for LegTargetKind {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "wallet" => Ok(Self::Wallet),
            "flow" => Ok(Self::Flow),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum LegTarget {
    Wallet { wallet_id: Uuid },
    Flow { flow_id: Uuid },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leg {
    pub id: Uuid,
    pub transaction_id: Uuid,
    pub target: LegTarget,
    pub amount_minor: i64,
    pub currency: Currency,
    pub attributed_user_id: Option<String>,
}

impl Leg {
    pub fn new(
        transaction_id: Uuid,
        target: LegTarget,
        amount_minor: i64,
        currency: Currency,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            transaction_id,
            target,
            amount_minor,
            currency,
            attributed_user_id: None,
        }
    }

    fn target_kind(&self) -> LegTargetKind {
        match self.target {
            LegTarget::Wallet { .. } => LegTargetKind::Wallet,
            LegTarget::Flow { .. } => LegTargetKind::Flow,
        }
    }

    fn target_id(&self) -> Uuid {
        match self.target {
            LegTarget::Wallet { wallet_id } => wallet_id,
            LegTarget::Flow { flow_id } => flow_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "legs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub transaction_id: Uuid,
    pub target_kind: LegTargetKind,
    pub target_id: Uuid,
    pub amount_minor: i64,
    pub currency: String,
    pub attributed_user_id: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::transactions::Entity",
        from = "Column::TransactionId",
        to = "super::transactions::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    Transactions,
}

impl Related<super::transactions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Transactions.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

impl From<&Leg> for ActiveModel {
    fn from(leg: &Leg) -> Self {
        Self {
            id: ActiveValue::Set(leg.id),
            transaction_id: ActiveValue::Set(leg.transaction_id),
            target_kind: ActiveValue::Set(leg.target_kind()),
            target_id: ActiveValue::Set(leg.target_id()),
            amount_minor: ActiveValue::Set(leg.amount_minor),
            currency: ActiveValue::Set(leg.currency.code().to_string()),
            attributed_user_id: ActiveValue::Set(leg.attributed_user_id.clone()),
        }
    }
}

impl TryFrom<Model> for Leg {
    type Error = EngineError;

    fn try_from(model: Model) -> Result<Self, Self::Error> {
        let target = match model.target_kind {
            LegTargetKind::Wallet => LegTarget::Wallet {
                wallet_id: model.target_id,
            },
            LegTargetKind::Flow => LegTarget::Flow {
                flow_id: model.target_id,
            },
        };

        Ok(Self {
            id: model.id,
            transaction_id: model.transaction_id,
            target,
            amount_minor: model.amount_minor,
            currency: Currency::try_from(model.currency.as_str()).unwrap_or_default(),
            attributed_user_id: model.attributed_user_id,
        })
    }
}
