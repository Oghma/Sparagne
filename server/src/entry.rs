//! Entries API endpoints
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{server::ServerState, user, ServerError};

#[derive(Deserialize, Serialize)]
pub struct CreateEntry {
    vault_id: Uuid,
    amount: f64,
    category: String,
    note: String,
}

pub async fn entry_new(
    _: Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<CreateEntry>,
) -> Result<StatusCode, ServerError> {
    let mut engine = state.engine.write().await;

    engine
        .add_entry(
            payload.amount,
            &payload.category,
            &payload.note,
            &payload.vault_id,
            None,
            None,
        )
        .await?;

    Ok(StatusCode::CREATED)
}
