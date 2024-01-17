//! Vault API endpoints

use axum::{extract::State, Extension, Json};
use serde::{Deserialize, Serialize};

use crate::{server::ServerState, user, ServerError};

#[derive(Deserialize, Debug)]
pub struct VaultNew {
    name: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Vault {
    pub id: Option<String>,
    pub name: Option<String>,
}

/// Handle requests for creating new `Vault`
pub async fn vault_new(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<VaultNew>,
) -> Result<Json<Vault>, ServerError> {
    let mut engine = state.engine.write().await;
    let vault_id = engine.new_vault(&payload.name, &user.username).await?;

    Ok(Json(Vault {
        id: Some(vault_id),
        name: Some(payload.name),
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

    let engine = state.engine.read().await;
    let vault = engine.vault(payload.id.as_deref(), payload.name, &user.username)?;

    Ok(Json(Vault {
        id: Some(vault.id.clone()),
        name: Some(vault.name.clone()),
    }))
}
