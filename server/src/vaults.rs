//! Vault routes: listing, creation, push and pull (`docs/v2/SYNC.md` §3).

use axum::{extract::State, http::StatusCode};
use chrono::Utc;
use serde::Deserialize;
use sparagne_core::{
    Command, CommandEnvelope, DomainError,
    sync::{
        MemberRole, PullResponse, PushOutcome, PushRequest, PushResponse, PushResult, SyncRecord,
        VaultSummary,
    },
};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    db::ServerDb,
    error::{ApiError, ApiResult},
    extract::{Json, Path, Query},
    state::AppState,
};

const DEFAULT_PULL_LIMIT: i64 = 500;
const MAX_PULL_LIMIT: i64 = 1000;

/// `GET /vaults`: the vaults the caller is a member of.
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<VaultSummary>>> {
    let summaries = state
        .run(move |state| {
            let core = state.core();
            let roles: std::collections::HashMap<Uuid, MemberRole> = state
                .db()
                .memberships_of_user(user.id)?
                .into_iter()
                .collect();
            let mut out = Vec::with_capacity(roles.len());
            for vault in core.vaults()? {
                let Some(role) = roles.get(&vault.id).copied() else {
                    continue;
                };
                out.push(VaultSummary {
                    id: vault.id,
                    name: vault.name,
                    currency: vault.currency,
                    owner: vault.owner,
                    role,
                    last_seq: core.last_seq(vault.id)?,
                });
            }
            Ok(out)
        })
        .await?;
    Ok(Json(summaries))
}

/// `POST /vaults`: a `CreateVault` envelope, plus the owner membership.
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(envelope): Json<CommandEnvelope>,
) -> ApiResult<(StatusCode, Json<PushResult>)> {
    if envelope.author != user.username {
        return Err(ApiError::author_mismatch());
    }
    if !matches!(envelope.command, Command::CreateVault { .. }) {
        return Err(ApiError::invalid_request("expected a create_vault command"));
    }
    if envelope.vault_id != envelope.id {
        return Err(ApiError::invalid_request(
            "create_vault: vault_id must equal the command id",
        ));
    }
    let result = state
        .run(move |state| {
            let mut core = state.core();
            let command_id = envelope.id;
            let receipt = core.execute(envelope)?;
            // A command id already in the log gives back the original
            // receipt; make sure it really is this user's vault.
            let owned = core
                .vaults()?
                .into_iter()
                .any(|vault| vault.id == command_id && vault.owner == user.username);
            if !owned {
                return Err(ApiError::already_exists("command already used"));
            }
            state.db().insert_membership(
                command_id,
                user.id,
                MemberRole::Owner,
                Utc::now().timestamp(),
            )?;
            Ok(PushResult {
                command_id,
                outcome: PushOutcome::Applied {
                    seq: receipt.seq,
                    result_id: receipt.result_id,
                },
            })
        })
        .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// `POST /vaults/{vault_id}/push`.
pub async fn push(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(vault_id): Path<Uuid>,
    Json(request): Json<PushRequest>,
) -> ApiResult<Json<PushResponse>> {
    let response = state
        .run(move |state| {
            let role = membership(&state.db(), vault_id, user.id)?;
            if !role.can_write() {
                return Err(ApiError::forbidden());
            }
            // Nothing is applied before the whole batch is addressed to this
            // vault and signed by this user.
            for envelope in &request.commands {
                if envelope.vault_id != vault_id {
                    return Err(ApiError::invalid_request(
                        "a command is addressed to another vault",
                    ));
                }
                if envelope.author != user.username {
                    return Err(ApiError::author_mismatch());
                }
            }
            let mut core = state.core();
            let mut results = Vec::with_capacity(request.commands.len());
            for envelope in request.commands {
                let command_id = envelope.id;
                let outcome = match core.execute(envelope) {
                    Ok(receipt) => PushOutcome::Applied {
                        seq: receipt.seq,
                        result_id: receipt.result_id,
                    },
                    Err(DomainError::Storage(detail)) => return Err(ApiError::internal(detail)),
                    Err(err) => PushOutcome::Rejected {
                        code: err.code().to_string(),
                        message: err.to_string(),
                    },
                };
                results.push(PushResult {
                    command_id,
                    outcome,
                });
            }
            Ok(PushResponse {
                results,
                last_seq: core.last_seq(vault_id)?,
            })
        })
        .await?;
    Ok(Json(response))
}

/// Query of `GET /vaults/{vault_id}/pull`.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct PullParams {
    pub since: Option<i64>,
    pub limit: Option<i64>,
}

/// `GET /vaults/{vault_id}/pull?since=&limit=`.
pub async fn pull(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(vault_id): Path<Uuid>,
    Query(params): Query<PullParams>,
) -> ApiResult<Json<PullResponse>> {
    let since = params.since.unwrap_or(0).max(0);
    let limit = usize::try_from(
        params
            .limit
            .unwrap_or(DEFAULT_PULL_LIMIT)
            .clamp(1, MAX_PULL_LIMIT),
    )
    .unwrap_or(1);
    let response = state
        .run(move |state| {
            membership(&state.db(), vault_id, user.id)?;
            let core = state.core();
            let mut records = core.commands_since(vault_id, since)?;
            records.truncate(limit);
            Ok(PullResponse {
                commands: records
                    .into_iter()
                    .map(|record| SyncRecord {
                        envelope: record.envelope,
                        seq: record.seq,
                        result_id: record.result_id,
                        created_at: record.created_at,
                    })
                    .collect(),
                last_seq: core.last_seq(vault_id)?,
            })
        })
        .await?;
    Ok(Json(response))
}

/// The caller's role, or a blind 404 when there is no membership.
pub fn membership(db: &ServerDb, vault_id: Uuid, user_id: Uuid) -> ApiResult<MemberRole> {
    db.membership(vault_id, user_id)?
        .ok_or_else(ApiError::not_found)
}
