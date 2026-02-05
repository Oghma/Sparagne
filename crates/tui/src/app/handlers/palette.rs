use super::super::*;

use crate::error::Result;
use api_types::transaction::TransactionKind;

impl App {
    pub(crate) fn handle_help_action(&mut self, action: crate::ui::keymap::AppAction) {
        match action {
            crate::ui::keymap::AppAction::Cancel => {
                self.state.help.active = false;
            }
            crate::ui::keymap::AppAction::Input('?') => {
                self.state.help.active = false;
            }
            _ => {}
        }
    }

    pub(crate) fn open_palette(&mut self) {
        // Load MRU from local state
        if let Some(vault) = &self.state.vault
            && let Some(vault_id) = &vault.id
        {
            let mru_strings = self
                .local_state
                .mru_commands_for(&self.config.username, vault_id);
            self.state.palette.mru = mru_strings.iter().filter_map(|s| s.parse().ok()).collect();
        }

        self.state.palette.active = true;
        self.state.palette.query.clear();
        self.state.palette.selected = 0;
        self.update_palette_filtered_count();
    }

    pub(crate) async fn handle_palette_action(
        &mut self,
        action: crate::ui::keymap::AppAction,
    ) -> Result<()> {
        match action {
            crate::ui::keymap::AppAction::Cancel => {
                self.state.palette.active = false;
            }
            crate::ui::keymap::AppAction::Backspace => {
                self.state.palette.query.pop();
                self.state.palette.selected = 0;
                self.update_palette_filtered_count();
            }
            crate::ui::keymap::AppAction::Up => {
                self.state.palette.select_prev();
            }
            crate::ui::keymap::AppAction::Down => {
                self.state.palette.select_next();
            }
            crate::ui::keymap::AppAction::Input(ch) => {
                self.state.palette.query.push(ch);
                self.state.palette.selected = 0;
                self.update_palette_filtered_count();
            }
            crate::ui::keymap::AppAction::Submit => {
                if let Some(command) = self.filtered_commands().get(self.state.palette.selected) {
                    self.execute_command(*command).await?;
                    self.state.palette.active = false;
                }
            }
            crate::ui::keymap::AppAction::TogglePalette => {
                self.state.palette.active = false;
            }
            _ => {}
        }

        Ok(())
    }

    /// Updates the cached filtered command count for selection bounds.
    fn update_palette_filtered_count(&mut self) {
        self.state.palette.filtered_count = self.filtered_commands().len();
    }

    pub(crate) fn filtered_commands(&self) -> Vec<PaletteCommand> {
        filter_commands(self.state.palette.query.as_str(), &self.state.palette.mru)
    }

    pub(crate) async fn execute_command(&mut self, command: PaletteCommand) -> Result<()> {
        // Track MRU
        self.track_mru_command(command);

        match command {
            PaletteCommand::NewExpense => {
                self.start_transaction_form(TransactionKind::Expense)
                    .await?;
            }
            PaletteCommand::NewIncome => {
                self.start_transaction_form(TransactionKind::Income).await?;
            }
            PaletteCommand::NewRefund => {
                self.start_transaction_form(TransactionKind::Refund).await?;
            }
            PaletteCommand::NewTransferWallet => {
                self.state.section = Section::Transactions;
                self.start_transfer_wallet();
            }
            PaletteCommand::NewTransferFlow => {
                self.state.section = Section::Transactions;
                self.start_transfer_flow();
            }
            PaletteCommand::Categories => {
                self.state.section = Section::Settings;
                self.state.settings_tab = SettingsTab::Categories;
                self.load_categories().await?;
            }
            PaletteCommand::CategoryAliases => {
                self.state.section = Section::Settings;
                self.state.settings_tab = SettingsTab::Categories;
                self.load_categories().await?;
                self.start_category_aliases().await?;
            }
            PaletteCommand::Members => {
                self.open_members().await?;
            }
            PaletteCommand::WalletNew => {
                self.state.section = Section::Accounts;
                self.state.accounts_tab = AccountsTab::Sources;
                self.start_wallet_create();
            }
            PaletteCommand::FlowNew => {
                self.state.section = Section::Accounts;
                self.accounts_set_tab(1);
                if self.state.snapshot.is_none() {
                    self.refresh_snapshot().await?;
                }
                self.start_flow_create();
            }
            PaletteCommand::VaultCreate => {
                self.state.section = Section::Settings;
                self.state.settings_tab = SettingsTab::Vault;
                self.start_vault_create();
            }
            PaletteCommand::Refresh => {
                self.refresh_snapshot().await?;
                if self.state.section == Section::Transactions {
                    self.load_transactions(true).await?;
                } else if self.is_settings_tab(SettingsTab::Categories) {
                    self.load_categories().await?;
                } else if self.state.section == Section::Analytics {
                    self.load_stats().await?;
                }
            }
            PaletteCommand::ToggleVoided => {
                if self.state.section != Section::Transactions {
                    self.state.section = Section::Transactions;
                }
                self.state.transactions.include_voided = !self.state.transactions.include_voided;
                self.load_transactions(true).await?;
            }
        }

        Ok(())
    }

    /// Tracks a command in MRU and persists to local state.
    fn track_mru_command(&mut self, command: PaletteCommand) {
        // Update in-memory MRU
        self.state.palette.mru.retain(|c| *c != command);
        self.state.palette.mru.insert(0, command);
        self.state.palette.mru.truncate(MRU_LIMIT);

        // Persist to local state
        if let Some(vault) = &self.state.vault
            && let Some(vault_id) = &vault.id
        {
            self.local_state.push_mru_command(
                &self.config.username,
                vault_id,
                command.as_str(),
                MRU_LIMIT,
            );
            // Best-effort save, ignore errors
            let _ = self.local_state.save(&self.local_state_path);
        }
    }
}
