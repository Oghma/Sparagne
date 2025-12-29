use chrono::{DateTime, Utc};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::{
    Currency, EngineError, Leg, LegTarget, ResultEngine, Transaction, TransactionKind,
    TransactionNew,
};

mod access;
mod balances;
mod flows;
mod memberships;
mod transactions;
mod vaults;
mod wallets;

pub use transactions::TransactionListFilter;

/// Run a block inside a DB transaction, committing on success and rolling back on error.
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

fn normalize_required_name(value: &str, label: &str) -> ResultEngine<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EngineError::InvalidAmount(format!(
            "{label} name must not be empty"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalize_required_flow_name(value: &str) -> ResultEngine<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EngineError::InvalidFlow(
            "flow name must not be empty".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

fn flow_wallet_signed_amount(
    kind: TransactionKind,
    amount_minor: i64,
) -> ResultEngine<i64> {
    match kind {
        TransactionKind::Income | TransactionKind::Refund => Ok(amount_minor),
        TransactionKind::Expense => Ok(-amount_minor),
        _ => Err(EngineError::InvalidAmount(
            "invalid transaction: unexpected kind".to_string(),
        )),
    }
}

fn build_transaction(
    vault_id: &str,
    kind: TransactionKind,
    occurred_at: DateTime<Utc>,
    amount_minor: i64,
    currency: Currency,
    category: Option<String>,
    note: Option<String>,
    created_by: &str,
    idempotency_key: Option<String>,
    refunded_transaction_id: Option<Uuid>,
) -> ResultEngine<Transaction> {
    Transaction::new(TransactionNew {
        vault_id: vault_id.to_string(),
        kind,
        occurred_at,
        amount_minor,
        currency,
        category,
        note,
        created_by: created_by.to_string(),
        idempotency_key,
        refunded_transaction_id,
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
            LegTarget::Flow { flow_id: to_flow_id },
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
