//! Membership management endpoints (owner-only).

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use uuid::Uuid;

use api_types::membership::{MemberUpsert, MemberView, MembersResponse, MembershipRole};

use crate::{ServerError, server::ServerState, user};

pub async fn list_vault_members(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path(vault_id): Path<String>,
) -> Result<Json<MembersResponse>, ServerError> {
    let members = state
        .engine
        .list_vault_members(&vault_id, &user.username)
        .await?
        .into_iter()
        .map(|(username, role)| MemberView {
            username,
            role: match role.as_str() {
                "owner" => MembershipRole::Owner,
                "editor" => MembershipRole::Editor,
                _ => MembershipRole::Viewer,
            },
        })
        .collect();

    Ok(Json(MembersResponse { members }))
}

pub async fn upsert_vault_member(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path(vault_id): Path<String>,
    Json(payload): Json<MemberUpsert>,
) -> Result<StatusCode, ServerError> {
    state
        .engine
        .upsert_vault_member(
            &vault_id,
            &payload.username,
            payload.role.as_str(),
            &user.username,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_vault_member(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path((vault_id, username)): Path<(String, String)>,
) -> Result<StatusCode, ServerError> {
    state
        .engine
        .remove_vault_member(&vault_id, &username, &user.username)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_flow_members(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path((vault_id, flow_id)): Path<(String, Uuid)>,
) -> Result<Json<MembersResponse>, ServerError> {
    let members = state
        .engine
        .list_flow_members(&vault_id, flow_id, &user.username)
        .await?
        .into_iter()
        .map(|(username, role)| MemberView {
            username,
            role: match role.as_str() {
                "owner" => MembershipRole::Owner,
                "editor" => MembershipRole::Editor,
                _ => MembershipRole::Viewer,
            },
        })
        .collect();

    Ok(Json(MembersResponse { members }))
}

pub async fn upsert_flow_member(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path((vault_id, flow_id)): Path<(String, Uuid)>,
    Json(payload): Json<MemberUpsert>,
) -> Result<StatusCode, ServerError> {
    state
        .engine
        .upsert_flow_member(
            &vault_id,
            flow_id,
            &payload.username,
            payload.role.as_str(),
            &user.username,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_flow_member(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path((vault_id, flow_id, username)): Path<(String, Uuid, String)>,
) -> Result<StatusCode, ServerError> {
    state
        .engine
        .remove_flow_member(&vault_id, flow_id, &username, &user.username)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
