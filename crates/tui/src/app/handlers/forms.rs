use super::super::*;

impl App {
    pub(crate) fn advance_focus(&mut self) {
        if self.state.screen == Screen::Login {
            self.state.login.focus = match self.state.login.focus {
                LoginField::Username => LoginField::Password,
                LoginField::Password => LoginField::Username,
            };
            return;
        }

        if self.state.section == Section::Transactions
            && matches!(
                self.state.transactions.mode,
                TransactionsMode::Form | TransactionsMode::Edit
            )
        {
            self.advance_transaction_form_focus();
            return;
        }

        if self.state.section == Section::Transactions
            && matches!(
                self.state.transactions.mode,
                TransactionsMode::TransferWallet | TransactionsMode::TransferFlow
            )
        {
            self.advance_transfer_focus();
            return;
        }
        if self.state.section == Section::Transactions
            && self.state.transactions.mode == TransactionsMode::Filter
        {
            self.advance_filter_focus();
            return;
        }
        if self.state.section == Section::Settings
            && self.state.settings_tab == SettingsTab::Categories
            && self.state.categories.mode == CategoriesMode::Aliases
        {
            self.toggle_alias_focus();
            return;
        }
        if self.state.section == Section::Settings
            && self.state.settings_tab == SettingsTab::Members
            && self.state.members.mode == MembersMode::Form
        {
            self.state.members.form.focus = match self.state.members.form.focus {
                MemberFormField::Username => MemberFormField::Role,
                MemberFormField::Role => MemberFormField::Username,
            };
            self.state.members.form.update_focus();
            return;
        }

        match self.state.section {
            Section::Accounts => match self.state.accounts_tab {
                AccountsTab::Sources => self.advance_wallet_focus(),
                AccountsTab::Envelopes => self.advance_flow_focus(),
                AccountsTab::Goals => {}
            },
            Section::Settings if self.state.settings_tab == SettingsTab::Vault => {
                self.advance_vault_focus()
            }
            _ => {}
        }
    }

    pub(crate) fn active_field_mut(&mut self) -> &mut String {
        match self.state.login.focus {
            LoginField::Username => &mut self.state.login.username,
            LoginField::Password => &mut self.state.login.password,
        }
    }

    pub(crate) fn advance_wallet_focus(&mut self) {
        if !matches!(
            self.state.wallets.mode,
            EntityListMode::Create | EntityListMode::Rename
        ) {
            return;
        }

        if self.state.wallets.mode == EntityListMode::Rename {
            self.state.wallets.form.focus = WalletFormField::Name;
            self.state.wallets.form.update_focus();
            return;
        }

        self.state.wallets.form.focus = match self.state.wallets.form.focus {
            WalletFormField::Name => WalletFormField::Opening,
            WalletFormField::Opening => WalletFormField::Name,
        };
        self.state.wallets.form.update_focus();
    }

    pub(crate) fn advance_flow_focus(&mut self) {
        if !matches!(self.state.flows.mode, EntityListMode::Create | EntityListMode::Rename) {
            return;
        }

        if self.state.flows.mode == EntityListMode::Rename {
            self.state.flows.form.focus = FlowFormField::Name;
            self.state.flows.form.update_focus();
            return;
        }

        self.state.flows.form.focus = match self.state.flows.form.focus {
            FlowFormField::Name => FlowFormField::Mode,
            FlowFormField::Mode => FlowFormField::Cap,
            FlowFormField::Cap => FlowFormField::Opening,
            FlowFormField::Opening => FlowFormField::Name,
        };
        self.state.flows.form.update_focus();
    }

    pub(crate) fn advance_vault_focus(&mut self) {
        match self.state.vault_ui.mode {
            VaultMode::Create => {
                self.state.vault_ui.form.error = None;
            }
            VaultMode::Defaults => {
                self.state.vault_ui.defaults.error = None;
                self.state.vault_ui.defaults.focus = match self.state.vault_ui.defaults.focus {
                    DefaultsField::Wallet => DefaultsField::Flow,
                    DefaultsField::Flow => DefaultsField::Wallet,
                };
            }
            VaultMode::Select => {}
            VaultMode::View => {}
        }
    }

    pub(crate) fn advance_transfer_focus(&mut self) {
        let transfer = &mut self.state.transactions.transfer;
        transfer.focus = match transfer.focus {
            TransferField::From => TransferField::To,
            TransferField::To => TransferField::Amount,
            TransferField::Amount => TransferField::Note,
            TransferField::Note => TransferField::OccurredAt,
            TransferField::OccurredAt => TransferField::From,
        };
    }
    pub(crate) fn handle_form_input(&mut self, ch: char) -> bool {
        match self.state.section {
            Section::Accounts => match self.state.accounts_tab {
                AccountsTab::Sources => {
                    if matches!(
                        self.state.wallets.mode,
                        EntityListMode::Create | EntityListMode::Rename
                    ) {
                        match self.state.wallets.form.focus {
                            WalletFormField::Name => self.state.wallets.form.name.push(ch),
                            WalletFormField::Opening => self.state.wallets.form.opening.push(ch),
                        }
                        return true;
                    }
                }
                AccountsTab::Envelopes => {
                    if matches!(self.state.flows.mode, EntityListMode::Create | EntityListMode::Rename) {
                        match self.state.flows.form.focus {
                            FlowFormField::Name => self.state.flows.form.name.push(ch),
                            FlowFormField::Cap => self.state.flows.form.cap.push(ch),
                            FlowFormField::Opening => self.state.flows.form.opening.push(ch),
                            FlowFormField::Mode => {
                                if matches!(ch, 'm' | 'M' | ' ') {
                                    self.cycle_flow_mode();
                                }
                                return true;
                            }
                        }
                        return true;
                    }
                }
                AccountsTab::Goals => {}
            },
            Section::Settings => match self.state.settings_tab {
                SettingsTab::Categories => {
                    if matches!(
                        self.state.categories.mode,
                        CategoriesMode::Create | CategoriesMode::Rename
                    ) {
                        self.state.categories.form.name.push(ch);
                        return true;
                    }
                }
                SettingsTab::Members => {
                    if self.state.members.mode == MembersMode::Form {
                        match self.state.members.form.focus {
                            MemberFormField::Username => self.state.members.form.username.push(ch),
                            MemberFormField::Role => {
                                if ch == ' ' {
                                    self.cycle_member_role(true);
                                }
                                return true;
                            }
                        }
                        return true;
                    }
                }
                SettingsTab::Vault => {
                    if self.state.vault_ui.mode == VaultMode::Create {
                        self.state.vault_ui.form.name.push(ch);
                        return true;
                    }
                }
                SettingsTab::Preferences => {
                    // Preferences tab handles input through dedicated handlers
                }
            },
            _ => {}
        }
        false
    }
    pub(crate) fn advance_filter_focus(&mut self) {
        let filter = &mut self.state.transactions.filter;
        filter.focus = match filter.focus {
            FilterField::From => FilterField::To,
            FilterField::To => FilterField::Kinds,
            FilterField::Kinds => FilterField::From,
        };
    }

    pub(crate) fn handle_filter_input(&mut self, ch: char) {
        let filter = &mut self.state.transactions.filter;
        match filter.focus {
            FilterField::From => {
                filter.from_input.push(ch);
            }
            FilterField::To => {
                filter.to_input.push(ch);
            }
            FilterField::Kinds => match ch {
                'i' | 'I' => filter.kind_income = !filter.kind_income,
                'e' | 'E' => filter.kind_expense = !filter.kind_expense,
                'r' | 'R' => filter.kind_refund = !filter.kind_refund,
                'w' | 'W' => filter.kind_transfer_wallet = !filter.kind_transfer_wallet,
                'f' | 'F' => filter.kind_transfer_flow = !filter.kind_transfer_flow,
                _ => {}
            },
        }
    }
    pub(crate) fn backspace_wallet_form(&mut self) {
        if !matches!(
            self.state.wallets.mode,
            EntityListMode::Create | EntityListMode::Rename
        ) {
            return;
        }
        match self.state.wallets.form.focus {
            WalletFormField::Name => {
                self.state.wallets.form.name.pop();
            }
            WalletFormField::Opening => {
                self.state.wallets.form.opening.pop();
            }
        }
    }

    pub(crate) fn backspace_flow_form(&mut self) {
        if !matches!(self.state.flows.mode, EntityListMode::Create | EntityListMode::Rename) {
            return;
        }
        match self.state.flows.form.focus {
            FlowFormField::Name => {
                self.state.flows.form.name.pop();
            }
            FlowFormField::Cap => {
                self.state.flows.form.cap.pop();
            }
            FlowFormField::Opening => {
                self.state.flows.form.opening.pop();
            }
            FlowFormField::Mode => {}
        }
    }

    pub(crate) fn backspace_vault_form(&mut self) {
        if self.state.vault_ui.mode != VaultMode::Create {
            return;
        }
        self.state.vault_ui.form.name.pop();
    }

    pub(crate) fn backspace_category_form(&mut self) {
        if matches!(
            self.state.categories.mode,
            CategoriesMode::Create | CategoriesMode::Rename
        ) {
            self.state.categories.form.name.pop();
        }
    }

    pub(crate) fn reset_wallet_form(&mut self) {
        self.state.wallets.reset_form();
    }

    pub(crate) fn reset_flow_form(&mut self) {
        self.state.flows.reset_form();
    }

    pub(crate) fn reset_vault_form(&mut self) {
        self.state.vault_ui.form = VaultFormState::default();
        self.state.vault_ui.error = None;
    }

    pub(crate) fn reset_category_form(&mut self) {
        self.state.categories.reset_form();
    }

    pub(crate) fn reset_member_form(&mut self) {
        self.state.members.reset_form();
    }

    pub(crate) fn reset_category_aliases(&mut self) {
        self.state.categories.aliases = CategoryAliasState::default();
    }
}
