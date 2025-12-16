//! Entries API endpoints
use api_types::entry::{EntryDelete, EntryNew};
use axum::{Extension, Json, extract::State, http::StatusCode};
use chrono::Utc;
use uuid::Uuid;

use crate::{ServerError, server::ServerState, user};

pub async fn entry_new(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<EntryNew>,
) -> Result<StatusCode, ServerError> {
    let mut engine = state.engine.write().await;
    let occurred_at = payload.date.with_timezone(&Utc);

    if payload.amount_minor > 0 {
        engine
            .income(
                &payload.vault_id,
                payload.amount_minor,
                payload.cash_flow_id,
                payload.wallet_id,
                Some(&payload.category),
                Some(&payload.note),
                &user.username,
                occurred_at,
            )
            .await?;
    } else {
        engine
            .expense(
                &payload.vault_id,
                payload.amount_minor.abs(),
                payload.cash_flow_id,
                payload.wallet_id,
                Some(&payload.category),
                Some(&payload.note),
                &user.username,
                occurred_at,
            )
            .await?;
    }

    Ok(StatusCode::CREATED)
}

pub async fn entry_delete(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<EntryDelete>,
) -> Result<StatusCode, ServerError> {
    let mut engine = state.engine.write().await;
    let transaction_id = Uuid::parse_str(&payload.entry_id)
        .map_err(|_| ServerError::Generic("invalid entry_id".to_string()))?;

    engine
        .void_transaction(
            &payload.vault_id,
            transaction_id,
            &user.username,
            Utc::now(),
        )
        .await?;

    Ok(StatusCode::OK)
}
