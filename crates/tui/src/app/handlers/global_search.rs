use super::super::*;

use crate::{error::Result, ui::keymap::AppAction};

impl App {
    pub(crate) async fn handle_global_search_action(&mut self, action: AppAction) -> Result<()> {
        match action {
            AppAction::Cancel => {
                self.close_global_search();
            }
            AppAction::Submit => {
                self.global_search_submit().await?;
            }
            AppAction::Up => {
                self.global_search_select_prev();
            }
            AppAction::Down => {
                self.global_search_select_next();
            }
            AppAction::Backspace => {
                self.global_search_backspace();
            }
            AppAction::Input(ch) => {
                self.global_search_input(ch);
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn open_global_search(&mut self) {
        self.state.global_search.active = true;
        self.state.global_search.query.clear();
        self.state.global_search.selected = 0;
        self.state.global_search.results.clear();
    }

    pub(crate) fn close_global_search(&mut self) {
        self.state.global_search.active = false;
        self.state.global_search.query.clear();
        self.state.global_search.selected = 0;
        self.state.global_search.results.clear();
    }

    pub(crate) fn global_search_input(&mut self, ch: char) {
        self.state.global_search.query.push(ch);
        self.state.global_search.selected = 0;
        self.update_global_search_results();
    }

    pub(crate) fn global_search_backspace(&mut self) {
        self.state.global_search.query.pop();
        self.state.global_search.selected = 0;
        self.update_global_search_results();
    }

    pub(crate) fn global_search_select_next(&mut self) {
        self.state.global_search.select_next();
    }

    pub(crate) fn global_search_select_prev(&mut self) {
        self.state.global_search.select_prev();
    }

    pub(crate) async fn global_search_submit(&mut self) -> Result<()> {
        let selected = self.state.global_search.selected;
        let results = &self.state.global_search.results;

        if selected >= results.len() {
            self.close_global_search();
            return Ok(());
        }

        let result = results[selected].clone();
        self.close_global_search();

        match result.kind {
            SearchResultKind::Transaction => {
                self.state.section = Section::Transactions;
                self.state.transactions.mode = TransactionsMode::List;

                // Try to find and select the transaction
                if let Some(idx) = self
                    .state
                    .transactions
                    .items
                    .iter()
                    .position(|t| t.id == result.id)
                {
                    self.state.transactions.selected = idx;
                    self.open_transaction_detail().await?;
                } else {
                    // Transaction not in current page - load transactions and search
                    self.load_transactions(true).await?;
                    if let Some(idx) = self
                        .state
                        .transactions
                        .items
                        .iter()
                        .position(|t| t.id == result.id)
                    {
                        self.state.transactions.selected = idx;
                        self.open_transaction_detail().await?;
                    }
                }
            }
            SearchResultKind::Wallet => {
                self.state.section = Section::Accounts;
                self.state.accounts_tab = AccountsTab::Sources;
                self.state.wallets.mode = EntityListMode::List;

                if let Some(snapshot) = &self.state.snapshot
                    && let Some(idx) = snapshot.wallets.iter().position(|w| w.id == result.id)
                {
                    self.state.wallets.selected = idx;
                    self.open_wallet_detail().await?;
                }
            }
            SearchResultKind::Flow => {
                self.state.section = Section::Accounts;
                self.state.accounts_tab = AccountsTab::Envelopes;
                self.state.flows.mode = EntityListMode::List;

                if let Some(snapshot) = &self.state.snapshot
                    && let Some(idx) = snapshot.flows.iter().position(|f| f.id == result.id)
                {
                    self.state.flows.selected = idx;
                    self.open_flow_detail().await?;
                }
            }
            SearchResultKind::Category => {
                self.state.section = Section::Settings;
                self.state.settings_tab = SettingsTab::Categories;
                self.state.categories.mode = CategoriesMode::List;

                if let Some(idx) = self
                    .state
                    .categories
                    .items
                    .iter()
                    .position(|c| c.id == result.id)
                {
                    self.state.categories.selected = idx;
                }
            }
        }

        Ok(())
    }

    fn update_global_search_results(&mut self) {
        let query = self.state.global_search.query.to_lowercase();
        let mut results = Vec::new();

        if query.is_empty() {
            self.state.global_search.results = results;
            return;
        }

        // Search transactions (notes, category)
        for tx in &self.state.transactions.items {
            let note_match = tx
                .note
                .as_ref()
                .map(|n| n.to_lowercase().contains(&query))
                .unwrap_or(false);
            let category_match = tx
                .category
                .as_ref()
                .map(|c| c.to_lowercase().contains(&query))
                .unwrap_or(false);

            if note_match || category_match {
                let label = tx.note.clone().unwrap_or_else(|| {
                    tx.category
                        .clone()
                        .unwrap_or_else(|| "Transaction".to_string())
                });
                let detail = tx.occurred_at.format("%Y-%m-%d").to_string();

                results.push(SearchResult {
                    kind: SearchResultKind::Transaction,
                    id: tx.id,
                    label,
                    detail: Some(detail),
                });

                if results.len() >= 20 {
                    break;
                }
            }
        }

        // Search wallets
        if let Some(snapshot) = &self.state.snapshot {
            for wallet in &snapshot.wallets {
                if wallet.name.to_lowercase().contains(&query) && !wallet.archived {
                    results.push(SearchResult {
                        kind: SearchResultKind::Wallet,
                        id: wallet.id,
                        label: wallet.name.clone(),
                        detail: None,
                    });
                }
            }

            // Search flows (envelopes)
            for flow in &snapshot.flows {
                if flow.name.to_lowercase().contains(&query) && !flow.archived {
                    results.push(SearchResult {
                        kind: SearchResultKind::Flow,
                        id: flow.id,
                        label: flow.name.clone(),
                        detail: None,
                    });
                }
            }
        }

        // Search categories
        for category in &self.state.categories.items {
            if category.name.to_lowercase().contains(&query) && !category.archived {
                results.push(SearchResult {
                    kind: SearchResultKind::Category,
                    id: category.id,
                    label: category.name.clone(),
                    detail: None,
                });
            }
        }

        // Limit results
        results.truncate(50);

        self.state.global_search.results = results;
    }
}
