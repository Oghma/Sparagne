mod accounts;
mod categories;
mod flows;
mod forms;
mod home;
mod members;
mod navigation;
mod overlays;
mod palette;
mod search;
mod stats;
mod toast;
mod transactions;
mod vault;
mod wallets;

use crossterm::event::KeyEvent;

use crate::error::Result;

use super::{App, state::*};

impl App {
    #[doc(hidden)]
    pub async fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        let action = crate::ui::keymap::map_key(key);
        if self.state.overlays.confirm.is_some() {
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
        if self.state.help.active {
            self.handle_help_action(action);
            return Ok(());
        }
        if self.state.palette.active {
            self.handle_palette_action(action).await?;
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
                    self.start_search();
                }
            }
            crate::ui::keymap::AppAction::Quit => {
                self.should_quit = true;
            }
            crate::ui::keymap::AppAction::Cancel => {
                if self.state.screen == Screen::Login {
                    self.should_quit = true;
                } else if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                    && self.state.transactions.visual_mode
                {
                    self.exit_visual_mode();
                    return Ok(());
                } else if self.maybe_open_discard_dialog() {
                    return Ok(());
                } else if self.stop_search_if_active().await? {
                    return Ok(());
                } else if self.state.section == Section::Transactions {
                    match self.state.transactions.mode {
                        TransactionsMode::Edit => {
                            self.state.transactions.mode = TransactionsMode::Detail;
                            self.state.transactions.form = TransactionFormState::default();
                        }
                        TransactionsMode::Detail => {
                            self.state.transactions.mode = TransactionsMode::List;
                            self.state.transactions.detail = None;
                        }
                        TransactionsMode::PickWallet | TransactionsMode::PickFlow => {
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
                } else if self.state.section == Section::Wallets {
                    match self.state.wallets.mode {
                        WalletsMode::Create | WalletsMode::Rename => {
                            self.reset_wallet_form();
                            self.state.wallets.mode = WalletsMode::List;
                        }
                        WalletsMode::Detail => {
                            self.state.wallets.mode = WalletsMode::List;
                            self.state.wallets.detail = WalletDetailState::default();
                        }
                        WalletsMode::List => {
                            self.state.section = Section::Home;
                        }
                    }
                } else if self.state.section == Section::Flows {
                    match self.state.accounts_tab {
                        AccountsTab::Sources => match self.state.wallets.mode {
                            WalletsMode::Create | WalletsMode::Rename => {
                                self.reset_wallet_form();
                                self.state.wallets.mode = WalletsMode::List;
                            }
                            WalletsMode::Detail => {
                                self.state.wallets.mode = WalletsMode::List;
                                self.state.wallets.detail = WalletDetailState::default();
                            }
                            WalletsMode::List => {
                                self.state.section = Section::Home;
                            }
                        },
                        AccountsTab::Envelopes | AccountsTab::Goals => {
                            match self.state.flows.mode {
                                FlowsMode::Create | FlowsMode::Rename => {
                                    self.reset_flow_form();
                                    self.state.flows.mode = FlowsMode::List;
                                }
                                FlowsMode::Detail => {
                                    self.state.flows.mode = FlowsMode::List;
                                    self.state.flows.detail = FlowDetailState::default();
                                }
                                FlowsMode::List => {
                                    self.state.section = Section::Home;
                                }
                            }
                        }
                    }
                } else if self.state.section == Section::Vault {
                    match self.state.vault_ui.mode {
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
                    }
                } else if self.state.section == Section::Categories {
                    match self.state.categories.mode {
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
                    }
                } else if self.state.section == Section::Members {
                    match self.state.members.mode {
                        MembersMode::Form => {
                            self.reset_member_form();
                            self.state.members.mode = MembersMode::List;
                        }
                        MembersMode::List => {
                            self.state.section = Section::Home;
                        }
                    }
                } else if self.state.section == Section::Stats {
                    self.state.section = Section::Home;
                }
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
                } else if self.state.section == Section::Wallets {
                    self.handle_wallets_submit().await?;
                } else if self.state.section == Section::Flows {
                    match self.state.accounts_tab {
                        AccountsTab::Sources => self.handle_wallets_submit().await?,
                        AccountsTab::Envelopes => self.handle_flows_submit().await?,
                        AccountsTab::Goals => {}
                    }
                } else if self.state.section == Section::Categories {
                    self.handle_categories_submit().await?;
                } else if self.state.section == Section::Members {
                    self.handle_members_submit().await?;
                } else if self.state.section == Section::Vault {
                    self.handle_vault_submit().await?;
                } else if self.state.section == Section::Stats {
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
                } else if self.state.section == Section::Categories
                    && self.state.categories.mode == CategoriesMode::Aliases
                    && self.state.categories.aliases.focus == AliasFocus::Input
                {
                    self.state.categories.aliases.input.pop();
                } else if self.state.section == Section::Categories
                    && matches!(
                        self.state.categories.mode,
                        CategoriesMode::Create | CategoriesMode::Rename
                    )
                {
                    self.backspace_category_form();
                } else if self.state.section == Section::Wallets {
                    self.backspace_wallet_form();
                } else if self.state.section == Section::Flows {
                    match self.state.accounts_tab {
                        AccountsTab::Sources => self.backspace_wallet_form(),
                        AccountsTab::Envelopes => self.backspace_flow_form(),
                        AccountsTab::Goals => {}
                    }
                } else if self.state.section == Section::Members
                    && self.state.members.mode == MembersMode::Form
                {
                    if self.state.members.form.focus == MemberFormField::Username {
                        self.state.members.form.username.pop();
                    }
                } else if self.state.section == Section::Vault {
                    self.backspace_vault_form();
                }
            }
            crate::ui::keymap::AppAction::Up => {
                if self.state.screen == Screen::Home && self.state.section == Section::Home {
                    self.home_feed_select_prev();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Members
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
                    && self.state.section == Section::Wallets
                    && matches!(
                        self.state.wallets.mode,
                        WalletsMode::List | WalletsMode::Detail
                    )
                {
                    self.wallets_select_prev();
                    if self.state.wallets.mode == WalletsMode::Detail {
                        self.open_wallet_detail().await?;
                    }
                } else if self.state.screen == Screen::Home && self.state.section == Section::Flows
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
                    && self.state.section == Section::Categories
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
                    && self.state.section == Section::Vault
                    && self.state.vault_ui.mode == VaultMode::Defaults
                {
                    self.defaults_select_prev();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Vault
                    && self.state.vault_ui.mode == VaultMode::Select
                {
                    self.vaults_select_prev();
                }
            }
            crate::ui::keymap::AppAction::Down => {
                if self.state.screen == Screen::Home && self.state.section == Section::Home {
                    self.home_feed_select_next();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Members
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
                    && self.state.section == Section::Wallets
                    && matches!(
                        self.state.wallets.mode,
                        WalletsMode::List | WalletsMode::Detail
                    )
                {
                    self.wallets_select_next();
                    if self.state.wallets.mode == WalletsMode::Detail {
                        self.open_wallet_detail().await?;
                    }
                } else if self.state.screen == Screen::Home && self.state.section == Section::Flows
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
                    && self.state.section == Section::Categories
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
                    && self.state.section == Section::Vault
                    && self.state.vault_ui.mode == VaultMode::Defaults
                {
                    self.defaults_select_next();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Vault
                    && self.state.vault_ui.mode == VaultMode::Select
                {
                    self.vaults_select_next();
                }
            }
            crate::ui::keymap::AppAction::Left => {
                if self.state.screen == Screen::Home {
                    if self.state.section == Section::Flows {
                        self.accounts_prev_tab();
                    } else if self.state.section == Section::Stats {
                        self.stats_prev_tab();
                    }
                }
            }
            crate::ui::keymap::AppAction::Right => {
                if self.state.screen == Screen::Home {
                    if self.state.section == Section::Flows {
                        self.accounts_next_tab();
                    } else if self.state.section == Section::Stats {
                        self.stats_next_tab();
                    }
                }
            }
            crate::ui::keymap::AppAction::Input(ch) => {
                if self.state.screen == Screen::Login {
                    let field = self.active_field_mut();
                    field.push(ch);
                } else {
                    if self.state.section == Section::Categories
                        && self.state.categories.mode == CategoriesMode::Aliases
                        && self.state.categories.aliases.focus == AliasFocus::Input
                    {
                        self.state.categories.aliases.input.push(ch);
                        return Ok(());
                    }
                    if self.state.section == Section::Members
                        && self.handle_members_input(ch).await?
                    {
                        return Ok(());
                    }
                    if self.handle_search_input(ch).await? {
                        return Ok(());
                    } else if self.state.section == Section::Transactions
                        && matches!(
                            self.state.transactions.mode,
                            TransactionsMode::Form | TransactionsMode::Edit
                        )
                    {
                        self.handle_transaction_form_input(ch);
                        return Ok(());
                    } else if self.state.section == Section::Transactions
                        && matches!(
                            self.state.transactions.mode,
                            TransactionsMode::TransferWallet | TransactionsMode::TransferFlow
                        )
                    {
                        match self.state.transactions.transfer.focus {
                            TransferField::Amount => {
                                self.state.transactions.transfer.amount.push(ch);
                                return Ok(());
                            }
                            TransferField::Note => {
                                self.state.transactions.transfer.note.push(ch);
                                return Ok(());
                            }
                            TransferField::OccurredAt => {
                                self.state.transactions.transfer.occurred_at.push(ch);
                                return Ok(());
                            }
                            _ => {}
                        }
                    } else if self.state.section == Section::Transactions
                        && self.state.transactions.mode == TransactionsMode::Filter
                    {
                        self.handle_filter_input(ch);
                        return Ok(());
                    } else if self.state.section == Section::Transactions
                        && self.state.transactions.mode == TransactionsMode::List
                        && self.state.transactions.quick_active
                    {
                        self.state.transactions.quick_input.push(ch);
                        return Ok(());
                    } else if self.handle_form_input(ch) {
                        return Ok(());
                    } else {
                        self.handle_non_login_key(ch).await?;
                    }
                }
            }
            crate::ui::keymap::AppAction::None => {}
        }

        Ok(())
    }
}
