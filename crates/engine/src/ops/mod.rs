use chrono::{DateTime, Utc};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::{
    Currency, EngineError, Leg, LegTarget, ResultEngine, Transaction, TransactionKind,
    TransactionNew,
    util::{model_currency, normalize_optional_text, normalize_required_name},
};

mod access;
mod balances;
mod flows;
mod memberships;
mod transactions;
mod vaults;
mod wallets;

pub use transactions::TransactionListFilter;

/// Run a block inside a DB transaction, committing on success and rolling back
/// on error.
macro_rules! with_tx {
    ($self:expr, |$tx:ident| $body:expr) => {{
        let $tx = $self.database.begin().await?;
        let result = $body;
        match result {
            Ok(value) => {
                $tx.commit().await?;
                Ok(value)
            }
            Err(err) => Err(err),
        }
    }};
}

pub(crate) use with_tx;

/// Parse a vault_id string into Uuid for DB queries.
pub(crate) fn parse_vault_uuid(vault_id: &str) -> ResultEngine<Uuid> {
    Uuid::parse_str(vault_id)
        .map_err(|_| EngineError::KeyNotFound("vault not found".to_string()))
}

#[derive(Debug)]
pub struct Engine {
    database: DatabaseConnection,
}

impl Engine {
    /// Return a builder for `Engine`. Help to build the struct.
    pub fn builder() -> EngineBuilder {
        EngineBuilder::default()
    }
}

fn flow_wallet_signed_amount(kind: TransactionKind, amount_minor: i64) -> ResultEngine<i64> {
    match kind {
        TransactionKind::Income | TransactionKind::Refund => Ok(amount_minor),
        TransactionKind::Expense => Ok(-amount_minor),
        _ => Err(EngineError::InvalidAmount(
            "invalid transaction: unexpected kind".to_string(),
        )),
    }
}

fn parse_vault_currency(value: &str) -> ResultEngine<Currency> {
    model_currency(value)
}

pub(super) struct TransactionBuildInput<'a> {
    pub(super) vault_id: &'a str,
    pub(super) kind: TransactionKind,
    pub(super) occurred_at: DateTime<Utc>,
    pub(super) amount_minor: i64,
    pub(super) currency: Currency,
    pub(super) category: Option<String>,
    pub(super) note: Option<String>,
    pub(super) created_by: &'a str,
    pub(super) idempotency_key: Option<String>,
    pub(super) refunded_transaction_id: Option<Uuid>,
}

fn build_transaction(input: TransactionBuildInput<'_>) -> ResultEngine<Transaction> {
    Transaction::new(TransactionNew {
        vault_id: input.vault_id.to_string(),
        kind: input.kind,
        occurred_at: input.occurred_at,
        amount_minor: input.amount_minor,
        currency: input.currency,
        category: input.category,
        note: input.note,
        created_by: input.created_by.to_string(),
        idempotency_key: input.idempotency_key,
        refunded_transaction_id: input.refunded_transaction_id,
    })
}

fn flow_wallet_legs(
    tx_id: Uuid,
    wallet_id: Uuid,
    flow_id: Uuid,
    signed_amount_minor: i64,
    currency: Currency,
) -> Vec<Leg> {
    vec![
        Leg::new(
            tx_id,
            LegTarget::Wallet { wallet_id },
            signed_amount_minor,
            currency,
        ),
        Leg::new(
            tx_id,
            LegTarget::Flow { flow_id },
            signed_amount_minor,
            currency,
        ),
    ]
}

fn transfer_wallet_legs(
    tx_id: Uuid,
    from_wallet_id: Uuid,
    to_wallet_id: Uuid,
    amount_minor: i64,
    currency: Currency,
) -> Vec<Leg> {
    vec![
        Leg::new(
            tx_id,
            LegTarget::Wallet {
                wallet_id: from_wallet_id,
            },
            -amount_minor,
            currency,
        ),
        Leg::new(
            tx_id,
            LegTarget::Wallet {
                wallet_id: to_wallet_id,
            },
            amount_minor,
            currency,
        ),
    ]
}

fn transfer_flow_legs(
    tx_id: Uuid,
    from_flow_id: Uuid,
    to_flow_id: Uuid,
    amount_minor: i64,
    currency: Currency,
) -> Vec<Leg> {
    vec![
        Leg::new(
            tx_id,
            LegTarget::Flow {
                flow_id: from_flow_id,
            },
            -amount_minor,
            currency,
        ),
        Leg::new(
            tx_id,
            LegTarget::Flow {
                flow_id: to_flow_id,
            },
            amount_minor,
            currency,
        ),
    ]
}

/// The builder for `Engine`
#[derive(Default)]
pub struct EngineBuilder {
    database: DatabaseConnection,
}

impl EngineBuilder {
    /// Pass the required database
    pub fn database(mut self, db: DatabaseConnection) -> EngineBuilder {
        self.database = db;
        self
    }

    /// Construct `Engine`
    pub async fn build(self) -> ResultEngine<Engine> {
        Ok(Engine {
            database: self.database,
        })
    }
}
