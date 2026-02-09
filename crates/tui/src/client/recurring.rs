//! Recurring template CRUD operations

use api_types::recurring::{
    PendingRecurringList, PendingRecurringListResponse, RecurringExecute,
    RecurringExecuteResponse, RecurringTemplateArchive, RecurringTemplateCreated,
    RecurringTemplateList, RecurringTemplateListResponse, RecurringTemplateNew,
    RecurringTemplateUpdate,
};
use uuid::Uuid;

use super::{Client, ClientError, handle_empty, handle_json};

impl Client {
    pub async fn recurring_list(
        &self,
        payload: RecurringTemplateList,
    ) -> std::result::Result<RecurringTemplateListResponse, ClientError> {
        let endpoint = self
            .base_url
            .join("recurring/list")
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.post(endpoint))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn recurring_create(
        &self,
        payload: RecurringTemplateNew,
    ) -> std::result::Result<RecurringTemplateCreated, ClientError> {
        let endpoint = self
            .base_url
            .join("recurring")
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.post(endpoint))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn recurring_update(
        &self,
        id: Uuid,
        payload: RecurringTemplateUpdate,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("recurring/{id}"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.patch(endpoint))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
    }

    pub async fn recurring_archive(
        &self,
        id: Uuid,
        payload: RecurringTemplateArchive,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("recurring/{id}/archive"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.post(endpoint))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
    }

    pub async fn recurring_pending(
        &self,
        payload: PendingRecurringList,
    ) -> std::result::Result<PendingRecurringListResponse, ClientError> {
        let endpoint = self
            .base_url
            .join("recurring/pending")
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .auth(self.http.post(endpoint))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn recurring_execute(
        &self,
        id: Uuid,
        payload: RecurringExecute,
    ) -> std::result::Result<RecurringExecuteResponse, ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("recurring/{id}/execute"))
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
