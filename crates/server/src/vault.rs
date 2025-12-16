//! Vault API endpoints

use api_types::vault::{Vault, VaultNew};
use axum::{Extension, Json, extract::State};

use crate::{ServerError, server::ServerState, user};

/// Handle requests for creating new `Vault`
pub async fn vault_new(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<VaultNew>,
) -> Result<Json<Vault>, ServerError> {
    let currency = payload.currency.unwrap_or(api_types::Currency::Eur);
    let vault_id = state
        .engine
        .new_vault(
            &payload.name,
            &user.username,
            Some(match currency {
                api_types::Currency::Eur => engine::Currency::Eur,
            }),
        )
        .await?;

    Ok(Json(Vault {
        id: Some(vault_id),
        name: Some(payload.name),
        currency: Some(currency),
    }))
}

/// Handle requests for listing user Vault
pub async fn get(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<Vault>,
) -> Result<Json<Vault>, ServerError> {
    if payload.id.is_none() && payload.name.is_none() {
        return Err(ServerError::Generic("id or name required".to_string()));
    }

    let vault = state
        .engine
        .vault_snapshot(payload.id.as_deref(), payload.name, &user.username)
        .await?;

    Ok(Json(Vault {
        id: Some(vault.id.clone()),
        name: Some(vault.name.clone()),
        currency: Some(match vault.currency {
            engine::Currency::Eur => api_types::Currency::Eur,
        }),
    }))
}
