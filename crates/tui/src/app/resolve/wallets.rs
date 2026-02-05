use std::collections::HashMap;

use uuid::Uuid;

use api_types::vault::WalletView;

use crate::app::AppState;

use super::super::query::normalize_query;

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

/// Returns active wallets ordered by priority (default, recent, then
/// alphabetic).
fn ordered_active_wallets(state: &AppState) -> Vec<&WalletView> {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return Vec::new();
    };
    let ordered_ids = super::super::ordering::ordered_wallet_ids_from_state(state);
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
