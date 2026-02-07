//! Input character routing.
//!
//! This module handles routing of character input to appropriate handlers
//! using a chain of responsibility pattern.

use super::super::*;

use crate::error::Result;

impl App {
    /// Routes input characters to the appropriate handler.
    ///
    /// Returns `true` if the input was handled by a form or specific context,
    /// `false` if it should be passed to the shortcut handler.
    pub(crate) async fn route_input(&mut self, ch: char) -> Result<bool> {
        // Category aliases input
        if self.try_category_aliases_input(ch) {
            return Ok(true);
        }

        // Members section input
        if self.is_settings_tab(SettingsTab::Members) && self.handle_members_input(ch).await? {
            return Ok(true);
        }

        // Preferences toggle
        if self.is_settings_tab(SettingsTab::Preferences) && ch == ' ' {
            self.handle_preferences_toggle();
            return Ok(true);
        }

        // Search input
        if self.handle_search_input(ch).await? {
            return Ok(true);
        }

        // Transaction form input
        if self.try_transaction_form_input(ch) {
            return Ok(true);
        }

        // Transfer form input
        if self.try_transfer_form_input(ch) {
            return Ok(true);
        }

        // Filter input
        if self.try_filter_input(ch) {
            return Ok(true);
        }

        // Quick add input
        if self.try_quick_add_input(ch) {
            return Ok(true);
        }

        // Generic form input (wallets, flows, categories, members, vault)
        if self.handle_form_input(ch) {
            return Ok(true);
        }

        Ok(false)
    }

    /// Handles category aliases input field.
    fn try_category_aliases_input(&mut self, ch: char) -> bool {
        if self.is_settings_tab(SettingsTab::Categories)
            && self.state.categories.mode == CategoriesMode::Aliases
            && self.state.categories.aliases.focus == AliasFocus::Input
        {
            self.state.categories.aliases.input.push(ch);
            return true;
        }
        false
    }

    /// Handles transaction form input.
    fn try_transaction_form_input(&mut self, ch: char) -> bool {
        if self.state.section == Section::Transactions
            && matches!(
                self.state.transactions.mode,
                TransactionsMode::Form | TransactionsMode::Edit
            )
        {
            self.handle_transaction_form_input(ch);
            return true;
        }
        false
    }

    /// Handles transfer form input.
    fn try_transfer_form_input(&mut self, ch: char) -> bool {
        if self.state.section == Section::Transactions
            && matches!(
                self.state.transactions.mode,
                TransactionsMode::TransferWallet | TransactionsMode::TransferFlow
            )
        {
            match self.state.transactions.transfer.focus {
                TransferField::Amount => {
                    self.state.transactions.transfer.amount.push(ch);
                    return true;
                }
                TransferField::Note => {
                    self.state.transactions.transfer.note.push(ch);
                    return true;
                }
                TransferField::OccurredAt => {
                    self.state.transactions.transfer.occurred_at.push(ch);
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Handles filter input.
    fn try_filter_input(&mut self, ch: char) -> bool {
        if self.state.section == Section::Transactions
            && self.state.transactions.mode == TransactionsMode::Filter
        {
            self.handle_filter_input(ch);
            return true;
        }
        false
    }

    /// Handles quick add input.
    fn try_quick_add_input(&mut self, ch: char) -> bool {
        if self.state.section == Section::Transactions
            && self.state.transactions.mode == TransactionsMode::List
            && self.state.transactions.quick_active
        {
            self.state.transactions.quick_input.push(ch);
            return true;
        }
        false
    }
}
