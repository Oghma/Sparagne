use super::super::*;

use std::time::Duration;

use crate::client::ClientError;
use api_types::error::ErrorCode;

impl App {
    pub(crate) fn expire_toast(&mut self) {
        if let Some(toast) = &self.state.toast
            && std::time::Instant::now() >= toast.expires_at
        {
            self.state.toast = None;
        }
    }

    pub(crate) fn set_toast(&mut self, message: &str, level: ToastLevel) {
        self.state.toast = Some(ToastState {
            message: message.to_string(),
            level,
            expires_at: std::time::Instant::now() + Duration::from_secs(3),
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
        true
    }
}
