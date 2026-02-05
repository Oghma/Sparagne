//! Global search state.

use uuid::Uuid;

use super::selectable::SelectableList;

/// Global search overlay state for cross-screen search.
#[derive(Debug, Default)]
pub struct GlobalSearchState {
    pub active: bool,
    pub query: String,
    pub selected: usize,
    pub results: Vec<SearchResult>,
}

impl SelectableList for GlobalSearchState {
    fn visible_count(&self) -> usize {
        self.results.len()
    }

    fn selected(&self) -> usize {
        self.selected
    }

    fn set_selected(&mut self, idx: usize) {
        self.selected = idx;
    }
}

/// A search result that can be navigated to.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub kind: SearchResultKind,
    pub id: Uuid,
    pub label: String,
    pub detail: Option<String>,
}

/// The type of search result, determining navigation behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchResultKind {
    Transaction,
    Wallet,
    Flow,
    Category,
}
