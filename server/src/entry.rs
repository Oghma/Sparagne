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

pub async fn entry_new(
    _: Extension<user::Model>,
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
        )
        .await?;

    Ok(StatusCode::CREATED)
}
