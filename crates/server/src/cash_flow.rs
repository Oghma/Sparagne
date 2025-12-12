//! CashFlow API endpoints

use axum::{extract::State, Extension, Json};
use api_types::cash_flow::CashFlowGet;
use engine::CashFlow;

use crate::{server::ServerState, user, ServerError};

pub async fn get(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<CashFlowGet>,
) -> Result<Json<CashFlow>, ServerError> {
    let engine = state.engine.read().await;
    let flow = engine.cash_flow(&payload.name, &payload.vault_id, &user.username)?;

    Ok(Json(flow.clone()))
}
