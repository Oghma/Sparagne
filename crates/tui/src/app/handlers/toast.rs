use super::super::*;

use std::time::Duration;

use crate::{client::ClientError, error::Result};
use api_types::error::ErrorCode;

impl App {
    pub(crate) async fn expire_toast(&mut self) -> Result<()> {
        let Some(toast) = self.state.toast.clone() else {
            return Ok(());
        };
        if std::time::Instant::now() < toast.expires_at {
            return Ok(());
        }
        if let Some(action) = toast.undo_action {
            self.apply_undo_expired(action).await?;
        }
        self.state.toast = None;
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
        self.state.login.message = Some("Credenziali errate o pairing mancante.".to_string());
        self.state.vault = None;
        self.state.snapshot = None;
        self.state.section = Section::Home;
        self.state.transactions = TransactionsState::default();
        self.state.overlays = OverlayState::default();
        true
    }

    pub(crate) async fn finalize_pending_undo(&mut self) -> Result<()> {
        let Some(toast) = self.state.toast.clone() else {
            return Ok(());
        };
        if toast.level != ToastLevel::Undo {
            return Ok(());
        }
        if let Some(action) = toast.undo_action {
            self.apply_undo_expired(action).await?;
        }
        self.state.toast = None;
        Ok(())
    }

    pub(crate) async fn handle_undo_hotkey(&mut self) -> Result<bool> {
        let Some(toast) = self.state.toast.clone() else {
            return Ok(false);
        };
        if toast.level != ToastLevel::Undo {
            return Ok(false);
        }
        let Some(action) = toast.undo_action else {
            return Ok(false);
        };
        self.state.toast = None;
        self.apply_undo_action(action).await?;
        Ok(true)
    }

    async fn apply_undo_action(&mut self, action: UndoAction) -> Result<()> {
        match action {
            UndoAction::TransactionVoid { ids } => {
                for id in ids {
                    self.state.transactions.pending_delete_ids.remove(&id);
                }
                self.set_toast("Undo applied.", ToastLevel::Success);
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
