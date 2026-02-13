//! Editing and creation shortcuts.

use super::super::super::*;

use crate::error::Result;

impl App {
    /// Handles editing and creation shortcuts.
    pub(crate) async fn handle_editing_shortcut(&mut self, ch: char) -> Result<()> {
        match ch {
            // Edit selected item
            'e' | 'E' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::Detail
                {
                    self.start_transaction_edit().await?;
                } else if self.state.section == Section::Accounts {
                    match self.state.accounts_tab {
                        AccountsTab::Wallets if self.state.wallets.mode == EntityListMode::List => {
                            self.start_wallet_rename();
                        }
                        AccountsTab::Budget if self.state.flows.mode == EntityListMode::List => {
                            self.start_flow_rename();
                        }
                        AccountsTab::Wallets | AccountsTab::Budget => {}
                    }
                } else if self.is_settings_tab(SettingsTab::Categories)
                    && self.state.categories.mode == CategoriesMode::List
                {
                    self.start_category_rename();
                }
            }
            // Back
            'b' | 'B' => {
                self.handle_back_shortcut().await?;
            }
            // Create / clear filters
            'c' | 'C' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    if self.state.transactions.visual_mode {
                        self.open_bulk_category_dialog();
                        return Ok(());
                    }
                    self.clear_filters().await?;
                } else if self.state.section == Section::Accounts {
                    match self.state.accounts_tab {
                        AccountsTab::Wallets if self.state.wallets.mode == EntityListMode::List => {
                            self.start_wallet_create();
                        }
                        AccountsTab::Budget if self.state.flows.mode == EntityListMode::List => {
                            self.start_flow_create();
                        }
                        AccountsTab::Wallets | AccountsTab::Budget => {}
                    }
                } else if self.is_settings_tab(SettingsTab::Categories)
                    && self.state.categories.mode == CategoriesMode::List
                {
                    self.start_category_create();
                } else if self.is_settings_tab(SettingsTab::Vault)
                    && self.state.vault_ui.mode == VaultMode::View
                {
                    self.start_vault_create();
                }
            }
            // Aliases / vault list
            'l' | 'L' => {
                if self.is_settings_tab(SettingsTab::Categories)
                    && self.state.categories.mode == CategoriesMode::List
                {
                    self.start_category_aliases().await?;
                } else if self.is_settings_tab(SettingsTab::Vault)
                    && self.state.vault_ui.mode == VaultMode::View
                {
                    self.start_vault_select().await?;
                }
            }
            // Delete alias / delete vault
            'x' | 'X' => {
                if self.is_settings_tab(SettingsTab::Categories)
                    && self.state.categories.mode == CategoriesMode::Aliases
                    && self.state.categories.aliases.focus == AliasFocus::List
                {
                    self.delete_category_alias().await?;
                } else if self.is_settings_tab(SettingsTab::Vault)
                    && self.state.vault_ui.mode == VaultMode::View
                {
                    self.open_vault_delete_dialog();
                }
            }
            // Merge categories / cycle flow mode
            'm' | 'M' => {
                if self.state.section == Section::Accounts
                    && self.state.accounts_tab == AccountsTab::Budget
                    && self.state.flows.mode == EntityListMode::Create
                    && self.state.flows.form.focus == FlowFormField::Mode
                {
                    self.cycle_flow_mode_next();
                } else if self.is_settings_tab(SettingsTab::Categories)
                    && self.state.categories.mode == CategoriesMode::List
                {
                    self.start_category_merge();
                }
            }
            // Delete/archive
            'd' | 'D' => {
                self.handle_delete_archive_shortcut().await?;
            }
            // Unshare flow (remove flow reference)
            'u' | 'U' => {
                if self.state.section == Section::Accounts
                    && self.state.accounts_tab == AccountsTab::Budget
                    && self.state.flows.mode == EntityListMode::List
                    && let Some(flow) = self.selected_flow()
                    && flow.is_reference
                {
                    self.open_flow_unshare_dialog();
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Handles the back shortcut.
    async fn handle_back_shortcut(&mut self) -> Result<()> {
        if self.state.section == Section::Transactions
            && self.state.transactions.mode != TransactionsMode::List
        {
            match self.state.transactions.mode {
                TransactionsMode::Detail => {
                    self.state.transactions.mode = TransactionsMode::List;
                    self.state.transactions.detail = None;
                }
                TransactionsMode::Edit => {
                    self.state.transactions.mode = TransactionsMode::Detail;
                    self.state.transactions.form = TransactionFormState::default();
                }
                TransactionsMode::Form => {
                    if self.state.transactions.form.editing_id.is_some() {
                        self.state.transactions.mode = TransactionsMode::Detail;
                    } else {
                        self.state.transactions.mode = TransactionsMode::List;
                    }
                    self.state.transactions.form = TransactionFormState::default();
                }
                TransactionsMode::TransferWallet | TransactionsMode::TransferFlow => {
                    if self.state.transactions.transfer.editing_id.is_some() {
                        self.state.transactions.mode = TransactionsMode::Detail;
                    } else {
                        self.state.transactions.mode = TransactionsMode::List;
                    }
                    self.state.transactions.transfer = TransferFormState::default();
                }
                TransactionsMode::PickWallet | TransactionsMode::PickFlow => {
                    self.state.transactions.mode = TransactionsMode::List;
                    self.state.transactions.picker_index = 0;
                }
                TransactionsMode::TransferPicker => {
                    self.state.transactions.mode = TransactionsMode::List;
                    self.state.transactions.picker_index = 0;
                }
                TransactionsMode::Filter => {
                    self.state.transactions.mode = TransactionsMode::List;
                    self.state.transactions.filter.error = None;
                }
                TransactionsMode::List => {}
            }
        } else if self.state.section == Section::Accounts {
            match self.state.accounts_tab {
                AccountsTab::Wallets if self.state.wallets.mode != EntityListMode::List => {
                    self.state.wallets.mode = EntityListMode::List;
                    self.state.wallets.detail = WalletDetailState::default();
                    self.reset_wallet_form();
                }
                AccountsTab::Budget if self.state.flows.mode != EntityListMode::List => {
                    self.state.flows.mode = EntityListMode::List;
                    self.state.flows.detail = FlowDetailState::default();
                    self.reset_flow_form();
                }
                AccountsTab::Wallets | AccountsTab::Budget => {}
            }
        } else if self.is_settings_tab(SettingsTab::Vault)
            && self.state.vault_ui.mode != VaultMode::View
        {
            self.reset_vault_form();
            self.state.vault_ui.defaults = DefaultsFormState::default();
            self.state.vault_ui.mode = VaultMode::View;
        }
        Ok(())
    }

    /// Handles delete/archive shortcuts.
    async fn handle_delete_archive_shortcut(&mut self) -> Result<()> {
        if self.state.section == Section::Transactions
            && matches!(
                self.state.transactions.mode,
                TransactionsMode::List | TransactionsMode::Detail
            )
        {
            self.open_transaction_delete_dialog();
            return Ok(());
        }
        if self.state.section == Section::Accounts {
            match self.state.accounts_tab {
                AccountsTab::Wallets if self.state.wallets.mode == EntityListMode::List => {
                    if let Some(wallet) = self.selected_wallet()
                        && !wallet.archived
                    {
                        self.open_wallet_archive_dialog();
                    } else {
                        self.toggle_wallet_archive().await?;
                    }
                    return Ok(());
                }
                AccountsTab::Budget if self.state.flows.mode == EntityListMode::List => {
                    if let Some(flow) = self.selected_flow()
                        && !flow.archived
                    {
                        self.open_flow_archive_dialog();
                    } else {
                        self.toggle_flow_archive().await?;
                    }
                    return Ok(());
                }
                AccountsTab::Wallets | AccountsTab::Budget => {}
            }
        }
        if self.is_settings_tab(SettingsTab::Categories)
            && self.state.categories.mode == CategoriesMode::List
        {
            if let Some(category) = self.selected_category()
                && !category.archived
            {
                self.open_category_archive_dialog();
            } else {
                self.toggle_category_archive().await?;
            }
            return Ok(());
        }
        if self.is_settings_tab(SettingsTab::Vault) && self.state.vault_ui.mode == VaultMode::View {
            if self.state.snapshot.is_none() {
                self.refresh_snapshot().await?;
            }
            self.start_defaults();
        }
        Ok(())
    }
}
