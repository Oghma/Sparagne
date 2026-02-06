//! Accounts section dispatch handling (wallets and flows).

use crate::app::state::{AccountsTab, EntityListMode};
use crate::app::App;
use crate::error::Result;
use crate::ui::keymap::AppAction;

impl App {
    /// Dispatches actions for the Accounts section (Sources/Envelopes/Goals tabs).
    pub(crate) async fn dispatch_accounts(&mut self, action: AppAction) -> Result<bool> {
        match action {
            AppAction::Submit => {
                match self.state.accounts_tab {
                    AccountsTab::Sources => self.handle_wallets_submit().await?,
                    AccountsTab::Envelopes => self.handle_flows_submit().await?,
                    AccountsTab::Goals => {}
                }
                Ok(true)
            }
            AppAction::Backspace => self.dispatch_accounts_backspace(),
            AppAction::Up => self.dispatch_accounts_up().await,
            AppAction::Down => self.dispatch_accounts_down().await,
            AppAction::Left => {
                self.accounts_prev_tab();
                Ok(true)
            }
            AppAction::Right => {
                self.accounts_next_tab();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn dispatch_accounts_backspace(&mut self) -> Result<bool> {
        match self.state.accounts_tab {
            AccountsTab::Sources => {
                self.backspace_wallet_form();
                Ok(true)
            }
            AccountsTab::Envelopes => {
                self.backspace_flow_form();
                Ok(true)
            }
            AccountsTab::Goals => Ok(false),
        }
    }

    async fn dispatch_accounts_up(&mut self) -> Result<bool> {
        match self.state.accounts_tab {
            AccountsTab::Sources
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
            AccountsTab::Envelopes
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
            AccountsTab::Sources
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
            AccountsTab::Envelopes
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
