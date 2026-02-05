use uuid::Uuid;

use crate::app::AppState;

use super::super::query::normalize_query;

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
