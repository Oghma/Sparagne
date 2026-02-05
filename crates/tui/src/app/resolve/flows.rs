use std::collections::HashMap;

use uuid::Uuid;

use api_types::vault::FlowView;

use crate::app::AppState;

use super::super::query::normalize_query;
use super::super::ordering::ordered_flow_ids_from_state;

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
