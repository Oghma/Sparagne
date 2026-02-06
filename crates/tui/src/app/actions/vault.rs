use super::super::*;

use crate::{
    error::Result,
    text::{t, TextKey},
};
use api_types::vault::{Vault, VaultNew};

impl App {
    pub(crate) async fn start_vault_select(&mut self) -> Result<()> {
        self.state.vault_ui.error = None;
        self.state.vault_ui.mode = VaultMode::Select;
        self.state.vault_ui.list.error = None;
        self.load_vault_list().await
    }

    pub(crate) async fn load_vault_list(&mut self) -> Result<()> {
        let res = self
            .client
            .vault_list()
            .await;

        match res {
            Ok(response) => {
                let current_id = self
                    .state
                    .vault
                    .as_ref()
                    .and_then(|vault| vault.id.as_deref());
                self.state.vault_ui.list.items = response.vaults;
                self.state.vault_ui.list.selected = current_id
                    .and_then(|id| {
                        self.state
                            .vault_ui
                            .list
                            .items
                            .iter()
                            .position(|vault| vault.id == id)
                    })
                    .unwrap_or(0);
                self.state.vault_ui.list.error = None;
            }
            Err(err) => {
                let Some(msg) = self.client_error_message(err) else { return Ok(()); };
                self.state.vault_ui.list.error = Some(msg);
            }
        }

        Ok(())
    }

    pub(crate) async fn submit_vault_select(&mut self) -> Result<()> {
        let Some(selected) = self
            .state
            .vault_ui
            .list
            .items
            .get(self.state.vault_ui.list.selected)
        else {
            return Ok(());
        };
        let selected = selected.clone();
        let vault = Vault {
            id: Some(selected.id.clone()),
            name: Some(selected.name.clone()),
            currency: Some(selected.currency),
            owner: Some(selected.owner.clone()),
        };

        self.state.vault = Some(vault);
        self.state.vault_ui.mode = VaultMode::View;
        self.state.vault_ui.list.error = None;
        self.state.transactions.scope_wallet_id = None;
        self.state.transactions.scope_flow_id = None;
        self.state.transactions.search.query.clear();
        self.state.transactions.search.active = false;

        self.refresh_snapshot().await?;
        self.apply_local_defaults();
        self.load_transactions(true).await?;
        self.set_toast(t(self.state.locale, TextKey::SuccessVaultSelected), ToastLevel::Success);
        Ok(())
    }
    pub(crate) async fn save_defaults(&mut self) -> Result<()> {
        let Some(snapshot) = self.state.snapshot.as_ref() else {
            self.state.vault_ui.defaults.error =
                Some(t(self.state.locale, TextKey::StateSnapshotUnavailable).to_string());
            return Ok(());
        };
        let username = self.state.login.username.trim();
        let Some(vault_id) = self.state.vault.as_ref().and_then(|v| v.id.as_deref()) else {
            self.state.vault_ui.defaults.error =
                Some(t(self.state.locale, TextKey::StateVaultUnavailable).to_string());
            return Ok(());
        };
        if username.is_empty() {
            self.state.vault_ui.defaults.error =
                Some(t(self.state.locale, TextKey::StateUserUnavailable).to_string());
            return Ok(());
        }

        let wallet_ids = snapshot
            .wallets
            .iter()
            .filter(|wallet| !wallet.archived)
            .map(|wallet| wallet.id)
            .collect::<Vec<_>>();
        let flow_ids = snapshot
            .flows
            .iter()
            .filter(|flow| !flow.archived)
            .map(|flow| flow.id)
            .collect::<Vec<_>>();

        let wallet_id = if self.state.vault_ui.defaults.wallet_index == 0 {
            None
        } else {
            wallet_ids
                .get(self.state.vault_ui.defaults.wallet_index - 1)
                .copied()
        };
        let flow_id = if self.state.vault_ui.defaults.flow_index == 0 {
            None
        } else {
            flow_ids
                .get(self.state.vault_ui.defaults.flow_index - 1)
                .copied()
        };

        self.state.default_wallet_id = wallet_id;
        self.state.default_flow_id = flow_id;
        self.local_state
            .set_defaults(username, vault_id, wallet_id, flow_id);
        if let Err(err) = self.local_state.save(self.local_state_path.as_str()) {
            self.state.vault_ui.defaults.error = Some(err.to_string());
            self.set_toast(t(self.state.locale, TextKey::ErrorSaveDefaults), ToastLevel::Error);
            return Ok(());
        }

        self.state.vault_ui.mode = VaultMode::View;
        self.state.vault_ui.defaults = DefaultsFormState::default();
        self.set_toast(t(self.state.locale, TextKey::SuccessDefaultsSaved), ToastLevel::Success);
        Ok(())
    }
    pub(crate) async fn submit_vault_create(&mut self) -> Result<()> {
        let name = self.state.vault_ui.form.name.trim();
        if name.is_empty() {
            self.state.vault_ui.form.error =
                Some(t(self.state.locale, TextKey::PromptEnterName).to_string());
            return Ok(());
        }

        let res = self
            .client
            .vault_new(
                VaultNew {
                    name: name.to_string(),
                    currency: Some(api_types::Currency::Eur),
                },
            )
            .await;

        match res {
            Ok(vault) => {
                self.state.vault = Some(vault);
                self.state.vault_ui.mode = VaultMode::View;
                self.reset_vault_form();
                self.refresh_snapshot().await?;
                self.set_toast(t(self.state.locale, TextKey::SuccessVaultCreated), ToastLevel::Success);
            }
            Err(err) => {
                let Some(msg) = self.client_error_message(err) else { return Ok(()); };
                self.state.vault_ui.form.error = Some(msg);
                self.set_toast(t(self.state.locale, TextKey::ErrorCreateVault), ToastLevel::Error);
            }
        }

        Ok(())
    }
    pub(crate) async fn delete_vault(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let res = self
            .client
            .vault_delete(vault_id.as_str())
            .await;

        match res {
            Ok(()) => {
                self.reset_after_vault_delete();
            }
            Err(err) => {
                let Some(message) = self.client_error_message(err) else { return Ok(()); };
                self.state.vault_ui.error = Some(message.clone());
                self.state.overlays.error = Some(ErrorDialogState::error(
                    t(self.state.locale, TextKey::UiError),
                    t(self.state.locale, TextKey::UiFailedToDeleteVault),
                    Some(message),
                ));
                self.set_toast(t(self.state.locale, TextKey::ErrorDeleteVault), ToastLevel::Error);
            }
        }

        Ok(())
    }
}
