//! CashFlow API endpoints

use api_types::cash_flow::CashFlowGet;
use axum::{Extension, Json, extract::State};
use engine::CashFlow;

use crate::{ServerError, server::ServerState, user};

pub async fn get(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<CashFlowGet>,
) -> Result<Json<CashFlow>, ServerError> {
    let engine = state.engine.read().await;
    let flow = engine.cash_flow(&payload.name, &payload.vault_id, &user.username)?;

    Ok(Json(flow.clone()))
}
