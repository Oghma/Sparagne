//! Statistics endpoints

use api_types::{stats::Statistic, vault::Vault};

use super::{Client, ClientError, handle_json};

impl Client {
    pub async fn stats_get(
        &self,
        username: &str,
        password: &str,
        payload: Vault,
    ) -> std::result::Result<Statistic, ClientError> {
        let endpoint = self
            .base_url
            .join("stats/get")
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .post(endpoint)
            .basic_auth(username, Some(password))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }
}
