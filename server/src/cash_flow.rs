//! CashFlow API endpoints

use axum::{extract::State, Extension, Json};
use engine::CashFlow;
use serde::{Deserialize, Serialize};

use crate::{server::ServerState, user, ServerError};

#[derive(Debug, Serialize, Deserialize)]
pub struct CashFlowGet {
    pub name: String,
    pub vault_id: String,
}

pub async fn get(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<CashFlowGet>,
) -> Result<Json<CashFlow>, ServerError> {
    let engine = state.engine.read().await;
    let flow = engine.cash_flow(&payload.name, &payload.vault_id, &user.username)?;

    Ok(Json(flow.clone()))
}
