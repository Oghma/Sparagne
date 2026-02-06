//! Sorting and ordering utilities.
//!
//! This module provides functions for ordering entities (wallets, flows),
//! computing visible indices for filtered lists, and managing MRU (most
//! recently used) priorities.

use uuid::Uuid;

use api_types::vault::FlowView;

use crate::app::AppState;

use super::query::normalize_query;
use super::resolve::ordered_active_flows;

/// Returns ordered wallet IDs, prioritizing default and recent wallets.
///
/// Order: default wallet → recent wallets → remaining active wallets.
pub(crate) fn ordered_wallet_ids_from_state(state: &AppState) -> Vec<Uuid> {
    let active_ids = state
        .snapshot
        .as_ref()
        .map(|snap| {
            snap.wallets
                .iter()
                .filter(|wallet| !wallet.archived)
                .map(|wallet| wallet.id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut priority = Vec::new();
    if let Some(default_id) = state.default_wallet_id {
        priority.push(default_id);
    }
    priority.extend(state.transactions.recent_wallet_ids.iter().copied());
    ordered_ids(active_ids, &priority)
}

/// Returns ordered flow IDs, prioritizing default and recent flows.
///
/// Order: default flow → recent flows → remaining active flows.
pub(crate) fn ordered_flow_ids_from_state(state: &AppState) -> Vec<Uuid> {
    let active_ids = state
        .snapshot
        .as_ref()
        .map(|snap| {
            snap.flows
                .iter()
                .filter(|flow| !flow.archived)
                .map(|flow| flow.id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut priority = Vec::new();
    if let Some(default_id) = state.default_flow_id {
        priority.push(default_id);
    }
    priority.extend(state.transactions.recent_flow_ids.iter().copied());
    ordered_ids(active_ids, &priority)
}

/// Returns a list of IDs ordered by priority (MRU), followed by remaining
/// active IDs.
pub(crate) fn ordered_ids(active: Vec<Uuid>, recents: &[Uuid]) -> Vec<Uuid> {
    let mut ordered = Vec::with_capacity(active.len());
    for recent in recents {
        if active.contains(recent) && !ordered.contains(recent) {
            ordered.push(*recent);
        }
    }
    for id in active {
        if !ordered.contains(&id) {
            ordered.push(id);
        }
    }
    ordered
}

/// Pushes a recent ID to a list if not already present and limit not reached.
pub(crate) fn push_recent_id(target: &mut Vec<Uuid>, value: Uuid, limit: usize) {
    if target.contains(&value) {
        return;
    }
    if target.len() >= limit {
        return;
    }
    target.push(value);
}

/// Returns indices of transactions visible after filtering and hiding pending
/// deletes.
pub(crate) fn transactions_visible_indices(state: &AppState) -> Vec<usize> {
    let query = normalize_query(state.transactions.search.query.as_str());
    let hidden_ids = &state.transactions.pending_delete_ids;
    if query.is_empty() {
        return state
            .transactions
            .items
            .iter()
            .enumerate()
            .filter_map(|(idx, tx)| {
                if hidden_ids.contains(&tx.id) {
                    None
                } else {
                    Some(idx)
                }
            })
            .collect();
    }

    state
        .transactions
        .items
        .iter()
        .enumerate()
        .filter_map(|(idx, tx)| {
            if hidden_ids.contains(&tx.id) {
                return None;
            }
            if super::query::transaction_matches_query(tx, query.as_str()) {
                Some(idx)
            } else {
                None
            }
        })
        .collect()
}

/// Returns indices of transactions for the home feed (excluding pending
/// deletes).
pub(crate) fn home_feed_indices(state: &AppState) -> Vec<usize> {
    let hidden_ids = &state.transactions.pending_delete_ids;
    state
        .transactions
        .items
        .iter()
        .enumerate()
        .filter_map(|(idx, tx)| {
            if hidden_ids.contains(&tx.id) {
                None
            } else {
                Some(idx)
            }
        })
        .collect()
}

/// Returns indices of wallets visible after filtering by search query and
/// archived status.
pub(crate) fn wallets_visible_indices(state: &AppState) -> Vec<usize> {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return Vec::new();
    };
    let query = normalize_query(state.wallets.search.query.as_str());
    let show_archived = state.wallets.show_archived;

    snapshot
        .wallets
        .iter()
        .enumerate()
        .filter_map(|(idx, wallet)| {
            // Filter by archived status
            if !show_archived && wallet.archived {
                return None;
            }
            // Filter by search query
            if !query.is_empty() && !wallet.name.to_lowercase().contains(query.as_str()) {
                return None;
            }
            Some(idx)
        })
        .collect()
}

/// Returns indices of flows visible after filtering by search query and
/// archived status.
pub(crate) fn flows_visible_indices(state: &AppState) -> Vec<usize> {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return Vec::new();
    };
    let query = normalize_query(state.flows.search.query.as_str());
    let show_archived = state.flows.show_archived;

    snapshot
        .flows
        .iter()
        .enumerate()
        .filter_map(|(idx, flow)| {
            // Filter by archived status
            if !show_archived && flow.archived {
                return None;
            }
            // Filter by search query
            if !query.is_empty() && !flow.name.to_lowercase().contains(query.as_str()) {
                return None;
            }
            Some(idx)
        })
        .collect()
}

/// Returns flow name suggestions based on query match quality, up to limit.
///
/// Suggestions are ordered by match quality (exact > prefix > contains).
pub(crate) fn flow_name_suggestions(state: &AppState, query: &str, limit: usize) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let query = normalize_query(query);
    if query.is_empty() {
        return Vec::new();
    }
    let ordered = ordered_active_flows(state);
    let (exact, prefix, contains) = flow_name_buckets(&ordered, query.as_str());
    let source = if !exact.is_empty() {
        exact
    } else if !prefix.is_empty() {
        prefix
    } else {
        contains
    };
    source
        .into_iter()
        .take(limit)
        .map(|flow| flow.name.clone())
        .collect()
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

/// Home feed item: either a flow alert or a transaction.
#[derive(Debug, Clone)]
pub(crate) enum HomeFeedItem {
    FlowAlert(FlowAlertItem),
    Transaction { index: usize },
}

/// Flow alert metadata for home feed.
#[derive(Debug, Clone)]
pub(crate) struct FlowAlertItem {
    pub flow_id: Uuid,
    pub name: String,
    pub balance_minor: i64,
    pub threshold_minor: i64,
    pub severity: FlowAlertSeverity,
}

/// Alert severity: critical (negative balance) or warning (low balance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlowAlertSeverity {
    Warning,
    Critical,
}

/// Returns home feed items: flow alerts followed by transactions.
pub(crate) fn home_feed_items(state: &AppState) -> Vec<HomeFeedItem> {
    let mut items = flow_alert_items(state);
    items.extend(
        home_feed_indices(state)
            .into_iter()
            .map(|index| HomeFeedItem::Transaction { index }),
    );
    items
}

/// Generates flow alert items for flows with negative or low balances.
fn flow_alert_items(state: &AppState) -> Vec<HomeFeedItem> {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return Vec::new();
    };
    let low_balance_minor = state.home_low_balance_minor.max(0);

    let mut alerts: Vec<FlowAlertItem> = snapshot
        .flows
        .iter()
        .filter(|flow| !flow.archived && !flow.is_unallocated)
        .filter_map(|flow| {
            let balance = flow.balance_minor;
            let (severity, threshold_minor) = if balance < 0 {
                (FlowAlertSeverity::Critical, 0)
            } else if low_balance_minor > 0 && balance <= low_balance_minor {
                (FlowAlertSeverity::Warning, low_balance_minor)
            } else {
                return None;
            };

            Some(FlowAlertItem {
                flow_id: flow.id,
                name: flow.name.clone(),
                balance_minor: balance,
                threshold_minor,
                severity,
            })
        })
        .collect();

    alerts.sort_by(|a, b| {
        severity_rank(a.severity)
            .cmp(&severity_rank(b.severity))
            .then_with(|| a.balance_minor.cmp(&b.balance_minor))
            .then_with(|| a.name.cmp(&b.name))
    });

    alerts.into_iter().map(HomeFeedItem::FlowAlert).collect()
}

/// Returns numeric rank for severity (lower = more severe).
fn severity_rank(severity: FlowAlertSeverity) -> u8 {
    match severity {
        FlowAlertSeverity::Critical => 0,
        FlowAlertSeverity::Warning => 1,
    }
}
