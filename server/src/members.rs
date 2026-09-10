//! Membership routes: who may read and write a vault (`docs/v2/SYNC.md` §3).
//!
//! The owner's row is written when the vault is created and can never be
//! changed or removed.

use axum::{extract::State, http::StatusCode};
use chrono::Utc;
use sparagne_core::sync::{MemberEntry, MemberRole, SetMemberRequest};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{ApiError, ApiResult},
    extract::{Json, Path},
    state::AppState,
    vaults::membership,
};

/// `GET /vaults/{vault_id}/members`.
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(vault_id): Path<Uuid>,
) -> ApiResult<Json<Vec<MemberEntry>>> {
    let members = state
        .run(move |state| {
            let db = state.db();
            membership(&db, vault_id, user.id)?;
            db.members_of_vault(vault_id)
        })
        .await?;
    Ok(Json(members))
}

/// `PUT /vaults/{vault_id}/members`: upsert an editor or a viewer.
pub async fn set(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(vault_id): Path<Uuid>,
    Json(request): Json<SetMemberRequest>,
) -> ApiResult<StatusCode> {
    if request.role == MemberRole::Owner {
        return Err(ApiError::invalid_request("the owner cannot be changed"));
    }
    state
        .run(move |state| {
            let db = state.db();
            require_owner(&db, vault_id, user.id)?;
            let Some(target) = db.user_by_username(&request.username)? else {
                return Err(ApiError::not_found());
            };
            if db.membership(vault_id, target.id)? == Some(MemberRole::Owner) {
                return Err(ApiError::forbidden());
            }
            db.set_membership(vault_id, target.id, request.role, Utc::now().timestamp())
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /vaults/{vault_id}/members/{username}`.
pub async fn remove(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((vault_id, username)): Path<(Uuid, String)>,
) -> ApiResult<StatusCode> {
    state
        .run(move |state| {
            let db = state.db();
            require_owner(&db, vault_id, user.id)?;
            let Some(target) = db.user_by_username(&username)? else {
                return Err(ApiError::not_found());
            };
            match db.membership(vault_id, target.id)? {
                None => return Err(ApiError::not_found()),
                Some(MemberRole::Owner) => return Err(ApiError::forbidden()),
                Some(_) => {}
            }
            db.remove_membership(vault_id, target.id).map(|_| ())
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn require_owner(db: &crate::db::ServerDb, vault_id: Uuid, user_id: Uuid) -> ApiResult<()> {
    if membership(db, vault_id, user_id)? == MemberRole::Owner {
        Ok(())
    } else {
        Err(ApiError::forbidden())
    }
}
