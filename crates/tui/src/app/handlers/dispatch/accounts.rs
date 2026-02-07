//! Accounts section dispatch handling (wallets and flows).

use crate::{
    app::{
        App,
        state::{AccountsTab, EntityListMode},
    },
    error::Result,
    ui::keymap::AppAction,
};

impl App {
    /// Dispatches actions for the Accounts section (Wallets/Budget tabs).
    pub(crate) async fn dispatch_accounts(&mut self, action: AppAction) -> Result<bool> {
        match action {
            AppAction::Submit => {
                match self.state.accounts_tab {
                    AccountsTab::Wallets => self.handle_wallets_submit().await?,
                    AccountsTab::Budget => self.handle_flows_submit().await?,
                }
                Ok(true)
            }
            AppAction::Backspace => self.dispatch_accounts_backspace(),
            AppAction::Up => self.dispatch_accounts_up().await,
            AppAction::Down => self.dispatch_accounts_down().await,
            AppAction::Left => {
                self.accounts_set_focus(AccountsTab::Wallets);
                Ok(true)
            }
            AppAction::Right => {
                self.accounts_set_focus(AccountsTab::Budget);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn dispatch_accounts_backspace(&mut self) -> Result<bool> {
        match self.state.accounts_tab {
            AccountsTab::Wallets => {
                self.backspace_wallet_form();
                Ok(true)
            }
            AccountsTab::Budget => {
                self.backspace_flow_form();
                Ok(true)
            }
        }
    }

    async fn dispatch_accounts_up(&mut self) -> Result<bool> {
        match self.state.accounts_tab {
            AccountsTab::Wallets
                if matches!(
                    self.state.wallets.mode,
                    EntityListMode::List | EntityListMode::Detail
                ) =>
            {
                self.wallets_select_prev();
                if self.state.wallets.mode == EntityListMode::Detail {
                    self.open_wallet_detail().await?;
                }
                Ok(true)
            }
            AccountsTab::Budget
                if matches!(
                    self.state.flows.mode,
                    EntityListMode::List | EntityListMode::Detail
                ) =>
            {
                self.flows_select_prev();
                if self.state.flows.mode == EntityListMode::Detail {
                    self.open_flow_detail().await?;
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn dispatch_accounts_down(&mut self) -> Result<bool> {
        match self.state.accounts_tab {
            AccountsTab::Wallets
                if matches!(
                    self.state.wallets.mode,
                    EntityListMode::List | EntityListMode::Detail
                ) =>
            {
                self.wallets_select_next();
                if self.state.wallets.mode == EntityListMode::Detail {
                    self.open_wallet_detail().await?;
                }
                Ok(true)
            }
            AccountsTab::Budget
                if matches!(
                    self.state.flows.mode,
                    EntityListMode::List | EntityListMode::Detail
                ) =>
            {
                self.flows_select_next();
                if self.state.flows.mode == EntityListMode::Detail {
                    self.open_flow_detail().await?;
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
