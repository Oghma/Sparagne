//! Cancel action handlers.
//!
//! This module contains handlers for the Cancel (Escape) action, organized by section.

use super::super::*;

use crate::error::Result;

impl App {
    /// Handles the Cancel action for the current context.
    ///
    /// Returns `true` if the cancel was fully handled, `false` if the caller should
    /// continue with default behavior (going home).
    pub(crate) async fn handle_cancel(&mut self) -> Result<bool> {
        // Handle login screen
        if self.state.screen == Screen::Login {
            self.should_quit = true;
            return Ok(true);
        }

        // Handle visual mode in transactions
        if self.state.section == Section::Transactions
            && self.state.transactions.mode == TransactionsMode::List
            && self.state.transactions.visual_mode
        {
            self.exit_visual_mode();
            return Ok(true);
        }

        // Handle discard dialog or search
        if self.maybe_open_discard_dialog() || self.stop_search_if_active().await? {
            return Ok(true);
        }

        // Handle section-specific cancel
        match self.state.section {
            Section::Transactions => {
                self.cancel_transactions();
                Ok(true)
            }
            Section::Accounts => {
                self.cancel_accounts();
                Ok(true)
            }
            Section::Settings => {
                self.cancel_settings();
                Ok(true)
            }
            Section::Analytics => {
                self.state.section = Section::Home;
                Ok(true)
            }
            Section::Home => Ok(true),
        }
    }

    /// Handles cancel within the Transactions section.
    fn cancel_transactions(&mut self) {
        match self.state.transactions.mode {
            TransactionsMode::Edit => {
                self.state.transactions.mode = TransactionsMode::Detail;
                self.state.transactions.form = TransactionFormState::default();
            }
            TransactionsMode::Detail => {
                self.state.transactions.mode = TransactionsMode::List;
                self.state.transactions.detail = None;
            }
            TransactionsMode::PickWallet
            | TransactionsMode::PickFlow
            | TransactionsMode::TransferPicker => {
                self.state.transactions.mode = TransactionsMode::List;
                self.state.transactions.picker_index = 0;
            }
            TransactionsMode::TransferWallet | TransactionsMode::TransferFlow => {
                if self.state.transactions.transfer.editing_id.is_some() {
                    self.state.transactions.mode = TransactionsMode::Detail;
                } else {
                    self.state.transactions.mode = TransactionsMode::List;
                }
                self.state.transactions.transfer = TransferFormState::default();
            }
            TransactionsMode::Form => {
                if self.state.transactions.form.editing_id.is_some() {
                    self.state.transactions.mode = TransactionsMode::Detail;
                } else {
                    self.state.transactions.mode = TransactionsMode::List;
                }
                self.state.transactions.form = TransactionFormState::default();
            }
            TransactionsMode::Filter => {
                self.state.transactions.mode = TransactionsMode::List;
                self.state.transactions.filter.error = None;
            }
            TransactionsMode::List => {
                if self.state.transactions.quick_active {
                    self.state.transactions.quick_active = false;
                    self.state.transactions.quick_input.clear();
                    self.state.transactions.quick_error = None;
                } else {
                    self.state.section = Section::Home;
                }
            }
        }
    }

    /// Handles cancel within the Accounts section.
    fn cancel_accounts(&mut self) {
        match self.state.accounts_tab {
            AccountsTab::Sources => match self.state.wallets.mode {
                EntityListMode::Create | EntityListMode::Rename => {
                    self.reset_wallet_form();
                    self.state.wallets.mode = EntityListMode::List;
                }
                EntityListMode::Detail => {
                    self.state.wallets.mode = EntityListMode::List;
                    self.state.wallets.detail = WalletDetailState::default();
                }
                EntityListMode::List => {
                    self.state.section = Section::Home;
                }
            },
            AccountsTab::Envelopes | AccountsTab::Goals => match self.state.flows.mode {
                EntityListMode::Create | EntityListMode::Rename => {
                    self.reset_flow_form();
                    self.state.flows.mode = EntityListMode::List;
                }
                EntityListMode::Detail => {
                    self.state.flows.mode = EntityListMode::List;
                    self.state.flows.detail = FlowDetailState::default();
                }
                EntityListMode::List => {
                    self.state.section = Section::Home;
                }
            },
        }
    }

    /// Handles cancel within the Settings section.
    fn cancel_settings(&mut self) {
        match self.state.settings_tab {
            SettingsTab::Vault => match self.state.vault_ui.mode {
                VaultMode::Create => {
                    self.reset_vault_form();
                    self.state.vault_ui.mode = VaultMode::View;
                }
                VaultMode::Defaults => {
                    self.state.vault_ui.defaults = DefaultsFormState::default();
                    self.state.vault_ui.mode = VaultMode::View;
                }
                VaultMode::Select => {
                    self.state.vault_ui.mode = VaultMode::View;
                }
                VaultMode::View => {
                    self.state.section = Section::Home;
                }
            },
            SettingsTab::Categories => match self.state.categories.mode {
                CategoriesMode::Merge => {
                    self.state.categories.mode = CategoriesMode::List;
                    self.state.categories.merge = CategoryMergeState::default();
                }
                CategoriesMode::Create | CategoriesMode::Rename => {
                    self.reset_category_form();
                    self.state.categories.mode = CategoriesMode::List;
                }
                CategoriesMode::Aliases => {
                    if self.state.categories.aliases.focus == AliasFocus::Input
                        && !self.state.categories.aliases.input.is_empty()
                    {
                        self.state.categories.aliases.input.clear();
                        self.state.categories.aliases.error = None;
                        self.state.categories.aliases.focus = AliasFocus::List;
                    } else {
                        self.state.categories.mode = CategoriesMode::List;
                        self.state.categories.aliases = CategoryAliasState::default();
                    }
                }
                CategoriesMode::List => {
                    self.state.section = Section::Home;
                }
            },
            SettingsTab::Members => match self.state.members.mode {
                MembersMode::Form => {
                    self.reset_member_form();
                    self.state.members.mode = MembersMode::List;
                }
                MembersMode::List => {
                    self.state.section = Section::Home;
                }
            },
            SettingsTab::Preferences => {
                self.state.section = Section::Home;
            }
        }
    }
}
