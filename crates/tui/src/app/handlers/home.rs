use super::super::*;

use crate::error::Result;

impl App {
    pub(crate) fn home_feed_select_next(&mut self) {
        let len = home_feed_items(&self.state).len();
        if len == 0 {
            self.state.home_feed_selected = 0;
            return;
        }
        self.state.home_feed_selected = (self.state.home_feed_selected + 1).min(len - 1);
    }

    pub(crate) fn home_feed_select_prev(&mut self) {
        let len = home_feed_items(&self.state).len();
        if len == 0 {
            self.state.home_feed_selected = 0;
            return;
        }
        self.state.home_feed_selected = self.state.home_feed_selected.saturating_sub(1);
    }

    pub(crate) async fn open_home_feed_item(&mut self) -> Result<()> {
        if self.state.transactions.items.is_empty() {
            self.load_transactions(true).await?;
        }
        if self.state.snapshot.is_none() {
            self.refresh_snapshot().await?;
        }

        let feed_items = home_feed_items(&self.state);
        if feed_items.is_empty() {
            return Ok(());
        }

        if self.state.home_feed_selected >= feed_items.len() {
            self.state.home_feed_selected = feed_items.len() - 1;
        }

        match &feed_items[self.state.home_feed_selected] {
            HomeFeedItem::Transaction { index } => {
                self.state.section = Section::Transactions;
                self.state.transactions.mode = TransactionsMode::List;
                self.state.transactions.search.query.clear();
                self.state.transactions.search.active = false;

                let visible = transactions_visible_indices(&self.state);
                if let Some(pos) = visible.iter().position(|idx| idx == index) {
                    self.state.transactions.selected = pos;
                }
                self.open_transaction_detail().await?;
            }
            HomeFeedItem::FlowAlert(alert) => {
                self.state.section = Section::Accounts;
                self.accounts_set_focus(AccountsTab::Budget);
                self.state.flows.search.query.clear();
                self.state.flows.search.active = false;
                if self.state.snapshot.is_none() {
                    self.refresh_snapshot().await?;
                }
                self.select_flow_by_id(alert.flow_id);
                self.open_flow_detail().await?;
            }
        }
        Ok(())
    }
}
