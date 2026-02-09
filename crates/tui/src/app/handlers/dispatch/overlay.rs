//! Overlay dispatch handling.
//!
//! Handles actions when modal overlays are active (confirm dialogs, error
//! dialogs, bulk category, grouping, help, palette, global search).

use crate::{app::App, error::Result, ui::keymap::AppAction};

impl App {
    /// Checks if an overlay is active and handles the action if so.
    ///
    /// Returns `Ok(true)` if an overlay consumed the action, `Ok(false)`
    /// otherwise.
    pub(crate) async fn dispatch_overlay(&mut self, action: AppAction) -> Result<bool> {
        // Modal dialogs take priority
        if self.state.overlays.has_confirm_dialog() {
            self.handle_confirm_action(action).await?;
            return Ok(true);
        }
        if self.state.overlays.error.is_some() {
            self.handle_error_action(action).await?;
            return Ok(true);
        }
        if self.state.overlays.bulk_category.is_some() {
            self.handle_bulk_category_action(action).await?;
            return Ok(true);
        }
        if self.state.overlays.grouping.is_some() {
            self.handle_grouping_action(action).await?;
            return Ok(true);
        }

        // Global overlays
        if self.state.help.active {
            self.handle_help_action(action);
            return Ok(true);
        }
        if self.state.palette.active {
            self.handle_palette_action(action).await?;
            return Ok(true);
        }
        if self.state.global_search.active {
            self.handle_global_search_action(action).await?;
            return Ok(true);
        }

        Ok(false)
    }
}
