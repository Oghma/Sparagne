//! Statistics endpoints

use api_types::{stats::Statistic, vault::Vault};

use super::{Client, ClientError, handle_json};

impl Client {
    pub async fn stats_get(&self, payload: Vault) -> std::result::Result<Statistic, ClientError> {
        let endpoint = self
            .base_url
            .join("stats/get")
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.post(endpoint))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }
}
