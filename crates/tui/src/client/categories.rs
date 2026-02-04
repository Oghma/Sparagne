//! Category CRUD operations

use api_types::category::{
    CategoryAliasCreate, CategoryAliasCreated, CategoryAliasDelete, CategoryAliasList,
    CategoryAliasListResponse, CategoryCreate, CategoryCreated, CategoryList, CategoryListResponse,
    CategoryMerge, CategoryMergePreview, CategoryMergePreviewResponse, CategoryUpdate,
    CategoryView,
};

use super::{Client, ClientError, handle_empty, handle_json};

impl Client {
    pub async fn categories_list(
        &self,
        username: &str,
        password: &str,
        payload: CategoryList,
    ) -> std::result::Result<CategoryListResponse, ClientError> {
        let endpoint = self
            .base_url
            .join("categories/list")
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

    pub async fn categories_create(
        &self,
        username: &str,
        password: &str,
        payload: CategoryCreate,
    ) -> std::result::Result<CategoryCreated, ClientError> {
        let endpoint = self
            .base_url
            .join("categories")
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

    pub async fn categories_update(
        &self,
        username: &str,
        password: &str,
        category_id: uuid::Uuid,
        payload: CategoryUpdate,
    ) -> std::result::Result<CategoryView, ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("categories/{category_id}"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .patch(endpoint)
            .basic_auth(username, Some(password))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn category_aliases_list(
        &self,
        username: &str,
        password: &str,
        category_id: uuid::Uuid,
        payload: CategoryAliasList,
    ) -> std::result::Result<CategoryAliasListResponse, ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("categories/{category_id}/aliases/list"))
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

    pub async fn category_alias_create(
        &self,
        username: &str,
        password: &str,
        category_id: uuid::Uuid,
        payload: CategoryAliasCreate,
    ) -> std::result::Result<CategoryAliasCreated, ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("categories/{category_id}/aliases"))
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

    pub async fn category_alias_delete(
        &self,
        username: &str,
        password: &str,
        category_id: uuid::Uuid,
        alias_id: uuid::Uuid,
        payload: CategoryAliasDelete,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("categories/{category_id}/aliases/{alias_id}"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .delete(endpoint)
            .basic_auth(username, Some(password))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
    }

    pub async fn categories_merge_preview(
        &self,
        username: &str,
        password: &str,
        category_id: uuid::Uuid,
        payload: CategoryMergePreview,
    ) -> std::result::Result<CategoryMergePreviewResponse, ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("categories/{category_id}/merge/preview"))
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

    pub async fn categories_merge(
        &self,
        username: &str,
        password: &str,
        category_id: uuid::Uuid,
        payload: CategoryMerge,
    ) -> std::result::Result<CategoryView, ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("categories/{category_id}/merge"))
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
