//! Categories API endpoints.

use api_types::category::{
    CategoryAliasCreate, CategoryAliasCreated, CategoryAliasDelete, CategoryAliasList,
    CategoryAliasListResponse, CategoryAliasView, CategoryCreate, CategoryCreated, CategoryList,
    CategoryListResponse, CategoryMerge, CategoryMergeConflict, CategoryMergePreview,
    CategoryMergePreviewResponse, CategoryUpdate, CategoryView,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use uuid::Uuid;

use crate::{ServerError, server::ServerState, user};

fn map_category(category: engine::Category) -> CategoryView {
    CategoryView {
        id: category.id,
        name: category.name,
        archived: category.archived,
        is_system: category.is_system,
    }
}

fn map_alias(alias: engine::CategoryAlias) -> CategoryAliasView {
    CategoryAliasView {
        id: alias.id,
        alias: alias.alias,
        category_id: alias.category_id,
    }
}

fn map_merge_conflict(conflict: engine::CategoryMergeConflict) -> CategoryMergeConflict {
    CategoryMergeConflict {
        kind: conflict.kind.as_str().to_string(),
        value: conflict.value,
    }
}

pub async fn list(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<CategoryList>,
) -> Result<Json<CategoryListResponse>, ServerError> {
    let include_archived = payload.include_archived.unwrap_or(false);
    let categories = state
        .engine
        .list_categories(&payload.vault_id, &user.username, include_archived)
        .await?
        .into_iter()
        .map(map_category)
        .collect();

    Ok(Json(CategoryListResponse { categories }))
}

pub async fn create(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<CategoryCreate>,
) -> Result<(StatusCode, Json<CategoryCreated>), ServerError> {
    let category = state
        .engine
        .create_category(&payload.vault_id, &payload.name, &user.username)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(CategoryCreated {
            id: category.id,
            name: category.name,
        }),
    ))
}

pub async fn update(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path(category_id): Path<Uuid>,
    Json(payload): Json<CategoryUpdate>,
) -> Result<Json<CategoryView>, ServerError> {
    if payload.name.is_none() && payload.archived.is_none() {
        return Err(ServerError::Generic(
            "provide at least one of name or archived".to_string(),
        ));
    }

    let category = state
        .engine
        .update_category(
            &payload.vault_id,
            category_id,
            payload.name.as_deref(),
            payload.archived,
            &user.username,
        )
        .await?;
    Ok(Json(map_category(category)))
}

pub async fn list_aliases(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path(category_id): Path<Uuid>,
    Json(payload): Json<CategoryAliasList>,
) -> Result<Json<CategoryAliasListResponse>, ServerError> {
    let aliases = state
        .engine
        .list_category_aliases(&payload.vault_id, category_id, &user.username)
        .await?
        .into_iter()
        .map(map_alias)
        .collect();
    Ok(Json(CategoryAliasListResponse { aliases }))
}

pub async fn create_alias(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path(category_id): Path<Uuid>,
    Json(payload): Json<CategoryAliasCreate>,
) -> Result<(StatusCode, Json<CategoryAliasCreated>), ServerError> {
    let alias = state
        .engine
        .create_category_alias(
            &payload.vault_id,
            category_id,
            &payload.alias,
            &user.username,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(CategoryAliasCreated {
            id: alias.id,
            alias: alias.alias,
        }),
    ))
}

pub async fn delete_alias(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path((category_id, alias_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<CategoryAliasDelete>,
) -> Result<StatusCode, ServerError> {
    state
        .engine
        .delete_category_alias(&payload.vault_id, category_id, alias_id, &user.username)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn merge(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path(category_id): Path<Uuid>,
    Json(payload): Json<CategoryMerge>,
) -> Result<Json<CategoryView>, ServerError> {
    let category = state
        .engine
        .merge_category(
            &payload.vault_id,
            category_id,
            payload.into_category_id,
            &user.username,
        )
        .await?;
    Ok(Json(map_category(category)))
}

pub async fn preview_merge(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path(category_id): Path<Uuid>,
    Json(payload): Json<CategoryMergePreview>,
) -> Result<Json<CategoryMergePreviewResponse>, ServerError> {
    let preview = state
        .engine
        .preview_category_merge(
            &payload.vault_id,
            category_id,
            payload.into_category_id,
            &user.username,
        )
        .await?;
    let conflicts = preview
        .conflicts
        .into_iter()
        .map(map_merge_conflict)
        .collect();
    Ok(Json(CategoryMergePreviewResponse {
        ok: preview.ok,
        conflicts,
    }))
}
