use super::super::*;

impl App {
    pub(crate) fn accounts_set_focus(&mut self, tab: AccountsTab) {
        self.state.accounts_tab = tab;
    }
}
