//! Flow CRUD operations

use api_types::{
    cash_flow::CashFlowGet,
    flow::{FlowCreated, FlowNew, FlowSharedList, FlowSharedListResponse, FlowUpdate},
};

use super::{Client, ClientError, handle_empty, handle_json};

impl Client {
    pub async fn flow_new(
        &self,
        payload: FlowNew,
    ) -> std::result::Result<FlowCreated, ClientError> {
        let endpoint = self
            .base_url
            .join("flows")
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.post(endpoint))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn flows_shared_list(
        &self,
        payload: FlowSharedList,
    ) -> std::result::Result<FlowSharedListResponse, ClientError> {
        let endpoint = self
            .base_url
            .join("flows/shared")
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.post(endpoint))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn flow_update(
        &self,
        flow_id: uuid::Uuid,
        payload: FlowUpdate,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("flows/{flow_id}"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.patch(endpoint))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
    }

    pub async fn cash_flow_get(
        &self,
        payload: CashFlowGet,
    ) -> std::result::Result<engine::CashFlow, ClientError> {
        let endpoint = self
            .base_url
            .join("cashFlow/get")
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.post(endpoint))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    /// Removes a flow reference from a vault (unshares a flow that was shared with the user).
    pub async fn flow_unshare(
        &self,
        vault_id: &str,
        flow_id: uuid::Uuid,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("vault/{vault_id}/flow-references/{flow_id}"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.delete(endpoint))
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
    }
}
