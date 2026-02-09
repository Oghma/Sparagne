//! Home section dispatch handling.

use crate::{app::App, error::Result, ui::keymap::AppAction};

impl App {
    /// Dispatches actions for the Home section (activity feed).
    pub(crate) async fn dispatch_home(&mut self, action: AppAction) -> Result<bool> {
        match action {
            AppAction::Submit => {
                self.open_home_feed_item().await?;
                Ok(true)
            }
            AppAction::Up => {
                self.home_feed_select_prev();
                Ok(true)
            }
            AppAction::Down => {
                self.home_feed_select_next();
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
