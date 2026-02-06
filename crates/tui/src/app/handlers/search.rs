use super::super::*;

use crate::error::Result;

impl App {
    pub(crate) fn start_search(&mut self) {
        match self.state.section {
            Section::Transactions => {
                self.state.transactions.search.active = true;
            }
            Section::Accounts => match self.state.accounts_tab {
                AccountsTab::Sources => {
                    self.state.wallets.search.active = true;
                }
                AccountsTab::Envelopes | AccountsTab::Goals => {
                    self.state.flows.search.active = true;
                }
            },
            _ => {}
        }
    }

    pub(crate) async fn stop_search_if_active(&mut self) -> Result<bool> {
        if self.state.transactions.search.active {
            self.state.transactions.search.active = false;
            self.refresh_transactions_search().await?;
            return Ok(true);
        }
        if self.state.wallets.search.active {
            self.state.wallets.search.active = false;
            self.refresh_wallets_search().await?;
            return Ok(true);
        }
        if self.state.flows.search.active {
            self.state.flows.search.active = false;
            self.refresh_flows_search().await?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) async fn handle_search_input(&mut self, ch: char) -> Result<bool> {
        match self.state.section {
            Section::Transactions if self.state.transactions.search.active => {
                self.state.transactions.search.query.push(ch);
                self.refresh_transactions_search().await?;
                return Ok(true);
            }
            Section::Accounts
                if self.state.accounts_tab == AccountsTab::Sources
                    && self.state.wallets.search.active =>
            {
                self.state.wallets.search.query.push(ch);
                self.refresh_wallets_search().await?;
                return Ok(true);
            }
            Section::Accounts
                if self.state.accounts_tab != AccountsTab::Sources
                    && self.state.flows.search.active =>
            {
                self.state.flows.search.query.push(ch);
                self.refresh_flows_search().await?;
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }

    pub(crate) async fn handle_search_backspace(&mut self) -> Result<bool> {
        match self.state.section {
            Section::Transactions if self.state.transactions.search.active => {
                self.state.transactions.search.query.pop();
                self.refresh_transactions_search().await?;
                return Ok(true);
            }
            Section::Accounts
                if self.state.accounts_tab == AccountsTab::Sources
                    && self.state.wallets.search.active =>
            {
                self.state.wallets.search.query.pop();
                self.refresh_wallets_search().await?;
                return Ok(true);
            }
            Section::Accounts
                if self.state.accounts_tab != AccountsTab::Sources
                    && self.state.flows.search.active =>
            {
                self.state.flows.search.query.pop();
                self.refresh_flows_search().await?;
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }

    pub(crate) async fn refresh_transactions_search(&mut self) -> Result<()> {
        let visible_len = transactions_visible_indices(&self.state).len();
        if visible_len == 0 {
            self.state.transactions.selected = 0;
            self.state.transactions.detail = None;
            return Ok(());
        }
        if self.state.transactions.selected >= visible_len {
            self.state.transactions.selected = 0;
        }
        if self.state.transactions.mode == TransactionsMode::Detail {
            self.open_transaction_detail().await?;
        }
        Ok(())
    }

    pub(crate) async fn refresh_wallets_search(&mut self) -> Result<()> {
        let visible_len = wallets_visible_indices(&self.state).len();
        if visible_len == 0 {
            self.state.wallets.selected = 0;
            self.state.wallets.detail = WalletDetailState::default();
            return Ok(());
        }
        if self.state.wallets.selected >= visible_len {
            self.state.wallets.selected = 0;
        }
        if self.state.wallets.mode == WalletsMode::Detail {
            self.open_wallet_detail().await?;
        }
        Ok(())
    }

    pub(crate) async fn refresh_flows_search(&mut self) -> Result<()> {
        let visible_len = flows_visible_indices(&self.state).len();
        if visible_len == 0 {
            self.state.flows.selected = 0;
            self.state.flows.detail = FlowDetailState::default();
            return Ok(());
        }
        if self.state.flows.selected >= visible_len {
            self.state.flows.selected = 0;
        }
        if self.state.flows.mode == FlowsMode::Detail {
            self.open_flow_detail().await?;
        }
        Ok(())
    }
}
