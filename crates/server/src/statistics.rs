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

    let engine = state.engine.read().await;
    let vault = engine.vault(payload.id.as_deref(), payload.name, &user.username)?;

    let result = vault
        .cash_flow
        .iter()
        .fold((0.0, 0.0, 0.0), |acc, (_, flow)| {
            let (income, expenses) = flow.entries.iter().fold((acc.0, acc.1), |acc, entry| {
                if entry.amount >= 0.0 {
                    (acc.0 + entry.amount, acc.1)
                } else {
                    (acc.0, acc.1 + entry.amount.abs())
                }
            });
            (income, expenses, acc.2 + flow.balance)
        });

    Ok(Json(Statistic {
        balance: result.2,
        total_income: result.0,
        total_expenses: result.1,
    }))
}
