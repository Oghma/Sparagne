//! Handler dispatch coordination.
//!
//! This module coordinates input handling by routing actions to
//! section-specific handlers. The main dispatcher in `handlers/mod.rs` now
//! delegates here for cleaner separation.

mod accounts;
mod analytics;
mod home;
mod login;
mod overlay;
mod settings;
mod transactions;

use crate::{
    app::{App, state::Section},
    error::Result,
    ui::keymap::AppAction,
};

impl App {
    /// Routes an action to the appropriate section handler.
    ///
    /// Returns `true` if the action was handled by a section dispatcher.
    pub(crate) async fn dispatch_section_action(&mut self, action: AppAction) -> Result<bool> {
        match self.state.section {
            Section::Home => self.dispatch_home(action).await,
            Section::Transactions => self.dispatch_transactions(action).await,
            Section::Accounts => self.dispatch_accounts(action).await,
            Section::Analytics => self.dispatch_analytics(action).await,
            Section::Settings => self.dispatch_settings(action).await,
        }
    }
}
