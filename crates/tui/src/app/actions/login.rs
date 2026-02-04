use super::super::*;

use crate::{app::errors::login_message_for_error, client::ClientError, error::Result, text::{TextKey, t}};

impl App {
    pub(crate) async fn attempt_login(&mut self) -> Result<()> {
        let username = self.state.login.username.trim();
        let password = self.state.login.password.trim();
        let vault_payload = self.vault_payload_from_config();
        let has_vault = vault_payload
            .id
            .as_deref()
            .map(|id| !id.trim().is_empty())
            .unwrap_or(false)
            || vault_payload
                .name
                .as_deref()
                .map(|name| !name.trim().is_empty())
                .unwrap_or(false);

        if username.is_empty() || password.is_empty() || !has_vault {
            self.state.login.message = Some(t(self.state.locale, TextKey::PromptFillAllFields).to_string());
            return Ok(());
        }

        match self
            .client
            .vault_get(username, password, &vault_payload)
            .await
        {
            Ok(vault) => {
                self.state.vault = Some(vault);
                match self
                    .client
                    .vault_snapshot(username, password, &vault_payload)
                    .await
                {
                    Ok(snapshot) => {
                        self.apply_snapshot(snapshot);
                        self.apply_local_defaults();
                        self.state.screen = Screen::Home;
                        self.state.login.message = None;
                        self.load_transactions(true).await?;
                    }
                    Err(ClientError::NotFound(_)) => {
                        if let Err(err) = self.refresh_shared_flows_snapshot().await {
                            self.state.login.message = Some(err.to_string());
                            return Ok(());
                        }
                        self.apply_local_defaults();
                        self.state.screen = Screen::Home;
                        self.state.login.message = None;
                        self.load_transactions(true).await?;
                    }
                    Err(err) => {
                        self.state.login.message =
                            Some(login_message_for_error(err, self.state.locale));
                    }
                }
            }
            Err(err) => {
                self.state.login.message = Some(login_message_for_error(err, self.state.locale));
            }
        }

        Ok(())
    }

    pub(crate) fn apply_local_defaults(&mut self) {
        let username = self.state.login.username.trim();
        let Some(vault_id) = self.state.vault.as_ref().and_then(|v| v.id.as_deref()) else {
            return;
        };
        if username.is_empty() {
            return;
        }

        if let Some(defaults) = self.local_state.defaults_for(username, vault_id) {
            self.state.default_wallet_id = defaults.wallet_id;
            self.state.default_flow_id = defaults.flow_id;
        } else {
            self.state.default_wallet_id = None;
            self.state.default_flow_id = None;
        }
        self.normalize_defaults();
    }

    pub(crate) fn normalize_defaults(&mut self) {
        let Some(snapshot) = self.state.snapshot.as_ref() else {
            return;
        };
        if let Some(wallet_id) = self.state.default_wallet_id {
            let valid = snapshot
                .wallets
                .iter()
                .any(|wallet| wallet.id == wallet_id && !wallet.archived);
            if !valid {
                self.state.default_wallet_id = None;
            }
        }
        if let Some(flow_id) = self.state.default_flow_id {
            let valid = snapshot
                .flows
                .iter()
                .any(|flow| flow.id == flow_id && !flow.archived);
            if !valid {
                self.state.default_flow_id = None;
            }
        }
    }
}
