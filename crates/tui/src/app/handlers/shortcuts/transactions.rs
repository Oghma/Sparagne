//! Transaction-related shortcuts.

use super::super::super::*;

use crate::error::Result;
use api_types::transaction::TransactionKind;

impl App {
    /// Handles transaction-related shortcuts.
    pub(crate) async fn handle_transaction_shortcut(&mut self, ch: char) -> Result<()> {
        match ch {
            // Income
            'i' | 'I' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.start_transaction_form(TransactionKind::Income).await?;
                }
            }
            // Refund
            'R' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.start_transaction_form(TransactionKind::Refund).await?;
                }
            }
            // Refresh / repeat
            'r' => {
                if self.state.section == Section::Transactions {
                    if self.state.transactions.mode == TransactionsMode::Detail {
                        self.repeat_transaction().await?;
                    } else if self.state.transactions.mode == TransactionsMode::List {
                        self.load_transactions(true).await?;
                    }
                } else if self.state.section == Section::Analytics {
                    self.load_stats().await?;
                } else if self.state.section == Section::Accounts {
                    self.refresh_snapshot().await?;
                } else if self.is_settings_tab(SettingsTab::Categories) {
                    if self.state.categories.mode == CategoriesMode::Aliases {
                        self.reload_category_aliases().await?;
                    } else {
                        self.load_categories().await?;
                    }
                } else if self.is_settings_tab(SettingsTab::Members) {
                    self.load_members().await?;
                }
            }
            // Quick add (inline)
            'n' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.state.transactions.quick_active = true;
                    self.state.transactions.quick_error = None;
                } else if self.state.section == Section::Home {
                    self.state.section = Section::Transactions;
                    self.state.transactions.mode = TransactionsMode::List;
                    if self.state.transactions.items.is_empty() {
                        self.load_transactions(true).await?;
                    }
                    self.state.transactions.quick_active = true;
                    self.state.transactions.quick_error = None;
                }
            }
            // New expense form (modal)
            'N' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.start_transaction_form(TransactionKind::Expense)
                        .await?;
                } else if self.state.section == Section::Home {
                    self.state.section = Section::Transactions;
                    self.state.transactions.mode = TransactionsMode::List;
                    if self.state.transactions.items.is_empty() {
                        self.load_transactions(true).await?;
                    }
                    self.start_transaction_form(TransactionKind::Expense)
                        .await?;
                }
            }
            // Visual mode / void transaction
            'v' | 'V' => {
                if self.state.section == Section::Transactions {
                    match self.state.transactions.mode {
                        TransactionsMode::List => self.toggle_visual_mode(),
                        TransactionsMode::Detail => {
                            self.void_transaction().await?;
                        }
                        _ => {}
                    }
                }
            }
            // Toggle voided visibility
            'z' | 'Z' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.state.transactions.include_voided =
                        !self.state.transactions.include_voided;
                    self.load_transactions(true).await?;
                }
            }
            // Transfer picker
            'T' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.open_transfer_picker();
                }
            }
            // Grouping dialog
            'g' | 'G' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.open_grouping_dialog();
                }
            }
            _ => {}
        }
        Ok(())
    }
}
