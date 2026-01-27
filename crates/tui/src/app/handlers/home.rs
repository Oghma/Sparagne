use super::super::*;

use crate::error::Result;

impl App {
    pub(crate) fn home_feed_select_next(&mut self) {
        let len = home_feed_indices(&self.state).len();
        if len == 0 {
            self.state.home_feed_selected = 0;
            return;
        }
        self.state.home_feed_selected = (self.state.home_feed_selected + 1).min(len - 1);
    }

    pub(crate) fn home_feed_select_prev(&mut self) {
        let len = home_feed_indices(&self.state).len();
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

        self.state.section = Section::Transactions;
        self.state.transactions.mode = TransactionsMode::List;
        self.state.transactions.search_query.clear();
        self.state.transactions.search_active = false;

        let indices = home_feed_indices(&self.state);
        if indices.is_empty() {
            return Ok(());
        }

        if self.state.home_feed_selected >= indices.len() {
            self.state.home_feed_selected = indices.len() - 1;
        }

        let item_index = indices[self.state.home_feed_selected];
        let visible = transactions_visible_indices(&self.state);
        if let Some(pos) = visible.iter().position(|idx| *idx == item_index) {
            self.state.transactions.selected = pos;
        }

        self.open_transaction_detail().await?;
        Ok(())
    }
}
