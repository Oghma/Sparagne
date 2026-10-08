//! Vault routes: listing, push (which also creates a vault) and pull.

use std::collections::{HashMap, HashSet};

use axum::extract::State;
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
            let roles = state.db().memberships_of_user(user.id)?;
            let core = state.core();
            // One query for every vault's last seq, then one lookup per
            // membership: no scan of the whole vault table.
            let seqs: HashMap<Uuid, i64> = core.last_seqs()?.into_iter().collect();
            let mut out = Vec::with_capacity(roles.len());
            for (vault_id, role) in roles {
                let Some(vault) = core.vault(vault_id)? else {
                    continue;
                };
                out.push(VaultSummary {
                    id: vault.id,
                    name: vault.name,
                    currency: vault.currency,
                    owner: vault.owner,
                    role,
                    last_seq: seqs.get(&vault_id).copied().unwrap_or(0),
                });
            }
            // Names may repeat: the id, a v7 uuid minted with the vault, keeps
            // namesakes in the order they were created.
            out.sort_by_cached_key(|v| (v.name.to_lowercase(), v.id));
            Ok(out)
        })
        .await?;
    Ok(Json(summaries))
}

/// `POST /vaults/{vault_id}/push`.
///
/// A vault the server has never heard of is created by this very push, when
/// its first command is the `CreateVault` that mints it: the vault and the
/// caller's `owner` membership appear together. Any
/// other first command for an unknown vault is a blind 404, exactly like a
/// vault the caller is not a member of.
///
/// A command that puts a row or a template on somebody who is not a member
/// of the vault is refused on its own as `not_a_member`, and the rest of the
/// batch goes on (see [`stranger`]).
pub async fn push(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(vault_id): Path<Uuid>,
    Json(request): Json<PushRequest>,
) -> ApiResult<Json<PushResponse>> {
    let response = state
        .run(move |state| {
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
            let mut commands = request.commands;
            let mut results = Vec::with_capacity(commands.len());
            // Bound first: holding the membership lock across the match would
            // deadlock `claim`, which takes it again to write the membership.
            let role = state.db().membership(vault_id, user.id)?;
            match role {
                Some(role) if role.can_write() => {}
                Some(_) => return Err(ApiError::forbidden()),
                None => results.push(claim(state, vault_id, &user, &mut commands)?),
            }
            // Once per push, and after `claim`, so a vault created by this
            // very push already counts its owner.
            let members: HashSet<String> = state
                .db()
                .members_of_vault(vault_id)?
                .into_iter()
                .map(|member| member.username)
                .collect();

            let mut core = state.core();
            for envelope in commands {
                let command_id = envelope.id;
                // A command the log already holds answers its seq again, even
                // if the person it names has left the vault since.
                let stray = match stranger(&envelope, &members) {
                    Some(name) if core.receipt(command_id)?.is_none() => Some(name),
                    _ => None,
                };
                let outcome = match stray {
                    Some(name) => not_a_member(&name),
                    None => match core.execute(envelope) {
                        Ok(receipt) => PushOutcome::Applied {
                            seq: receipt.seq,
                            result_id: receipt.result_id,
                        },
                        Err(DomainError::Storage(detail)) => {
                            return Err(ApiError::internal(detail));
                        }
                        Err(err) => PushOutcome::Rejected {
                            code: err.code().to_string(),
                            message: err.to_string(),
                            detail: None,
                        },
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

/// The first person or owner `envelope` names who may not stand on a row of
/// the vault: anyone other than its author and the vault's members, whatever
/// their role. Usernames are compared as they are, lowercase, so "Elisa" is
/// nobody even when "elisa" is a member.
///
/// The check lives here and not in the core because membership is the
/// server's to know: a refused command never enters the log, so replaying a
/// log never depends on who was a member when it was written.
fn stranger(envelope: &CommandEnvelope, members: &HashSet<String>) -> Option<String> {
    envelope
        .command
        .named_people()
        .into_iter()
        .find(|name| *name != envelope.author && !members.contains(*name))
        .map(str::to_string)
}

/// A command refused like any other: the rest of the batch goes on, and the
/// app shows it among the rejected changes. The name goes in `detail` as
/// well as in the sentence, so the app can say who without parsing English.
fn not_a_member(name: &str) -> PushOutcome {
    PushOutcome::Rejected {
        code: "not_a_member".to_string(),
        message: format!("{name} is not a member of this vault"),
        detail: Some(name.to_string()),
    }
}

/// Creates a vault the server does not hold yet from the first command of a
/// push, together with the caller's `owner` membership, and takes that command
/// out of `commands` so the ordinary loop does not run it twice.
///
/// A first command that is not the vault's own `CreateVault`, and a vault that
/// already exists without a membership for the caller, are both `404`: the
/// caller may not learn whether the id is taken. The name plays no part: vault
/// names are labels, so one the caller already used is just another vault.
fn claim(
    state: &AppState,
    vault_id: Uuid,
    user: &CurrentUser,
    commands: &mut Vec<CommandEnvelope>,
) -> ApiResult<PushResult> {
    let first = commands.first().ok_or_else(ApiError::not_found)?;
    if !matches!(first.command, Command::CreateVault { .. }) || first.id != vault_id {
        return Err(ApiError::not_found());
    }
    let envelope = commands.remove(0);
    let command_id = envelope.id;
    let receipt = {
        let mut core = state.core();
        if core.last_seq(vault_id)? > 0 {
            // Somebody else's vault, one whose membership vanished, or one
            // its owner deleted (the log outlives the projection): the caller
            // is not a member, so it does not exist for them.
            return Err(ApiError::not_found());
        }
        core.execute(envelope)?
    };
    state
        .db()
        .insert_membership(vault_id, user.id, MemberRole::Owner, Utc::now().timestamp())?;
    Ok(PushResult {
        command_id,
        outcome: PushOutcome::Applied {
            seq: receipt.seq,
            result_id: receipt.result_id,
        },
    })
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
