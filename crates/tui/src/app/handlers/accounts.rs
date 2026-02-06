use super::super::*;

impl App {
    pub(crate) fn accounts_set_tab(&mut self, index: usize) {
        let next = AccountsTab::from_index(index);
        if next == self.state.accounts_tab {
            return;
        }

        // Switching tabs should not leave hidden forms/details active.
        self.state.wallets.mode = EntityListMode::List;
        self.state.wallets.detail = WalletDetailState::default();
        self.state.wallets.search.active = false;
        self.reset_wallet_form();

        self.state.flows.mode = EntityListMode::List;
        self.state.flows.detail = FlowDetailState::default();
        self.state.flows.search.active = false;
        self.reset_flow_form();

        self.state.accounts_tab = next;
    }

    pub(crate) fn accounts_next_tab(&mut self) {
        let next = self.state.accounts_tab.next();
        self.accounts_set_tab(next.index());
    }

    pub(crate) fn accounts_prev_tab(&mut self) {
        let prev = self.state.accounts_tab.prev();
        self.accounts_set_tab(prev.index());
    }
}
