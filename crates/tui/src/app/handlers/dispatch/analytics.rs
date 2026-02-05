//! Analytics section dispatch handling.

use crate::app::App;
use crate::error::Result;
use crate::ui::keymap::AppAction;

impl App {
    /// Dispatches actions for the Analytics section.
    pub(crate) async fn dispatch_analytics(&mut self, action: AppAction) -> Result<bool> {
        match action {
            AppAction::Submit => {
                self.load_stats().await?;
                Ok(true)
            }
            AppAction::Left => {
                self.stats_prev_tab();
                Ok(true)
            }
            AppAction::Right => {
                self.stats_next_tab();
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
