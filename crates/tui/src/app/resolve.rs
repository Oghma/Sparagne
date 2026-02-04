//! Entity resolution and lookup utilities.
//!
//! This module provides functions for resolving entity IDs to names, finding
//! matching entities by name query, extracting entities from transactions, and
//! determining default entities for operations.

use std::collections::HashMap;
use uuid::Uuid;

use api_types::{
    transaction::TransactionDetailResponse,
    vault::{FlowView, WalletView},
};

use crate::{
    app::AppState,
    error::AppError,
    text::{Locale, TextKey, t},
};

use super::query::normalize_query;
use super::ordering::ordered_flow_ids_from_state;

/// Extracts wallet and flow IDs from a transaction's legs.
///
/// Returns (wallet_id, flow_id) where either may be None if not present in the
/// transaction.
pub(crate) fn extract_wallet_flow(
    detail: &TransactionDetailResponse,
) -> (Option<Uuid>, Option<Uuid>) {
    let mut wallet_id = None;
    let mut flow_id = None;
    for leg in &detail.legs {
        match leg.target {
            api_types::transaction::LegTarget::Wallet { wallet_id: id } => {
                wallet_id = Some(id);
            }
            api_types::transaction::LegTarget::Flow { flow_id: id } => {
                flow_id = Some(id);
            }
        }
    }
    (wallet_id, flow_id)
}

/// Extracts source and destination wallet IDs from a wallet transfer
/// transaction.
///
/// Returns (from_wallet_id, to_wallet_id) or an error if the transfer
/// structure is invalid.
pub(crate) fn extract_wallet_transfer(
    detail: &TransactionDetailResponse,
    locale: Locale,
) -> Result<(Uuid, Uuid), AppError> {
    let mut from_wallet = None;
    let mut to_wallet = None;
    for leg in &detail.legs {
        if let api_types::transaction::LegTarget::Wallet { wallet_id } = leg.target {
            if leg.amount_minor < 0 {
                from_wallet = Some(wallet_id);
            } else if leg.amount_minor > 0 {
                to_wallet = Some(wallet_id);
            }
        }
    }
    match (from_wallet, to_wallet) {
        (Some(from), Some(to)) => Ok((from, to)),
        _ => Err(AppError::Terminal(
            t(locale, TextKey::StateCannotDetermineWalletTransfer).to_string(),
        )),
    }
}

/// Extracts source and destination flow IDs from a flow transfer transaction.
///
/// Returns (from_flow_id, to_flow_id) or an error if the transfer structure is
/// invalid.
pub(crate) fn extract_flow_transfer(
    detail: &TransactionDetailResponse,
    locale: Locale,
) -> Result<(Uuid, Uuid), AppError> {
    let mut from_flow = None;
    let mut to_flow = None;
    for leg in &detail.legs {
        if let api_types::transaction::LegTarget::Flow { flow_id } = leg.target {
            if leg.amount_minor < 0 {
                from_flow = Some(flow_id);
            } else if leg.amount_minor > 0 {
                to_flow = Some(flow_id);
            }
        }
    }
    match (from_flow, to_flow) {
        (Some(from), Some(to)) => Ok((from, to)),
        _ => Err(AppError::Terminal(
            t(locale, TextKey::StateCannotDetermineFlowTransfer).to_string(),
        )),
    }
}

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

/// Resolves a flow name query to a flow ID and name.
///
/// Returns (flow_id, flow_name, is_exact_match) where is_exact_match
/// indicates if the query exactly matched the flow name (case-insensitive).
///
/// Priority: exact match > prefix match > contains match.
pub(crate) fn resolve_flow_name(state: &AppState, query: &str) -> Option<(Uuid, String, bool)> {
    let query = normalize_query(query);
    if query.is_empty() {
        return None;
    }
    let ordered = ordered_active_flows(state);
    let (exact, prefix, contains) = flow_name_buckets(&ordered, query.as_str());

    exact
        .first()
        .map(|flow| (flow.id, flow.name.clone(), true))
        .or_else(|| {
            prefix
                .first()
                .map(|flow| (flow.id, flow.name.clone(), false))
        })
        .or_else(|| {
            contains
                .first()
                .map(|flow| (flow.id, flow.name.clone(), false))
        })
}

/// Returns all matching categories for a query, ordered by match quality (exact
/// > prefix > contains).
pub(crate) fn resolve_category_matches(state: &AppState, query: &str) -> Vec<(Uuid, String)> {
    let query = normalize_query(query);
    if query.is_empty() {
        return Vec::new();
    }

    let mut exact = Vec::new();
    let mut prefix = Vec::new();
    let mut contains = Vec::new();

    for category in &state.categories.items {
        if category.archived {
            continue;
        }
        let name = category.name.to_lowercase();
        if name == query {
            exact.push((category.id, category.name.clone()));
        } else if name.starts_with(&query) {
            prefix.push((category.id, category.name.clone()));
        } else if name.contains(&query) {
            contains.push((category.id, category.name.clone()));
        }
    }

    let mut results = Vec::new();
    results.extend(exact);
    results.extend(prefix);
    results.extend(contains);
    results
}

/// Returns all matching flows for a query, ordered by match quality (exact >
/// prefix > contains).
pub(crate) fn resolve_flow_matches(state: &AppState, query: &str) -> Vec<(Uuid, String)> {
    let query = normalize_query(query);
    if query.is_empty() {
        return Vec::new();
    }
    let ordered = ordered_active_flows(state);
    let (exact, prefix, contains) = flow_name_buckets(&ordered, query.as_str());

    let mut results = Vec::new();
    for flow in exact {
        results.push((flow.id, flow.name.clone()));
    }
    for flow in prefix {
        results.push((flow.id, flow.name.clone()));
    }
    for flow in contains {
        results.push((flow.id, flow.name.clone()));
    }
    results
}

/// Resolves a wallet name query to a wallet ID and name.
///
/// Returns (wallet_id, wallet_name, is_exact_match) where is_exact_match
/// indicates if the query exactly matched the wallet name (case-insensitive).
///
/// Priority: exact match > prefix match > contains match.
pub(crate) fn resolve_wallet_name(state: &AppState, query: &str) -> Option<(Uuid, String, bool)> {
    let query = normalize_query(query);
    if query.is_empty() {
        return None;
    }
    let ordered = ordered_active_wallets(state);
    let (exact, prefix, contains) = wallet_name_buckets(&ordered, query.as_str());

    exact
        .first()
        .map(|wallet| (wallet.id, wallet.name.clone(), true))
        .or_else(|| {
            prefix
                .first()
                .map(|wallet| (wallet.id, wallet.name.clone(), false))
        })
        .or_else(|| {
            contains
                .first()
                .map(|wallet| (wallet.id, wallet.name.clone(), false))
        })
}

/// Returns all matching wallets for a query, ordered by match quality (exact >
/// prefix > contains).
pub(crate) fn resolve_wallet_matches(state: &AppState, query: &str) -> Vec<(Uuid, String)> {
    let query = normalize_query(query);
    if query.is_empty() {
        return Vec::new();
    }
    let ordered = ordered_active_wallets(state);
    let (exact, prefix, contains) = wallet_name_buckets(&ordered, query.as_str());

    let mut results = Vec::new();
    for wallet in exact {
        results.push((wallet.id, wallet.name.clone()));
    }
    for wallet in prefix {
        results.push((wallet.id, wallet.name.clone()));
    }
    for wallet in contains {
        results.push((wallet.id, wallet.name.clone()));
    }
    results
}

/// Returns active flows ordered by priority (default, recent, then alphabetic).
pub(crate) fn ordered_active_flows(state: &AppState) -> Vec<&FlowView> {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return Vec::new();
    };
    let ordered_ids = ordered_flow_ids_from_state(state);
    let mut by_id: HashMap<Uuid, &FlowView> = HashMap::with_capacity(snapshot.flows.len());
    for flow in snapshot.flows.iter().filter(|flow| !flow.archived) {
        by_id.insert(flow.id, flow);
    }

    let mut ordered = Vec::with_capacity(by_id.len());
    for id in ordered_ids {
        if let Some(flow) = by_id.get(&id) {
            ordered.push(*flow);
        }
    }
    ordered
}

/// Categorizes flows into exact, prefix, and contains match buckets.
fn flow_name_buckets<'a>(
    flows: &'a [&FlowView],
    query: &str,
) -> (Vec<&'a FlowView>, Vec<&'a FlowView>, Vec<&'a FlowView>) {
    let mut exact = Vec::new();
    let mut prefix = Vec::new();
    let mut contains = Vec::new();

    for flow in flows {
        let name = flow.name.to_lowercase();
        if name == query {
            exact.push(*flow);
        } else if name.starts_with(query) {
            prefix.push(*flow);
        } else if name.contains(query) {
            contains.push(*flow);
        }
    }

    (exact, prefix, contains)
}

/// Returns active wallets ordered by priority (default, recent, then
/// alphabetic).
fn ordered_active_wallets(state: &AppState) -> Vec<&WalletView> {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return Vec::new();
    };
    let ordered_ids = super::ordering::ordered_wallet_ids_from_state(state);
    let mut by_id: HashMap<Uuid, &WalletView> = HashMap::with_capacity(snapshot.wallets.len());
    for wallet in snapshot.wallets.iter().filter(|wallet| !wallet.archived) {
        by_id.insert(wallet.id, wallet);
    }

    let mut ordered = Vec::with_capacity(by_id.len());
    for id in ordered_ids {
        if let Some(wallet) = by_id.get(&id) {
            ordered.push(*wallet);
        }
    }
    ordered
}

/// Categorizes wallets into exact, prefix, and contains match buckets.
fn wallet_name_buckets<'a>(
    wallets: &'a [&WalletView],
    query: &str,
) -> (
    Vec<&'a WalletView>,
    Vec<&'a WalletView>,
    Vec<&'a WalletView>,
) {
    let mut exact = Vec::new();
    let mut prefix = Vec::new();
    let mut contains = Vec::new();

    for wallet in wallets {
        let name = wallet.name.to_lowercase();
        if name == query {
            exact.push(*wallet);
        } else if name.starts_with(query) {
            prefix.push(*wallet);
        } else if name.contains(query) {
            contains.push(*wallet);
        }
    }

    (exact, prefix, contains)
}
