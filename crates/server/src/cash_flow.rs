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
    let flow = match (payload.id, payload.name.as_deref()) {
        (Some(id), _) => state
            .engine
            .cash_flow(id, &payload.vault_id, &user.username)
            .await?,
        (None, Some(name)) => state
            .engine
            .cash_flow_by_name(name, &payload.vault_id, &user.username)
            .await?,
        (None, None) => {
            return Err(ServerError::Generic(
                "cash flow id or name required".to_string(),
            ));
        }
    };

    Ok(Json(flow))
}
