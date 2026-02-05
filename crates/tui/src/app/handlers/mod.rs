mod accounts;
mod cancel;
mod categories;
mod flows;
mod forms;
mod global_search;
mod home;
mod input;
mod members;
mod navigation;
mod overlays;
mod palette;
mod search;
mod shortcuts;
mod stats;
mod toast;
mod transactions;
mod vault;
mod wallets;

use crossterm::event::KeyEvent;

use crate::error::Result;

use super::{App, state::*};

impl App {
    /// Checks if we are in Settings section showing a specific sub-tab.
    fn is_settings_tab(&self, tab: SettingsTab) -> bool {
        self.state.section == Section::Settings && self.state.settings_tab == tab
    }

    #[doc(hidden)]
    pub async fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        let action = crate::ui::keymap::map_key(key);
        if self.state.overlays.has_confirm_dialog() {
            self.handle_confirm_action(action).await?;
            return Ok(());
        }
        if self.state.overlays.error.is_some() {
            self.handle_error_action(action).await?;
            return Ok(());
        }
        if self.state.overlays.bulk_category.is_some() {
            self.handle_bulk_category_action(action).await?;
            return Ok(());
        }
        if self.state.overlays.grouping.is_some() {
            self.handle_grouping_action(action).await?;
            return Ok(());
        }
        if self.state.help.active {
            self.handle_help_action(action);
            return Ok(());
        }
        if self.state.palette.active {
            self.handle_palette_action(action).await?;
            return Ok(());
        }
        if self.state.global_search.active {
            self.handle_global_search_action(action).await?;
            return Ok(());
        }

        match action {
            crate::ui::keymap::AppAction::TogglePalette => {
                if self.state.screen == Screen::Home {
                    self.open_palette();
                }
            }
            crate::ui::keymap::AppAction::Search => {
                if self.state.screen == Screen::Home {
                    match self.state.section {
                        Section::Transactions
                            if self.state.transactions.mode == TransactionsMode::List =>
                        {
                            self.start_search();
                        }
                        Section::Accounts => match self.state.accounts_tab {
                            AccountsTab::Sources
                                if self.state.wallets.mode == WalletsMode::List =>
                            {
                                self.start_search();
                            }
                            AccountsTab::Envelopes | AccountsTab::Goals
                                if self.state.flows.mode == FlowsMode::List =>
                            {
                                self.start_search();
                            }
                            _ => self.open_global_search(),
                        },
                        _ => self.open_global_search(),
                    }
                }
            }
            crate::ui::keymap::AppAction::CycleAmbiguous => {
                self.cycle_quick_add_ambiguous();
            }
            crate::ui::keymap::AppAction::Quit => {
                self.should_quit = true;
            }
            crate::ui::keymap::AppAction::Cancel => {
                self.handle_cancel().await?;
            }
            crate::ui::keymap::AppAction::NextField => {
                self.advance_focus();
            }
            crate::ui::keymap::AppAction::Submit => {
                if self.state.screen == Screen::Login {
                    self.attempt_login().await?;
                } else if self.state.section == Section::Home {
                    self.open_home_feed_item().await?;
                } else if self.state.section == Section::Transactions {
                    self.handle_transactions_submit().await?;
                } else if self.state.section == Section::Accounts {
                    match self.state.accounts_tab {
                        AccountsTab::Sources => self.handle_wallets_submit().await?,
                        AccountsTab::Envelopes => self.handle_flows_submit().await?,
                        AccountsTab::Goals => {}
                    }
                } else if self.is_settings_tab(SettingsTab::Categories) {
                    self.handle_categories_submit().await?;
                } else if self.is_settings_tab(SettingsTab::Members) {
                    self.handle_members_submit().await?;
                } else if self.is_settings_tab(SettingsTab::Vault) {
                    self.handle_vault_submit().await?;
                } else if self.state.section == Section::Analytics {
                    self.load_stats().await?;
                }
            }
            crate::ui::keymap::AppAction::Backspace => {
                if self.state.screen == Screen::Login {
                    let field = self.active_field_mut();
                    field.pop();
                } else if self.handle_search_backspace().await? {
                    return Ok(());
                } else if self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::Form | TransactionsMode::Edit
                    )
                {
                    self.backspace_transaction_form();
                } else if self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::TransferWallet | TransactionsMode::TransferFlow
                    )
                {
                    match self.state.transactions.transfer.focus {
                        TransferField::Amount => {
                            self.state.transactions.transfer.amount.pop();
                        }
                        TransferField::Note => {
                            self.state.transactions.transfer.note.pop();
                        }
                        TransferField::OccurredAt => {
                            self.state.transactions.transfer.occurred_at.pop();
                        }
                        _ => {}
                    }
                } else if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::Filter
                {
                    match self.state.transactions.filter.focus {
                        FilterField::From => {
                            self.state.transactions.filter.from_input.pop();
                        }
                        FilterField::To => {
                            self.state.transactions.filter.to_input.pop();
                        }
                        FilterField::Kinds => {}
                    }
                } else if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                    && self.state.transactions.quick_active
                {
                    self.state.transactions.quick_input.pop();
                } else if self.is_settings_tab(SettingsTab::Categories)
                    && self.state.categories.mode == CategoriesMode::Aliases
                    && self.state.categories.aliases.focus == AliasFocus::Input
                {
                    self.state.categories.aliases.input.pop();
                } else if self.is_settings_tab(SettingsTab::Categories)
                    && matches!(
                        self.state.categories.mode,
                        CategoriesMode::Create | CategoriesMode::Rename
                    )
                {
                    self.backspace_category_form();
                } else if self.state.section == Section::Accounts {
                    match self.state.accounts_tab {
                        AccountsTab::Sources => self.backspace_wallet_form(),
                        AccountsTab::Envelopes => self.backspace_flow_form(),
                        AccountsTab::Goals => {}
                    }
                } else if self.is_settings_tab(SettingsTab::Members)
                    && self.state.members.mode == MembersMode::Form
                {
                    if self.state.members.form.focus == MemberFormField::Username {
                        self.state.members.form.username.pop();
                    }
                } else if self.is_settings_tab(SettingsTab::Vault) {
                    self.backspace_vault_form();
                }
            }
            crate::ui::keymap::AppAction::Up => {
                if self.state.screen == Screen::Home && self.state.section == Section::Home {
                    self.home_feed_select_prev();
                } else if self.state.screen == Screen::Home
                    && self.is_settings_tab(SettingsTab::Members)
                {
                    match self.state.members.mode {
                        MembersMode::Form => {
                            if self.state.members.form.focus == MemberFormField::Role {
                                self.cycle_member_role(false);
                            }
                        }
                        MembersMode::List => {
                            self.members_select_prev();
                        }
                    }
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::List | TransactionsMode::Detail
                    )
                {
                    self.state.transactions.select_prev();
                    if self.state.transactions.mode == TransactionsMode::Detail {
                        self.open_transaction_detail().await?;
                    }
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::PickWallet | TransactionsMode::PickFlow
                    )
                {
                    self.transactions_picker_prev();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::TransferPicker
                {
                    self.transfer_picker_prev();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::TransferWallet | TransactionsMode::TransferFlow
                    )
                {
                    self.transfer_select_prev();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::Form | TransactionsMode::Edit
                    )
                {
                    self.transaction_form_select_prev();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Accounts
                {
                    match self.state.accounts_tab {
                        AccountsTab::Sources
                            if matches!(
                                self.state.wallets.mode,
                                WalletsMode::List | WalletsMode::Detail
                            ) =>
                        {
                            self.wallets_select_prev();
                            if self.state.wallets.mode == WalletsMode::Detail {
                                self.open_wallet_detail().await?;
                            }
                        }
                        AccountsTab::Envelopes
                            if matches!(
                                self.state.flows.mode,
                                FlowsMode::List | FlowsMode::Detail
                            ) =>
                        {
                            self.flows_select_prev();
                            if self.state.flows.mode == FlowsMode::Detail {
                                self.open_flow_detail().await?;
                            }
                        }
                        AccountsTab::Goals | AccountsTab::Sources | AccountsTab::Envelopes => {}
                    }
                } else if self.state.screen == Screen::Home
                    && self.is_settings_tab(SettingsTab::Categories)
                {
                    match self.state.categories.mode {
                        CategoriesMode::List | CategoriesMode::Create | CategoriesMode::Rename => {
                            self.categories_select_prev();
                        }
                        CategoriesMode::Merge => self.category_merge_select_prev(),
                        CategoriesMode::Aliases => {
                            if self.state.categories.aliases.focus == AliasFocus::List {
                                self.category_alias_select_prev();
                            }
                        }
                    }
                } else if self.state.screen == Screen::Home
                    && self.is_settings_tab(SettingsTab::Vault)
                    && self.state.vault_ui.mode == VaultMode::Defaults
                {
                    self.defaults_select_prev();
                } else if self.state.screen == Screen::Home
                    && self.is_settings_tab(SettingsTab::Vault)
                    && self.state.vault_ui.mode == VaultMode::Select
                {
                    self.vaults_select_prev();
                } else if self.state.screen == Screen::Home
                    && self.is_settings_tab(SettingsTab::Preferences)
                {
                    self.state.preferences.focus = self.state.preferences.focus.prev();
                }
            }
            crate::ui::keymap::AppAction::Down => {
                if self.state.screen == Screen::Home && self.state.section == Section::Home {
                    self.home_feed_select_next();
                } else if self.state.screen == Screen::Home
                    && self.is_settings_tab(SettingsTab::Members)
                {
                    match self.state.members.mode {
                        MembersMode::Form => {
                            if self.state.members.form.focus == MemberFormField::Role {
                                self.cycle_member_role(true);
                            }
                        }
                        MembersMode::List => {
                            self.members_select_next();
                        }
                    }
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::List | TransactionsMode::Detail
                    )
                {
                    self.state.transactions.select_next();
                    if self.state.transactions.mode == TransactionsMode::Detail {
                        self.open_transaction_detail().await?;
                    }
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::PickWallet | TransactionsMode::PickFlow
                    )
                {
                    self.transactions_picker_next();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::TransferPicker
                {
                    self.transfer_picker_next();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::TransferWallet | TransactionsMode::TransferFlow
                    )
                {
                    self.transfer_select_next();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::Form | TransactionsMode::Edit
                    )
                {
                    self.transaction_form_select_next();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Accounts
                {
                    match self.state.accounts_tab {
                        AccountsTab::Sources
                            if matches!(
                                self.state.wallets.mode,
                                WalletsMode::List | WalletsMode::Detail
                            ) =>
                        {
                            self.wallets_select_next();
                            if self.state.wallets.mode == WalletsMode::Detail {
                                self.open_wallet_detail().await?;
                            }
                        }
                        AccountsTab::Envelopes
                            if matches!(
                                self.state.flows.mode,
                                FlowsMode::List | FlowsMode::Detail
                            ) =>
                        {
                            self.flows_select_next();
                            if self.state.flows.mode == FlowsMode::Detail {
                                self.open_flow_detail().await?;
                            }
                        }
                        AccountsTab::Goals | AccountsTab::Sources | AccountsTab::Envelopes => {}
                    }
                } else if self.state.screen == Screen::Home
                    && self.is_settings_tab(SettingsTab::Categories)
                {
                    match self.state.categories.mode {
                        CategoriesMode::List | CategoriesMode::Create | CategoriesMode::Rename => {
                            self.categories_select_next();
                        }
                        CategoriesMode::Merge => self.category_merge_select_next(),
                        CategoriesMode::Aliases => {
                            if self.state.categories.aliases.focus == AliasFocus::List {
                                self.category_alias_select_next();
                            }
                        }
                    }
                } else if self.state.screen == Screen::Home
                    && self.is_settings_tab(SettingsTab::Vault)
                    && self.state.vault_ui.mode == VaultMode::Defaults
                {
                    self.defaults_select_next();
                } else if self.state.screen == Screen::Home
                    && self.is_settings_tab(SettingsTab::Vault)
                    && self.state.vault_ui.mode == VaultMode::Select
                {
                    self.vaults_select_next();
                } else if self.state.screen == Screen::Home
                    && self.is_settings_tab(SettingsTab::Preferences)
                {
                    self.state.preferences.focus = self.state.preferences.focus.next();
                }
            }
            crate::ui::keymap::AppAction::Left => {
                if self.state.screen == Screen::Home {
                    if self.is_settings_tab(SettingsTab::Preferences)
                        && self.state.preferences.focus == PreferencesField::Density
                    {
                        self.cycle_density_prev();
                    } else if self.state.section == Section::Accounts {
                        self.accounts_prev_tab();
                    } else if self.state.section == Section::Analytics {
                        self.stats_prev_tab();
                    } else if self.state.section == Section::Settings {
                        self.settings_prev_tab();
                    }
                }
            }
            crate::ui::keymap::AppAction::Right => {
                if self.state.screen == Screen::Home {
                    if self.is_settings_tab(SettingsTab::Preferences)
                        && self.state.preferences.focus == PreferencesField::Density
                    {
                        self.cycle_density_next();
                    } else if self.state.section == Section::Accounts {
                        self.accounts_next_tab();
                    } else if self.state.section == Section::Analytics {
                        self.stats_next_tab();
                    } else if self.state.section == Section::Settings {
                        self.settings_next_tab();
                    }
                }
            }
            crate::ui::keymap::AppAction::Input(ch) => {
                if self.state.screen == Screen::Login {
                    let field = self.active_field_mut();
                    field.push(ch);
                } else if !self.route_input(ch).await? {
                    self.handle_non_login_key(ch).await?;
                }
            }
            crate::ui::keymap::AppAction::None => {}
        }

        Ok(())
    }
}
