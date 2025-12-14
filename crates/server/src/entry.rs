//! Entries API endpoints
use api_types::entry::{EntryDelete, EntryNew};
use axum::{Extension, Json, extract::State, http::StatusCode};
use chrono::Utc;

use crate::{ServerError, server::ServerState, user};

pub async fn entry_new(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<EntryNew>,
) -> Result<StatusCode, ServerError> {
    let mut engine = state.engine.write().await;

    engine
        .add_entry(
            payload.amount_minor,
            &payload.category,
            &payload.note,
            &payload.vault_id,
            payload.cash_flow_id,
            payload.wallet_id,
            &user.username,
            payload.date.with_timezone(&Utc),
        )
        .await?;

    Ok(StatusCode::CREATED)
}

pub async fn entry_delete(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<EntryDelete>,
) -> Result<StatusCode, ServerError> {
    let mut engine = state.engine.write().await;

    engine
        .delete_entry(
            &payload.vault_id,
            payload.cash_flow_id,
            payload.wallet_id,
            &payload.entry_id,
            &user.username,
        )
        .await?;

    Ok(StatusCode::OK)
}
