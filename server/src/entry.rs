//! Entries API endpoints
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};

use crate::{server::ServerState, user, ServerError};

#[derive(Deserialize, Serialize)]
pub struct EntryNew {
    pub vault_id: String,
    pub amount: f64,
    pub category: String,
    pub note: String,
    pub cash_flow: String,
}

#[derive(Deserialize, Serialize)]
pub struct EntryDelete {
    pub vault_id: String,
    pub entry_id: String,
    pub cash_flow: Option<String>,
    pub wallet: Option<String>,
}

pub async fn entry_new(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<EntryNew>,
) -> Result<StatusCode, ServerError> {
    let mut engine = state.engine.write().await;

    engine
        .add_entry(
            payload.amount,
            &payload.category,
            &payload.note,
            &payload.vault_id,
            Some(&payload.cash_flow),
            None,
            &user.username,
        )
        .await?;

    Ok(StatusCode::CREATED)
}

pub async fn entry_delete(
    _: Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<EntryDelete>,
) -> Result<StatusCode, ServerError> {
    let mut engine = state.engine.write().await;

    engine
        .delete_entry(
            &payload.vault_id,
            payload.cash_flow.as_deref(),
            payload.wallet.as_deref(),
            &payload.entry_id,
        )
        .await?;

    Ok(StatusCode::OK)
}
