//! Vault API endpoints

use api_types::vault::{FlowView, Vault, VaultNew, VaultSnapshot, WalletView};
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

/// Fetch a vault snapshot for UI clients (bot/TUI).
pub async fn snapshot(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<Vault>,
) -> Result<Json<VaultSnapshot>, ServerError> {
    if payload.id.is_none() && payload.name.is_none() {
        return Err(ServerError::Generic("id or name required".to_string()));
    }

    let vault = state
        .engine
        .vault_snapshot(payload.id.as_deref(), payload.name, &user.username)
        .await?;

    let mut wallets = vault
        .wallet
        .into_iter()
        .map(|(id, wallet)| WalletView {
            id,
            name: wallet.name,
            balance_minor: wallet.balance,
            archived: wallet.archived,
        })
        .collect::<Vec<_>>();
    wallets.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let mut flows = vault
        .cash_flow
        .into_iter()
        .map(|(id, flow)| {
            let is_unallocated = flow.is_unallocated();
            FlowView {
                id,
                name: flow.name,
                balance_minor: flow.balance,
                archived: flow.archived,
                is_unallocated,
            }
        })
        .collect::<Vec<_>>();
    flows.sort_by(|a, b| match (a.is_unallocated, b.is_unallocated) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    let unallocated_flow_id = flows
        .iter()
        .find_map(|f| f.is_unallocated.then_some(f.id))
        .ok_or_else(|| ServerError::Generic("missing Unallocated flow".to_string()))?;

    Ok(Json(VaultSnapshot {
        id: vault.id,
        name: vault.name,
        currency: match vault.currency {
            engine::Currency::Eur => api_types::Currency::Eur,
        },
        wallets,
        flows,
        unallocated_flow_id,
    }))
}
