use base64::Engine as _;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use sea_orm::{Condition, QueryFilter, QueryOrder, QuerySelect, TransactionTrait, prelude::*};

use crate::{EngineError, ResultEngine, Transaction, TransactionKind, legs, transactions};

use super::super::{Engine, with_tx};

/// Filters for listing transactions.
///
/// `from` is inclusive and `to` is exclusive (`[from, to)`), both in UTC.
#[derive(Clone, Debug, Default)]
pub struct TransactionListFilter {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    /// If present, acts as an allow-list of kinds to return.
    pub kinds: Option<Vec<TransactionKind>>,
    /// If true, includes voided transactions (default: false).
    pub include_voided: bool,
    /// If true, includes internal transfers (default: false).
    pub include_transfers: bool,
}

fn validate_list_filter(filter: &TransactionListFilter) -> ResultEngine<()> {
    if let (Some(from), Some(to)) = (filter.from, filter.to)
        && from >= to
    {
        return Err(EngineError::InvalidAmount(
            "invalid range: from must be < to".to_string(),
        ));
    }
    if filter.kinds.as_ref().is_some_and(|k| k.is_empty()) {
        return Err(EngineError::InvalidAmount(
            "kinds must not be empty".to_string(),
        ));
    }
    Ok(())
}

trait ApplyTxFilters: QueryFilter + Sized {
    fn apply_tx_filters(self, filter: &TransactionListFilter) -> Self;
}

impl<T> ApplyTxFilters for T
where
    T: QueryFilter + Sized,
{
    fn apply_tx_filters(mut self, filter: &TransactionListFilter) -> Self {
        if let Some(from) = filter.from {
            self = self.filter(transactions::Column::OccurredAt.gte(from));
        }
        if let Some(to) = filter.to {
            self = self.filter(transactions::Column::OccurredAt.lt(to));
        }

        if !filter.include_voided {
            self = self.filter(transactions::Column::VoidedAt.is_null());
        }
        if let Some(kinds) = &filter.kinds {
            let kinds: Vec<String> = kinds.iter().map(|k| k.as_str().to_string()).collect();
            self = self.filter(transactions::Column::Kind.is_in(kinds));
        } else if !filter.include_transfers {
            self = self.filter(transactions::Column::Kind.is_not_in([
                TransactionKind::TransferWallet.as_str(),
                TransactionKind::TransferFlow.as_str(),
            ]));
        }

        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TransactionsCursor {
    occurred_at: DateTime<Utc>,
    transaction_id: String,
}

impl TransactionsCursor {
    fn encode(&self) -> ResultEngine<String> {
        let bytes = serde_json::to_vec(self)
            .map_err(|_| EngineError::InvalidCursor("invalid transactions cursor".to_string()))?;
        Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
    }

    fn decode(input: &str) -> ResultEngine<Self> {
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(input.as_bytes())
            .map_err(|_| EngineError::InvalidCursor("invalid transactions cursor".to_string()))?;
        serde_json::from_slice::<Self>(&bytes)
            .map_err(|_| EngineError::InvalidCursor("invalid transactions cursor".to_string()))
    }
}

impl Engine {
    /// Lists recent transactions that affect a given flow.
    ///
    /// Returns `(transaction, signed_amount_minor)` where `signed_amount_minor`
    /// is the leg amount for that flow.
    pub async fn list_transactions_for_flow(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        user_id: &str,
        limit: u64,
        filter: &TransactionListFilter,
    ) -> ResultEngine<Vec<(Transaction, i64)>> {
        let (items, _next) = self
            .list_transactions_for_flow_page(vault_id, flow_id, user_id, limit, None, filter)
            .await?;
        Ok(items)
    }

    /// Lists recent transactions that affect a given flow, with cursor-based
    /// pagination.
    ///
    /// Pagination is newest → older by `(occurred_at DESC, transaction_id
    /// DESC)`.
    pub async fn list_transactions_for_flow_page(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        user_id: &str,
        limit: u64,
        cursor: Option<&str>,
        filter: &TransactionListFilter,
    ) -> ResultEngine<(Vec<(Transaction, i64)>, Option<String>)> {
        with_tx!(self, |db_tx| {
            self.require_flow_read(&db_tx, vault_id, flow_id, user_id)
                .await?;
            validate_list_filter(filter)?;

            let limit_plus_one = limit.saturating_add(1);
            let mut query = legs::Entity::find()
                .filter(legs::Column::TargetKind.eq(crate::legs::LegTargetKind::Flow))
                .filter(legs::Column::TargetId.eq(flow_id.to_string()))
                .find_also_related(transactions::Entity)
                .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
                .order_by_desc(transactions::Column::OccurredAt)
                .order_by_desc(transactions::Column::Id)
                .limit(limit_plus_one);

            if let Some(cursor) = cursor {
                let cursor = TransactionsCursor::decode(cursor)?;
                query = query.filter(
                    Condition::any()
                        .add(transactions::Column::OccurredAt.lt(cursor.occurred_at))
                        .add(
                            Condition::all()
                                .add(transactions::Column::OccurredAt.eq(cursor.occurred_at))
                                .add(transactions::Column::Id.lt(cursor.transaction_id)),
                        ),
                );
            }
            query = query.apply_tx_filters(filter);

            let rows: Vec<(legs::Model, Option<transactions::Model>)> = query.all(&db_tx).await?;
            let has_more = rows.len() > limit as usize;

            let mut out: Vec<(Transaction, i64)> =
                Vec::with_capacity(rows.len().min(limit as usize));
            for (leg_model, tx_model) in rows.into_iter().take(limit as usize) {
                let Some(tx_model) = tx_model else {
                    continue;
                };
                let tx = Transaction::try_from(tx_model)?;
                out.push((tx, leg_model.amount_minor));
            }

            let next_cursor = out.last().map(|(tx, _)| TransactionsCursor {
                occurred_at: tx.occurred_at,
                transaction_id: tx.id.to_string(),
            });
            let next_cursor = if has_more {
                next_cursor.map(|c| c.encode()).transpose()?
            } else {
                None
            };

            Ok((out, next_cursor))
        })
    }

    /// Lists recent transactions affecting the whole vault, with cursor-based
    /// pagination.
    ///
    /// Pagination is newest → older by `(occurred_at DESC, transaction_id
    /// DESC)`.
    pub async fn list_transactions_for_vault_page(
        &self,
        vault_id: &str,
        user_id: &str,
        limit: u64,
        cursor: Option<&str>,
        filter: &TransactionListFilter,
    ) -> ResultEngine<(Vec<Transaction>, Option<String>)> {
        with_tx!(self, |db_tx| {
            self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
            validate_list_filter(filter)?;

            let limit_plus_one = limit.saturating_add(1);
            let mut query = transactions::Entity::find()
                .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
                .order_by_desc(transactions::Column::OccurredAt)
                .order_by_desc(transactions::Column::Id)
                .limit(limit_plus_one);

            if let Some(cursor) = cursor {
                let cursor = TransactionsCursor::decode(cursor)?;
                query = query.filter(
                    Condition::any()
                        .add(transactions::Column::OccurredAt.lt(cursor.occurred_at))
                        .add(
                            Condition::all()
                                .add(transactions::Column::OccurredAt.eq(cursor.occurred_at))
                                .add(transactions::Column::Id.lt(cursor.transaction_id)),
                        ),
                );
            }
            query = query.apply_tx_filters(filter);

            let rows: Vec<transactions::Model> = query.all(&db_tx).await?;
            let has_more = rows.len() > limit as usize;

            let mut out: Vec<Transaction> = Vec::with_capacity(rows.len().min(limit as usize));
            for tx_model in rows.into_iter().take(limit as usize) {
                out.push(Transaction::try_from(tx_model)?);
            }

            let next_cursor = out.last().map(|tx| TransactionsCursor {
                occurred_at: tx.occurred_at,
                transaction_id: tx.id.to_string(),
            });
            let next_cursor = if has_more {
                next_cursor.map(|c| c.encode()).transpose()?
            } else {
                None
            };

            Ok((out, next_cursor))
        })
    }

    /// Lists recent transactions that affect a given wallet.
    ///
    /// Returns `(transaction, signed_amount_minor)` where `signed_amount_minor`
    /// is the leg amount for that wallet.
    pub async fn list_transactions_for_wallet(
        &self,
        vault_id: &str,
        wallet_id: Uuid,
        user_id: &str,
        limit: u64,
        filter: &TransactionListFilter,
    ) -> ResultEngine<Vec<(Transaction, i64)>> {
        let (items, _next) = self
            .list_transactions_for_wallet_page(vault_id, wallet_id, user_id, limit, None, filter)
            .await?;
        Ok(items)
    }

    /// Lists recent transactions that affect a given wallet, with cursor-based
    /// pagination.
    ///
    /// Pagination is newest → older by `(occurred_at DESC, transaction_id
    /// DESC)`.
    pub async fn list_transactions_for_wallet_page(
        &self,
        vault_id: &str,
        wallet_id: Uuid,
        user_id: &str,
        limit: u64,
        cursor: Option<&str>,
        filter: &TransactionListFilter,
    ) -> ResultEngine<(Vec<(Transaction, i64)>, Option<String>)> {
        with_tx!(self, |db_tx| {
            self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
            validate_list_filter(filter)?;

            let limit_plus_one = limit.saturating_add(1);
            let mut query = legs::Entity::find()
                .filter(legs::Column::TargetKind.eq(crate::legs::LegTargetKind::Wallet))
                .filter(legs::Column::TargetId.eq(wallet_id.to_string()))
                .find_also_related(transactions::Entity)
                .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
                .order_by_desc(transactions::Column::OccurredAt)
                .order_by_desc(transactions::Column::Id)
                .limit(limit_plus_one);

            if let Some(cursor) = cursor {
                let cursor = TransactionsCursor::decode(cursor)?;
                query = query.filter(
                    Condition::any()
                        .add(transactions::Column::OccurredAt.lt(cursor.occurred_at))
                        .add(
                            Condition::all()
                                .add(transactions::Column::OccurredAt.eq(cursor.occurred_at))
                                .add(transactions::Column::Id.lt(cursor.transaction_id)),
                        ),
                );
            }
            query = query.apply_tx_filters(filter);

            let rows: Vec<(legs::Model, Option<transactions::Model>)> = query.all(&db_tx).await?;
            let has_more = rows.len() > limit as usize;

            let mut out: Vec<(Transaction, i64)> =
                Vec::with_capacity(rows.len().min(limit as usize));
            for (leg_model, tx_model) in rows.into_iter().take(limit as usize) {
                let Some(tx_model) = tx_model else {
                    continue;
                };
                let tx = Transaction::try_from(tx_model)?;
                out.push((tx, leg_model.amount_minor));
            }

            let next_cursor = out.last().map(|(tx, _)| TransactionsCursor {
                occurred_at: tx.occurred_at,
                transaction_id: tx.id.to_string(),
            });
            let next_cursor = if has_more {
                next_cursor.map(|c| c.encode()).transpose()?
            } else {
                None
            };

            Ok((out, next_cursor))
        })
    }
}
