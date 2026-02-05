//! Navigation helpers and submit handlers.

use super::super::*;

use crate::error::Result;

impl App {
    pub(crate) async fn handle_transactions_submit(&mut self) -> Result<()> {
        match self.state.transactions.mode {
            TransactionsMode::List => {
                if self.state.transactions.quick_active {
                    self.submit_quick_add().await
                } else {
                    self.open_transaction_detail().await
                }
            }
            TransactionsMode::Detail => Ok(()),
            TransactionsMode::Edit | TransactionsMode::Form => self.submit_transaction_form().await,
            TransactionsMode::PickWallet => self.apply_wallet_picker().await,
            TransactionsMode::PickFlow => self.apply_flow_picker().await,
            TransactionsMode::TransferPicker => self.apply_transfer_picker(),
            TransactionsMode::TransferWallet => self.submit_transfer_wallet().await,
            TransactionsMode::TransferFlow => self.submit_transfer_flow().await,
            TransactionsMode::Filter => self.apply_filter().await,
        }
    }

    pub(crate) async fn handle_wallets_submit(&mut self) -> Result<()> {
        match self.state.wallets.mode {
            WalletsMode::List => self.open_wallet_detail().await,
            WalletsMode::Detail => Ok(()),
            WalletsMode::Create => self.submit_wallet_create().await,
            WalletsMode::Rename => self.submit_wallet_rename().await,
        }
    }

    pub(crate) async fn handle_flows_submit(&mut self) -> Result<()> {
        match self.state.flows.mode {
            FlowsMode::List => self.open_flow_detail().await,
            FlowsMode::Detail => Ok(()),
            FlowsMode::Create => self.submit_flow_create().await,
            FlowsMode::Rename => self.submit_flow_rename().await,
        }
    }

    pub(crate) async fn handle_categories_submit(&mut self) -> Result<()> {
        match self.state.categories.mode {
            CategoriesMode::List => Ok(()),
            CategoriesMode::Merge => self.submit_category_merge().await,
            CategoriesMode::Create => self.submit_category_create().await,
            CategoriesMode::Rename => self.submit_category_rename().await,
            CategoriesMode::Aliases => self.submit_category_alias_create().await,
        }
    }

    pub(crate) async fn handle_members_submit(&mut self) -> Result<()> {
        match self.state.members.mode {
            MembersMode::List => {
                self.start_member_edit();
                Ok(())
            }
            MembersMode::Form => self.submit_member_form().await,
        }
    }

    pub(crate) async fn handle_vault_submit(&mut self) -> Result<()> {
        match self.state.vault_ui.mode {
            VaultMode::Create => {
                self.submit_vault_create().await?;
            }
            VaultMode::Defaults => {
                self.save_defaults().await?;
            }
            VaultMode::Select => {
                self.submit_vault_select().await?;
            }
            VaultMode::View => {}
        }
        Ok(())
    }

    /// Sets the settings tab and loads appropriate data.
    pub(crate) fn settings_set_tab(&mut self, index: usize) {
        self.state.settings_tab = SettingsTab::from_index(index);
    }

    /// Advances to next settings tab.
    pub(crate) fn settings_next_tab(&mut self) {
        self.state.settings_tab = self.state.settings_tab.next();
    }

    /// Goes to previous settings tab.
    pub(crate) fn settings_prev_tab(&mut self) {
        self.state.settings_tab = self.state.settings_tab.prev();
    }

    /// Handles toggle/cycle actions in the Preferences settings tab.
    pub(crate) fn handle_preferences_toggle(&mut self) {
        use crate::config::Density;
        match self.state.preferences.focus {
            PreferencesField::EmojiMode => {
                self.state.emoji_mode = !self.state.emoji_mode;
            }
            PreferencesField::Density => {
                self.state.density = match self.state.density {
                    Density::Compact => Density::Normal,
                    Density::Normal => Density::Comfortable,
                    Density::Comfortable => Density::Compact,
                };
            }
        }
    }

    /// Cycles density to the next value (Compact -> Normal -> Comfortable).
    pub(crate) fn cycle_density_next(&mut self) {
        use crate::config::Density;
        self.state.density = match self.state.density {
            Density::Compact => Density::Normal,
            Density::Normal => Density::Comfortable,
            Density::Comfortable => Density::Compact,
        };
    }

    /// Cycles density to the previous value (Comfortable -> Normal -> Compact).
    pub(crate) fn cycle_density_prev(&mut self) {
        use crate::config::Density;
        self.state.density = match self.state.density {
            Density::Compact => Density::Comfortable,
            Density::Normal => Density::Compact,
            Density::Comfortable => Density::Normal,
        };
    }
}
