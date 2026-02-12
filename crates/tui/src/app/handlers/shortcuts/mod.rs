//! Keyboard shortcut handlers organized by category.
//!
//! This module splits the large `handle_non_login_key` function into smaller,
//! focused modules for better maintainability.

mod editing;
mod navigation;
mod other;
mod pagination;
mod transactions;

use super::super::*;

use crate::error::Result;

impl App {
    /// Routes keyboard shortcuts to appropriate handlers.
    ///
    /// This is the main entry point for non-login keyboard handling.
    pub(crate) async fn handle_non_login_key(&mut self, ch: char) -> Result<()> {
        match ch {
            // Quit
            'q' => {
                self.should_quit = true;
                Ok(())
            }
            // Main section navigation
            'h' | 'H' | 't' | 'a' | 'y' | 'Y' | 's' | 'S' => {
                self.handle_navigation_shortcut(ch).await
            }
            // Toggle archived in Accounts
            'A' => self.handle_navigation_shortcut(ch).await,
            // Transaction-related shortcuts
            'i' | 'I' | 'R' | 'n' | 'N' | 'v' | 'V' | 'z' | 'Z' | 'T' | 'g' | 'G' | 'r' => {
                self.handle_transaction_shortcut(ch).await
            }
            // Editing and creation shortcuts
            'e' | 'E' | 'b' | 'B' | 'c' | 'C' | 'l' | 'L' | 'x' | 'X' | 'm' | 'M' | 'd' | 'D' | 'u' | 'U' => {
                self.handle_editing_shortcut(ch).await
            }
            // Pagination and tab selection
            ']' | '[' | '1' | '2' | '3' | '4' | 'j' | 'J' | 'k' | 'K' => {
                self.handle_pagination_shortcut(ch).await
            }
            // Other shortcuts (filter, help, undo, etc.)
            '/' | '?' | ' ' | 'w' | 'W' | 'f' | 'F' => {
                self.handle_other_shortcut(ch).await
            }
            _ => Ok(()),
        }
    }
}
