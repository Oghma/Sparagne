//! Pagination and tab selection shortcuts.

use super::super::super::*;

use crate::error::Result;

impl App {
    /// Handles pagination and tab selection shortcuts.
    pub(crate) async fn handle_pagination_shortcut(&mut self, ch: char) -> Result<()> {
        match ch {
            // Next page / next month
            ']' => {
                if self.state.section == Section::Transactions {
                    self.load_transactions_next().await?;
                } else if self.state.section == Section::Analytics {
                    self.stats_next_month();
                }
            }
            // Previous page / previous month
            '[' => {
                if self.state.section == Section::Transactions {
                    self.load_transactions_prev().await?;
                } else if self.state.section == Section::Analytics {
                    self.stats_prev_month();
                }
            }
            // Tab 1 / wallet picker
            '1' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.open_wallet_picker();
                } else if self.state.section == Section::Analytics {
                    self.stats_set_tab(0);
                } else if self.state.section == Section::Settings {
                    self.settings_set_tab(0);
                }
            }
            // Tab 2 / flow picker
            '2' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.open_flow_picker();
                } else if self.state.section == Section::Analytics {
                    self.stats_set_tab(1);
                } else if self.state.section == Section::Settings {
                    self.settings_set_tab(1);
                }
            }
            // Tab 3
            '3' => {
                if self.state.section == Section::Analytics {
                    self.stats_set_tab(2);
                } else if self.state.section == Section::Settings {
                    self.settings_set_tab(2);
                }
            }
            // Tab 4
            '4' => {
                if self.state.section == Section::Settings {
                    self.settings_set_tab(3);
                }
            }
            // j/k navigation (vim-style)
            'j' | 'J' => {
                if self.state.section == Section::Transactions {
                    self.state.transactions.select_next();
                } else if self.state.section == Section::Home {
                    self.home_feed_select_next();
                }
            }
            'k' | 'K' => {
                if self.state.section == Section::Transactions {
                    self.state.transactions.select_prev();
                } else if self.state.section == Section::Home {
                    self.home_feed_select_prev();
                }
            }
            _ => {}
        }
        Ok(())
    }
}
