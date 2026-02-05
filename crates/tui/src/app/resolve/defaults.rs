use uuid::Uuid;

use crate::{
    app::AppState,
    text::{Locale, TextKey, t},
};

/// Returns default wallet and flow for operations.
///
/// Priority order for wallet:
/// 1. Scope wallet (if set in transactions state)
/// 2. Default wallet (if set globally)
/// 3. Most recent wallet
/// 4. First active wallet
///
/// Priority order for flow:
/// 1. Scope flow (if set in transactions state)
/// 2. Default flow (if set globally)
/// 3. Most recent flow
/// 4. Last used flow (last_flow_id)
/// 5. Unallocated flow
///
/// Returns (wallet_id, flow_id, wallet_name, flow_name) or an error if no
/// suitable defaults can be found.
pub(crate) fn default_wallet_flow(
    state: &AppState,
    locale: Locale,
) -> Result<(Uuid, Uuid, String, String), String> {
    let snapshot = state
        .snapshot
        .as_ref()
        .ok_or_else(|| t(locale, TextKey::StateSnapshotUnavailable).to_string())?;

    let wallet = state
        .transactions
        .scope_wallet_id
        .and_then(|wallet_id| {
            snapshot
                .wallets
                .iter()
                .find(|wallet| wallet.id == wallet_id && !wallet.archived)
        })
        .or_else(|| {
            state.default_wallet_id.and_then(|wallet_id| {
                snapshot
                    .wallets
                    .iter()
                    .find(|wallet| wallet.id == wallet_id && !wallet.archived)
            })
        })
        .or_else(|| {
            state
                .transactions
                .recent_wallet_ids
                .iter()
                .find_map(|recent_id| {
                    snapshot
                        .wallets
                        .iter()
                        .find(|wallet| wallet.id == *recent_id && !wallet.archived)
                })
        })
        .or_else(|| snapshot.wallets.iter().find(|wallet| !wallet.archived))
        .ok_or_else(|| t(locale, TextKey::StateNoWalletAvailable).to_string())?;
    let flow = state
        .transactions
        .scope_flow_id
        .and_then(|flow_id| {
            snapshot
                .flows
                .iter()
                .find(|flow| flow.id == flow_id && !flow.archived)
        })
        .or_else(|| {
            state.default_flow_id.and_then(|flow_id| {
                snapshot
                    .flows
                    .iter()
                    .find(|flow| flow.id == flow_id && !flow.archived)
            })
        })
        .or_else(|| {
            state
                .transactions
                .recent_flow_ids
                .iter()
                .find_map(|recent_id| {
                    snapshot
                        .flows
                        .iter()
                        .find(|flow| flow.id == *recent_id && !flow.archived)
                })
        })
        .or_else(|| {
            state.last_flow_id.and_then(|last_id| {
                snapshot
                    .flows
                    .iter()
                    .find(|flow| flow.id == last_id && !flow.archived)
            })
        })
        .or_else(|| snapshot.flows.iter().find(|flow| flow.is_unallocated))
        .ok_or_else(|| t(locale, TextKey::StateUnallocatedMissing).to_string())?;

    Ok((wallet.id, flow.id, wallet.name.clone(), flow.name.clone()))
}
