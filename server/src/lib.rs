//! Sparagne v2 sync server.
//!
//! Accounts, vault memberships and the per-vault command log, applied with
//! the same `sparagne_core` the app uses. Protocol in `docs/v2/SYNC.md`.

use axum::{Json, Router, routing::get};
use serde_json::{Value, json};

/// The HTTP application.
pub fn router() -> Router {
    Router::new().route("/health", get(health))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
