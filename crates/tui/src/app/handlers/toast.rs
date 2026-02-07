use super::super::*;

use std::time::Duration;

use crate::{app::errors::login_message_for_error, client::ClientError, error::Result};
use api_types::error::ErrorCode;

impl App {
    pub(crate) async fn expire_toast(&mut self) -> Result<()> {
        let Some(toast) = self.state.toast.take() else {
            return Ok(());
        };
        if std::time::Instant::now() < toast.expires_at {
            self.state.toast = Some(toast);
            return Ok(());
        }
        if let Some(action) = toast.undo_action {
            self.apply_undo_expired(action).await?;
        }
        Ok(())
    }

    pub(crate) fn set_toast(&mut self, message: &str, level: ToastLevel) {
        let now = std::time::Instant::now();
        self.state.toast = Some(ToastState {
            message: message.to_string(),
            level,
            created_at: now,
            expires_at: now + Duration::from_secs(3),
            undo_action: None,
        });
    }

    pub(crate) fn set_undo_toast(&mut self, message: &str, action: UndoAction) {
        let now = std::time::Instant::now();
        let undo_secs = self.state.undo_toast_secs.max(1);
        self.state.toast = Some(ToastState {
            message: message.to_string(),
            level: ToastLevel::Undo,
            created_at: now,
            expires_at: now + Duration::from_secs(undo_secs),
            undo_action: Some(action),
        });
    }

    pub(crate) fn connection_ok(&mut self, message: Option<&str>) {
        self.state.connection.ok = true;
        self.state.connection.message = message.map(|msg| msg.to_string());
        self.state.last_refresh = Some(self.now_in_timezone());
    }

    pub(crate) fn connection_error(&mut self, message: &str) {
        self.state.connection.ok = false;
        self.state.connection.message = Some(message.to_string());
    }

    pub(crate) fn handle_auth_error(&mut self, err: &ClientError) -> bool {
        match err {
            ClientError::Unauthorized => {}
            ClientError::Forbidden(payload) if payload.code == ErrorCode::Forbidden => {}
            _ => return false,
        }

        self.state.screen = Screen::Login;
        self.state.login.password.clear();
        self.state.login.message =
            Some(t(self.state.locale, TextKey::ErrorInvalidCredentials).to_string());
        self.state.vault = None;
        self.state.snapshot = None;
        self.state.section = Section::Home;
        self.state.transactions = TransactionsState::default();
        self.state.overlays = OverlayState::default();
        true
    }

    /// Handles an API client error: checks for auth errors (redirecting to
    /// login) and returns a localized error message for non-auth errors.
    ///
    /// Returns `None` when the error was an auth error (already handled),
    /// or `Some(message)` with the localized error string.
    pub(crate) fn client_error_message(&mut self, err: ClientError) -> Option<String> {
        if self.handle_auth_error(&err) {
            return None;
        }
        Some(login_message_for_error(err, self.state.locale))
    }

    /// Handle an API error for data-loading calls: checks auth, shows
    /// connection error indicator. Returns `Some(msg)` for the caller to
    /// assign to an error field, or `None` if it was an auth error (already
    /// handled).
    pub(crate) fn on_api_error_connection(&mut self, err: ClientError) -> Option<String> {
        let msg = self.client_error_message(err)?;
        self.connection_error(t(self.state.locale, TextKey::ErrorConnection));
        Some(msg)
    }

    /// Handle an API error for mutation calls: checks auth, shows an error
    /// toast with the given key. Returns `Some(msg)` for the caller to
    /// assign to an error field, or `None` if it was an auth error (already
    /// handled).
    pub(crate) fn on_api_error_toast(
        &mut self,
        err: ClientError,
        toast_key: TextKey,
    ) -> Option<String> {
        let msg = self.client_error_message(err)?;
        self.set_toast(t(self.state.locale, toast_key), ToastLevel::Error);
        Some(msg)
    }

    pub(crate) async fn finalize_pending_undo(&mut self) -> Result<()> {
        let Some(toast) = self.state.toast.take() else {
            return Ok(());
        };
        if toast.level != ToastLevel::Undo {
            self.state.toast = Some(toast);
            return Ok(());
        }
        if let Some(action) = toast.undo_action {
            self.apply_undo_expired(action).await?;
        }
        Ok(())
    }

    pub(crate) async fn handle_undo_hotkey(&mut self) -> Result<bool> {
        let is_undo = self
            .state
            .toast
            .as_ref()
            .is_some_and(|t| t.level == ToastLevel::Undo && t.undo_action.is_some());
        if !is_undo {
            return Ok(false);
        }
        // Safety: we just verified toast is Some with an undo action.
        let toast = self.state.toast.take();
        let action = toast.and_then(|t| t.undo_action);
        if let Some(action) = action {
            self.apply_undo_action(action).await?;
        }
        Ok(true)
    }

    async fn apply_undo_action(&mut self, action: UndoAction) -> Result<()> {
        match action {
            UndoAction::TransactionVoid { ids } => {
                for id in ids {
                    self.state.transactions.pending_delete_ids.remove(&id);
                }
                self.set_toast(
                    t(self.state.locale, TextKey::UiUndoApplied),
                    ToastLevel::Success,
                );
            }
            UndoAction::WalletArchive { id } => {
                self.undo_wallet_archive(id).await?;
            }
            UndoAction::FlowArchive { id } => {
                self.undo_flow_archive(id).await?;
            }
        }
        Ok(())
    }

    async fn apply_undo_expired(&mut self, action: UndoAction) -> Result<()> {
        match action {
            UndoAction::TransactionVoid { ids } => {
                self.void_transactions_by_ids(&ids, None).await?;
                for id in ids {
                    self.state.transactions.pending_delete_ids.remove(&id);
                }
            }
            UndoAction::WalletArchive { .. } | UndoAction::FlowArchive { .. } => {}
        }
        Ok(())
    }
}
