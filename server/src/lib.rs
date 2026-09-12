//! Sparagne v2 sync server.
//!
//! Accounts, vault memberships and the per-vault command log, applied with
//! the same `sparagne_core` the app uses. Protocol in `docs/v2/SYNC.md`.

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod extract;
pub mod members;
pub mod state;
pub mod vaults;

use axum::{
    Router,
    routing::{delete, get, post},
};
use serde_json::{Value, json};
use tower_http::trace::TraceLayer;

pub use config::Config;
pub use error::{ApiError, ApiResult};
pub use state::AppState;

use crate::extract::Json;

/// The HTTP application.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/me", get(auth::me))
        .route("/vaults", get(vaults::list))
        .route("/vaults/{vault_id}/push", post(vaults::push))
        .route("/vaults/{vault_id}/pull", get(vaults::pull))
        .route(
            "/vaults/{vault_id}/members",
            get(members::list).put(members::set),
        )
        .route(
            "/vaults/{vault_id}/members/{username}",
            delete(members::remove),
        )
        .fallback(missing)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn missing() -> ApiError {
    ApiError::not_found()
}
