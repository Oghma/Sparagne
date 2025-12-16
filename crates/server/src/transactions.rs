//! Transactions API endpoints

use api_types::transaction::{
    TransactionKind as ApiKind, TransactionList, TransactionListResponse, TransactionView,
};
use axum::{Extension, Json, extract::State};
use chrono::FixedOffset;

use crate::{ServerError, server::ServerState, user};

pub async fn list(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<TransactionList>,
) -> Result<Json<TransactionListResponse>, ServerError> {
    let engine = state.engine.read().await;

    let limit = payload.limit.unwrap_or(50);
    let include_voided = payload.include_voided.unwrap_or(false);
    let include_transfers = payload.include_transfers.unwrap_or(false);

    let Some(flow_id) = payload.flow_id else {
        return Err(ServerError::Generic(
            "flow_id required (for now)".to_string(),
        ));
    };
    if payload.wallet_id.is_some() {
        return Err(ServerError::Generic(
            "wallet_id is not supported yet".to_string(),
        ));
    }

    let txs = engine
        .list_transactions_for_flow(
            &payload.vault_id,
            flow_id,
            &user.username,
            limit,
            include_voided,
            include_transfers,
        )
        .await?;

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

    Ok(Json(TransactionListResponse { transactions }))
}
