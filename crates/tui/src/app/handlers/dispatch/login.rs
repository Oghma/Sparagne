//! Login screen dispatch handling.

use crate::app::App;
use crate::error::Result;
use crate::ui::keymap::AppAction;

impl App {
    /// Handles actions on the login screen.
    pub(crate) async fn dispatch_login(&mut self, action: AppAction) -> Result<()> {
        match action {
            AppAction::Submit => {
                self.attempt_login().await?;
            }
            AppAction::Backspace => {
                let field = self.active_field_mut();
                field.pop();
            }
            AppAction::Input(ch) => {
                let field = self.active_field_mut();
                field.push(ch);
            }
            AppAction::NextField => {
                self.advance_focus();
            }
            _ => {}
        }
        Ok(())
    }
}
