//! Vault CRUD operations

use api_types::vault::{Vault, VaultList, VaultListResponse, VaultNew, VaultSnapshot};

use super::{Client, ClientError, handle_empty, handle_json};

impl Client {
    pub async fn vault_get(&self, payload: &Vault) -> std::result::Result<Vault, ClientError> {
        let endpoint = self
            .base_url
            .join("vault/get")
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.post(endpoint))
            .json(payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn vault_snapshot(
        &self,
        payload: &Vault,
    ) -> std::result::Result<VaultSnapshot, ClientError> {
        let endpoint = self
            .base_url
            .join("vault/snapshot")
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.post(endpoint))
            .json(payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn vault_list(&self) -> std::result::Result<VaultListResponse, ClientError> {
        let endpoint = self
            .base_url
            .join("vault/list")
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.post(endpoint))
            .json(&VaultList::default())
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn vault_new(&self, payload: VaultNew) -> std::result::Result<Vault, ClientError> {
        let endpoint = self
            .base_url
            .join("vault/new")
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.post(endpoint))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn vault_delete(&self, vault_id: &str) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("vault/{vault_id}"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.delete(endpoint))
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
    }
}
