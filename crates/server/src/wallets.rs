//! Wallets API endpoints.

use api_types::wallet::{WalletCreated, WalletNew, WalletUpdate};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use uuid::Uuid;

use crate::{ServerError, server::ServerState, user};

pub async fn wallet_new(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<WalletNew>,
) -> Result<(StatusCode, Json<WalletCreated>), ServerError> {
    let name_trimmed = payload.name.trim();
    let wallet_id = state
        .engine
        .new_wallet(&payload.vault_id, name_trimmed, 0, &user.username)
        .await?;

    if payload.opening_balance_minor != 0 {
        let vault = state
            .engine
            .vault_snapshot(Some(&payload.vault_id), None, &user.username)
            .await?;
        let unallocated_flow_id = vault
            .cash_flow
            .values()
            .find(|f| f.is_unallocated())
            .map(|f| f.id)
            .ok_or_else(|| ServerError::Generic("missing Unallocated flow".to_string()))?;

        let occurred_at = payload.occurred_at.with_timezone(&Utc);
        let opening_abs = payload.opening_balance_minor.abs();

        if payload.opening_balance_minor > 0 {
            state
                .engine
                .income(engine::IncomeCmd {
                    vault_id: payload.vault_id.clone(),
                    amount_minor: opening_abs,
                    flow_id: Some(unallocated_flow_id),
                    wallet_id: Some(wallet_id),
                    meta: engine::TxMeta {
                        category: Some("opening".to_string()),
                        note: Some(format!("opening balance for wallet '{name_trimmed}'")),
                        idempotency_key: None,
                        occurred_at,
                    },
                    user_id: user.username.clone(),
                })
                .await?;
        } else {
            state
                .engine
                .expense(engine::ExpenseCmd {
                    vault_id: payload.vault_id.clone(),
                    amount_minor: opening_abs,
                    flow_id: Some(unallocated_flow_id),
                    wallet_id: Some(wallet_id),
                    meta: engine::TxMeta {
                        category: Some("opening".to_string()),
                        note: Some(format!("opening balance for wallet '{name_trimmed}'")),
                        idempotency_key: None,
                        occurred_at,
                    },
                    user_id: user.username.clone(),
                })
                .await?;
        }
    }

    Ok((StatusCode::CREATED, Json(WalletCreated { id: wallet_id })))
}

pub async fn wallet_update(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path(wallet_id): Path<Uuid>,
    Json(payload): Json<WalletUpdate>,
) -> Result<StatusCode, ServerError> {
    if payload.name.is_none() && payload.archived.is_none() {
        return Err(ServerError::Generic(
            "provide at least one of name or archived".to_string(),
        ));
    }

    if let Some(name) = payload.name.as_deref() {
        state
            .engine
            .rename_wallet(&payload.vault_id, wallet_id, name, &user.username)
            .await?;
    }
    if let Some(archived) = payload.archived {
        state
            .engine
            .set_wallet_archived(&payload.vault_id, wallet_id, archived, &user.username)
            .await?;
    }

    Ok(StatusCode::OK)
}
