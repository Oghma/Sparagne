use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;

use crate::server::SharedState;

#[derive(Deserialize)]
pub struct CreateEntry {
    flow_name: String,
    amount: f64,
    category: String,
    note: String,
}

pub async fn entry_new(State(state): SharedState, Json(payload): Json<CreateEntry>) -> StatusCode {
    if let Ok(_) = state.write().await.add_flow_entry(
        &payload.flow_name,
        payload.amount,
        payload.category,
        payload.note,
    ) {
        StatusCode::ACCEPTED
    } else {
        StatusCode::NOT_IMPLEMENTED
    }
}
