//! Miscellaneous shortcuts (filter, help, undo, etc.).

use super::super::super::*;

use crate::error::Result;

impl App {
    /// Handles miscellaneous shortcuts.
    pub(crate) async fn handle_other_shortcut(&mut self, ch: char) -> Result<()> {
        match ch {
            // Filter
            '/' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.open_filter();
                }
            }
            // Help
            '?' => {
                if self.state.screen == Screen::Home {
                    self.state.help.active = true;
                }
            }
            // Undo
            'u' | 'U' => {
                if self.handle_undo_hotkey().await? {
                    return Ok(());
                }
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.undo_last_transaction().await?;
                }
            }
            // Space for visual selection
            ' ' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                    && self.state.transactions.visual_mode
                {
                    self.toggle_visual_selection();
                }
            }
            // Wallet picker (legacy shortcut)
            'w' | 'W' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.open_wallet_picker();
                }
            }
            // Flow picker (legacy shortcut)
            'f' | 'F' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.open_flow_picker();
                }
            }
            _ => {}
        }
        Ok(())
    }
}
