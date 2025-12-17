//! Transactions API endpoints

use api_types::transaction::{
    ExpenseNew, IncomeNew, Refund, TransactionCreated, TransactionKind as ApiKind,
    TransactionList, TransactionListResponse, TransactionUpdate, TransactionView, TransactionVoid,
    TransferFlowNew, TransferWalletNew,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{FixedOffset, Utc};
use uuid::Uuid;

use crate::{ServerError, server::ServerState, user};

pub async fn list(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<TransactionList>,
) -> Result<Json<TransactionListResponse>, ServerError> {
    let engine = &state.engine;

    let limit = payload.limit.unwrap_or(50);
    let include_voided = payload.include_voided.unwrap_or(false);
    let include_transfers = payload.include_transfers.unwrap_or(false);
    let from = payload.from.map(|dt| dt.with_timezone(&Utc));
    let to = payload.to.map(|dt| dt.with_timezone(&Utc));
    let kinds = payload.kinds.map(|kinds| {
        kinds
            .into_iter()
            .map(|k| match k {
                ApiKind::Income => engine::TransactionKind::Income,
                ApiKind::Expense => engine::TransactionKind::Expense,
                ApiKind::TransferWallet => engine::TransactionKind::TransferWallet,
                ApiKind::TransferFlow => engine::TransactionKind::TransferFlow,
                ApiKind::Refund => engine::TransactionKind::Refund,
            })
            .collect::<Vec<_>>()
    });

    let filter = engine::TransactionListFilter {
        from,
        to,
        kinds,
        include_voided,
        include_transfers,
    };

    let (txs, next_cursor) = match (payload.flow_id, payload.wallet_id) {
        (Some(flow_id), None) => {
            engine
                .list_transactions_for_flow_page(
                    &payload.vault_id,
                    flow_id,
                    &user.username,
                    limit,
                    payload.cursor.as_deref(),
                    &filter,
                )
                .await?
        }
        (None, Some(wallet_id)) => {
            engine
                .list_transactions_for_wallet_page(
                    &payload.vault_id,
                    wallet_id,
                    &user.username,
                    limit,
                    payload.cursor.as_deref(),
                    &filter,
                )
                .await?
        }
        (None, None) => {
            return Err(ServerError::Generic(
                "either flow_id or wallet_id is required".to_string(),
            ));
        }
        (Some(_), Some(_)) => {
            return Err(ServerError::Generic(
                "provide only one of flow_id or wallet_id".to_string(),
            ));
        }
    };

    let utc = FixedOffset::east_opt(0).unwrap();
    let transactions = txs
        .into_iter()
        .map(|(tx, amount_minor)| TransactionView {
            id: tx.id,
            kind: match tx.kind {
                engine::TransactionKind::Income => ApiKind::Income,
                engine::TransactionKind::Expense => ApiKind::Expense,
                engine::TransactionKind::TransferWallet => ApiKind::TransferWallet,
                engine::TransactionKind::TransferFlow => ApiKind::TransferFlow,
                engine::TransactionKind::Refund => ApiKind::Refund,
            },
            occurred_at: tx.occurred_at.with_timezone(&utc),
            amount_minor,
            category: tx.category,
            note: tx.note,
            voided: tx.voided_at.is_some(),
        })
        .collect();

    Ok(Json(TransactionListResponse {
        transactions,
        next_cursor,
    }))
}

pub async fn income_new(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<IncomeNew>,
) -> Result<(StatusCode, Json<TransactionCreated>), ServerError> {
    let id = state
        .engine
        .income(
            &payload.vault_id,
            payload.amount_minor,
            payload.flow_id,
            payload.wallet_id,
            payload.category.as_deref(),
            payload.note.as_deref(),
            payload.idempotency_key.as_deref(),
            &user.username,
            payload.occurred_at.with_timezone(&Utc),
        )
        .await?;

    Ok((StatusCode::CREATED, Json(TransactionCreated { id })))
}

pub async fn expense_new(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<ExpenseNew>,
) -> Result<(StatusCode, Json<TransactionCreated>), ServerError> {
    let id = state
        .engine
        .expense(
            &payload.vault_id,
            payload.amount_minor,
            payload.flow_id,
            payload.wallet_id,
            payload.category.as_deref(),
            payload.note.as_deref(),
            payload.idempotency_key.as_deref(),
            &user.username,
            payload.occurred_at.with_timezone(&Utc),
        )
        .await?;

    Ok((StatusCode::CREATED, Json(TransactionCreated { id })))
}

pub async fn refund_new(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<Refund>,
) -> Result<(StatusCode, Json<TransactionCreated>), ServerError> {
    let id = state
        .engine
        .refund(
            &payload.vault_id,
            payload.amount_minor,
            payload.flow_id,
            payload.wallet_id,
            payload.category.as_deref(),
            payload.note.as_deref(),
            payload.idempotency_key.as_deref(),
            &user.username,
            payload.occurred_at.with_timezone(&Utc),
        )
        .await?;

    Ok((StatusCode::CREATED, Json(TransactionCreated { id })))
}

pub async fn transfer_wallet_new(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<TransferWalletNew>,
) -> Result<(StatusCode, Json<TransactionCreated>), ServerError> {
    let id = state
        .engine
        .transfer_wallet(
            &payload.vault_id,
            payload.amount_minor,
            payload.from_wallet_id,
            payload.to_wallet_id,
            payload.note.as_deref(),
            payload.idempotency_key.as_deref(),
            &user.username,
            payload.occurred_at.with_timezone(&Utc),
        )
        .await?;

    Ok((StatusCode::CREATED, Json(TransactionCreated { id })))
}

pub async fn transfer_flow_new(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<TransferFlowNew>,
) -> Result<(StatusCode, Json<TransactionCreated>), ServerError> {
    let id = state
        .engine
        .transfer_flow(
            &payload.vault_id,
            payload.amount_minor,
            payload.from_flow_id,
            payload.to_flow_id,
            payload.note.as_deref(),
            payload.idempotency_key.as_deref(),
            &user.username,
            payload.occurred_at.with_timezone(&Utc),
        )
        .await?;

    Ok((StatusCode::CREATED, Json(TransactionCreated { id })))
}

pub async fn update(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<TransactionUpdate>,
) -> Result<StatusCode, ServerError> {
    let occurred_at_utc = payload.occurred_at.map(|dt| dt.with_timezone(&Utc));
    state
        .engine
        .update_transaction(
            &payload.vault_id,
            id,
            &user.username,
            payload.amount_minor,
            payload.wallet_id,
            payload.flow_id,
            payload.from_wallet_id,
            payload.to_wallet_id,
            payload.from_flow_id,
            payload.to_flow_id,
            payload.category.as_deref(),
            payload.note.as_deref(),
            occurred_at_utc,
        )
        .await?;

    Ok(StatusCode::OK)
}

pub async fn void_tx(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<TransactionVoid>,
) -> Result<StatusCode, ServerError> {
    let voided_at = payload
        .voided_at
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    state
        .engine
        .void_transaction(&payload.vault_id, id, &user.username, voided_at)
        .await?;

    Ok(StatusCode::OK)
}
