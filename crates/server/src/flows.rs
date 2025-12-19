//! Flows API endpoints.

use api_types::flow::{FlowCreated, FlowMode, FlowNew, FlowUpdate};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use uuid::Uuid;

use crate::{ServerError, server::ServerState, user};

fn map_mode(mode: FlowMode) -> Result<(Option<i64>, Option<bool>), ServerError> {
    match mode {
        FlowMode::Unlimited => Ok((None, None)),
        FlowMode::NetCapped { cap_minor } => Ok((Some(cap_minor), None)),
        FlowMode::IncomeCapped { cap_minor } => Ok((Some(cap_minor), Some(true))),
    }
}

pub async fn flow_new(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<FlowNew>,
) -> Result<(StatusCode, Json<FlowCreated>), ServerError> {
    if payload.opening_balance_minor < 0 {
        return Err(ServerError::Generic(
            "opening_balance_minor must be >= 0".to_string(),
        ));
    }

    let name_trimmed = payload.name.trim();
    let (max_balance, income_bounded) = map_mode(payload.mode)?;

    let flow_id = state
        .engine
        .new_cash_flow(
            &payload.vault_id,
            name_trimmed,
            0,
            max_balance,
            income_bounded,
            &user.username,
        )
        .await?;

    if payload.opening_balance_minor > 0 {
        let vault = state
            .engine
            .vault_snapshot(Some(&payload.vault_id), None, &user.username)
            .await?;
        let unallocated_flow_id = vault
            .cash_flow
            .values()
            .find(|f| f.is_unallocated())
            .map(|f| f.id)
            .ok_or_else(|| ServerError::Generic("missing Unallocated flow".to_string()))?;

        state
            .engine
            .transfer_flow(engine::TransferFlowCmd {
                vault_id: payload.vault_id.clone(),
                amount_minor: payload.opening_balance_minor,
                from_flow_id: unallocated_flow_id,
                to_flow_id: flow_id,
                note: Some(format!("opening allocation for flow '{name_trimmed}'")),
                idempotency_key: None,
                occurred_at: payload.occurred_at.with_timezone(&Utc),
                user_id: user.username.clone(),
            })
            .await?;
    }

    Ok((StatusCode::CREATED, Json(FlowCreated { id: flow_id })))
}

pub async fn flow_update(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path(flow_id): Path<Uuid>,
    Json(payload): Json<FlowUpdate>,
) -> Result<StatusCode, ServerError> {
    if payload.name.is_none() && payload.archived.is_none() && payload.mode.is_none() {
        return Err(ServerError::Generic(
            "provide at least one of name, archived, or mode".to_string(),
        ));
    }

    if let Some(name) = payload.name.as_deref() {
        state
            .engine
            .rename_cash_flow(&payload.vault_id, flow_id, name, &user.username)
            .await?;
    }
    if let Some(archived) = payload.archived {
        state
            .engine
            .set_cash_flow_archived(&payload.vault_id, flow_id, archived, &user.username)
            .await?;
    }
    if let Some(mode) = payload.mode {
        let (max_balance, income_bounded) = map_mode(mode)?;
        state
            .engine
            .set_cash_flow_mode(
                &payload.vault_id,
                flow_id,
                max_balance,
                income_bounded.is_some_and(|v| v),
                &user.username,
            )
            .await?;
    }

    Ok(StatusCode::OK)
}
