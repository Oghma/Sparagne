//! Main section navigation shortcuts.

use super::super::super::*;

use crate::error::Result;

impl App {
    /// Handles main section navigation shortcuts (h, t, a, y, s).
    pub(crate) async fn handle_navigation_shortcut(&mut self, ch: char) -> Result<()> {
        match ch {
            // Home
            'h' | 'H' => {
                self.state.section = Section::Home;
                self.state.transactions.mode = TransactionsMode::List;
            }
            // Transactions
            't' => {
                self.state.section = Section::Transactions;
                self.state.transactions.mode = TransactionsMode::List;
                if self.state.transactions.items.is_empty() {
                    self.load_transactions(true).await?;
                }
            }
            // Accounts
            'a' => {
                self.state.section = Section::Accounts;
                self.state.transactions.mode = TransactionsMode::List;
                if self.state.snapshot.is_none() {
                    self.refresh_snapshot().await?;
                }
            }
            // Toggle archived visibility in Accounts
            'A' => {
                if self.state.section == Section::Accounts {
                    match self.state.accounts_tab {
                        AccountsTab::Sources if self.state.wallets.mode == EntityListMode::List => {
                            self.toggle_wallets_show_archived();
                        }
                        AccountsTab::Envelopes if self.state.flows.mode == EntityListMode::List => {
                            self.toggle_flows_show_archived();
                        }
                        AccountsTab::Goals | AccountsTab::Sources | AccountsTab::Envelopes => {}
                    }
                }
            }
            // Analytics
            'y' | 'Y' => {
                self.state.section = Section::Analytics;
                self.state.transactions.mode = TransactionsMode::List;
                self.load_stats().await?;
            }
            // Settings
            's' | 'S' => {
                self.state.section = Section::Settings;
                self.state.transactions.mode = TransactionsMode::List;
                match self.state.settings_tab {
                    SettingsTab::Categories => self.load_categories().await?,
                    SettingsTab::Vault => {}
                    SettingsTab::Members => self.load_members().await?,
                    SettingsTab::Preferences => {}
                }
            }
            _ => {}
        }
        Ok(())
    }
}
