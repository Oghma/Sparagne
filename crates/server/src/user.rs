//! The module contains the definition of a user and its

use api_types::user::PairUser;
use axum::{Extension, Json, extract::State, http::StatusCode};
use sea_orm::{ActiveValue, entity::prelude::*};

use crate::{ServerError, server::ServerState};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub username: String,
    pub password: String,
    pub telegram_id: Option<String>,
    pub pair_code: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// Function to pair a user with its telegram id
pub async fn pair(
    _: Extension<Model>,
    State(state): State<ServerState>,
    Json(payload): Json<PairUser>,
) -> Result<StatusCode, ServerError> {
    if let Some(user) = Entity::find()
        .filter(Column::PairCode.eq(payload.code))
        .one(&state.db)
        .await
        .map_err(|err| ServerError::Generic(err.to_string()))?
    {
        let mut user: ActiveModel = user.into();
        user.telegram_id = ActiveValue::Set(Some(payload.telegram_id));
        user.pair_code = ActiveValue::Set(None);

        user.update(&state.db)
            .await
            .map_err(|err| ServerError::Generic(err.to_string()))?;
    } else {
        return Err(ServerError::Generic("user not found".to_string()));
    }

    Ok(StatusCode::CREATED)
}

/// Function to unpair the user with its teleram id
pub async fn unpair(
    Extension(user): Extension<Model>,
    State(state): State<ServerState>,
) -> Result<StatusCode, ServerError> {
    if let Some(user) = Entity::find()
        .filter(Column::TelegramId.eq(user.telegram_id))
        .one(&state.db)
        .await
        .map_err(|err| ServerError::Generic(err.to_string()))?
    {
        let mut user: ActiveModel = user.into();
        user.telegram_id = ActiveValue::Set(None);
        user.update(&state.db)
            .await
            .map_err(|err| ServerError::Generic(err.to_string()))?;
    } else {
        return Err(ServerError::Generic("user not found".to_string()));
    }

    Ok(StatusCode::ACCEPTED)
}
