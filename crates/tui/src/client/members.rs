//! Membership operations

use api_types::membership::{MemberUpsert, MembersResponse};

use super::{Client, ClientError, handle_empty, handle_json};

impl Client {
    pub async fn vault_members_list(
        &self,
        username: &str,
        password: &str,
        vault_id: &str,
    ) -> std::result::Result<MembersResponse, ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("vault/{vault_id}/members"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .get(endpoint)
            .basic_auth(username, Some(password))
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn vault_member_upsert(
        &self,
        username: &str,
        password: &str,
        vault_id: &str,
        payload: MemberUpsert,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("vault/{vault_id}/members"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .post(endpoint)
            .basic_auth(username, Some(password))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
    }

    pub async fn vault_member_remove(
        &self,
        username: &str,
        password: &str,
        vault_id: &str,
        member_username: &str,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("vault/{vault_id}/members/{member_username}"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .delete(endpoint)
            .basic_auth(username, Some(password))
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
    }

    pub async fn flow_members_list(
        &self,
        username: &str,
        password: &str,
        vault_id: &str,
        flow_id: uuid::Uuid,
    ) -> std::result::Result<MembersResponse, ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("vault/{vault_id}/flows/{flow_id}/members"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .get(endpoint)
            .basic_auth(username, Some(password))
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn flow_member_upsert(
        &self,
        username: &str,
        password: &str,
        vault_id: &str,
        flow_id: uuid::Uuid,
        payload: MemberUpsert,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("vault/{vault_id}/flows/{flow_id}/members"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .post(endpoint)
            .basic_auth(username, Some(password))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
    }

    pub async fn flow_member_remove(
        &self,
        username: &str,
        password: &str,
        vault_id: &str,
        flow_id: uuid::Uuid,
        member_username: &str,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!(
                "vault/{vault_id}/flows/{flow_id}/members/{member_username}"
            ))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .delete(endpoint)
            .basic_auth(username, Some(password))
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
    }
}
