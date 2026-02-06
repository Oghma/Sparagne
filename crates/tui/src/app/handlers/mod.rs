//! Input handling for the TUI application.
//!
//! This module coordinates keyboard input routing through a layered dispatch system:
//! 1. Overlays (modals, dialogs) - highest priority
//! 2. Global actions (quit, palette, search)
//! 3. Section-specific handlers (home, transactions, accounts, settings, analytics)
//! 4. Input routing for forms and search

mod accounts;
mod cancel;
mod categories;
mod dispatch;
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
use crate::ui::keymap::AppAction;

use super::state::{AccountsTab, EntityListMode, Screen, Section, SettingsTab, TransactionsMode};
use super::App;

impl App {
    /// Checks if we are in Settings section showing a specific sub-tab.
    fn is_settings_tab(&self, tab: SettingsTab) -> bool {
        self.state.section == Section::Settings && self.state.settings_tab == tab
    }

    /// Main entry point for keyboard event handling.
    #[doc(hidden)]
    pub async fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        let action = crate::ui::keymap::map_key(key);

        // 1. Check overlays first (modal dialogs, help, palette, global search)
        if self.dispatch_overlay(action).await? {
            return Ok(());
        }

        // 2. Handle global actions available from any screen
        match action {
            AppAction::Quit => {
                self.should_quit = true;
                return Ok(());
            }
            AppAction::Cancel => {
                self.handle_cancel().await?;
                return Ok(());
            }
            AppAction::NextField => {
                self.advance_focus();
                return Ok(());
            }
            AppAction::CycleAmbiguous => {
                self.cycle_quick_add_ambiguous();
                return Ok(());
            }
            _ => {}
        }

        // 3. Handle login screen separately
        if self.state.screen == Screen::Login {
            self.dispatch_login(action).await?;
            return Ok(());
        }

        // 4. Handle global toggles (palette, search) on Home screen
        if self.state.screen == Screen::Home && self.handle_global_toggles(action) {
            return Ok(());
        }

        // 5. Route to section-specific handlers
        if self.state.screen == Screen::Home && self.dispatch_section_action(action).await? {
            return Ok(());
        }

        // 6. Handle input characters
        if let AppAction::Input(ch) = action && !self.route_input(ch).await? {
            self.handle_non_login_key(ch).await?;
        }

        Ok(())
    }

    /// Handles global toggle actions (palette, search).
    fn handle_global_toggles(&mut self, action: AppAction) -> bool {
        match action {
            AppAction::TogglePalette => {
                self.open_palette();
                true
            }
            AppAction::Search => {
                self.handle_search_action();
                true
            }
            _ => false,
        }
    }

    /// Routes search action based on current section and mode.
    fn handle_search_action(&mut self) {
        match self.state.section {
            Section::Transactions if self.state.transactions.mode == TransactionsMode::List => {
                self.start_search();
            }
            Section::Accounts => match self.state.accounts_tab {
                AccountsTab::Sources if self.state.wallets.mode == EntityListMode::List => {
                    self.start_search();
                }
                AccountsTab::Envelopes | AccountsTab::Goals
                    if self.state.flows.mode == EntityListMode::List =>
                {
                    self.start_search();
                }
                _ => self.open_global_search(),
            },
            _ => self.open_global_search(),
        }
    }
}
