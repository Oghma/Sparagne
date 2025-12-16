//! Statistics API edpoints

use api_types::{stats::Statistic, vault::Vault};
use axum::{Extension, Json, extract::State};

use crate::{ServerError, server::ServerState, user};

/// Handle requests for user statistics
pub async fn get_stats(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<Vault>,
) -> Result<Json<Statistic>, ServerError> {
    if payload.id.is_none() && payload.name.is_none() {
        return Err(ServerError::Generic("id or name required".to_string()));
    }

    let vault = state
        .engine
        .vault_snapshot(payload.id.as_deref(), payload.name, &user.username)
        .await?;
    let (currency, balance_minor, total_income_minor, total_expenses_minor) = state
        .engine
        .vault_statistics(&vault.id, &user.username, false)
        .await?;

    Ok(Json(Statistic {
        currency: match currency {
            engine::Currency::Eur => api_types::Currency::Eur,
        },
        balance_minor,
        total_income_minor,
        total_expenses_minor,
    }))
}
