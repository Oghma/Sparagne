//! Vault API endpoints

use axum::{extract::State, Extension, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{server::ServerState, user, ServerError};

#[derive(Deserialize, Debug)]
pub struct VaultNew {
    name: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Vault {
    id: Option<Uuid>,
    name: Option<String>,
}

/// Handle requests for creating new `Vault`
pub async fn vault_new(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<VaultNew>,
) -> Result<Json<Vault>, ServerError> {
    //(StatusCode, Json<Vault>) {
    let mut engine = state.engine.write().await;
    match engine.new_vault(&payload.name, &user.username).await {
        Ok(uuid) => Ok(Json(Vault {
            id: uuid.to_string(),
            name: payload.name,
        })),
        Err(err) => Err(ServerError::Engine(err)),
    }
}

/// Handle requests for listing users Vaults
pub async fn vault_get(Extension(user): Extension<user::Model>, State(state): State<ServerState>) {
    let engine = state.engine.read().await;
}
