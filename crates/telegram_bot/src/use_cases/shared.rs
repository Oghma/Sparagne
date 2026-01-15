use chrono::{DateTime, FixedOffset, Utc};
use chrono_tz::Europe::Rome;
use engine::Currency as EngineCurrency;
use reqwest::StatusCode;
use teloxide::{prelude::*, types::InlineKeyboardMarkup};

use crate::{
    ConfigParameters,
    api::{ApiError, ApiGateway},
    bot_client::BotClient,
    i18n::{self, TextKey},
    state::{PrefsStore, UserPrefs},
};
use api_types::error::ErrorCode;
use uuid::Uuid;

pub(crate) fn user_message_for_api_error(locale: i18n::Locale, err: ApiError) -> String {
    match err {
        ApiError::Network(_) => i18n::t(locale, TextKey::ApiNetworkError).to_string(),
        ApiError::Server {
            status,
            code,
            message,
        } => match code {
            ErrorCode::MembershipLastOwner => {
                i18n::t(locale, TextKey::ApiMembershipLastOwner).to_string()
            }
            ErrorCode::MembershipOwnerImmutable => {
                i18n::t(locale, TextKey::ApiMembershipOwnerImmutable).to_string()
            }
            ErrorCode::MembershipOwnerRemoveForbidden => {
                i18n::t(locale, TextKey::ApiMembershipOwnerRemoveForbidden).to_string()
            }
            _ => match status {
                StatusCode::UNAUTHORIZED => i18n::t(locale, TextKey::ApiUnauthorized).to_string(),
                StatusCode::FORBIDDEN => i18n::t(locale, TextKey::ApiForbidden).to_string(),
                StatusCode::NOT_FOUND => i18n::t(locale, TextKey::ApiNotFound).to_string(),
                StatusCode::CONFLICT => i18n::t(locale, TextKey::ApiConflict).to_string(),
                StatusCode::BAD_REQUEST => {
                    if message == "user not found" {
                        i18n::t(locale, TextKey::ApiBadRequestUserNotFound).to_string()
                    } else {
                        message
                    }
                }
                StatusCode::UNPROCESSABLE_ENTITY => message,
                _ => i18n::t(locale, TextKey::ApiServerError).to_string(),
            },
        },
    }
}

pub(crate) async fn send_api_error(
    bot: &dyn BotClient,
    chat_id: ChatId,
    locale: i18n::Locale,
    err: ApiError,
) -> ResponseResult<()> {
    let text = user_message_for_api_error(locale, err);
    bot.send_message(chat_id, &text, None).await?;
    Ok(())
}

pub(crate) fn now_rome() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&Rome).fixed_offset()
}

pub(crate) fn engine_currency(currency: api_types::Currency) -> EngineCurrency {
    match currency {
        api_types::Currency::Eur => EngineCurrency::Eur,
    }
}

pub(crate) async fn ensure_flow_defaults(
    prefs_store: &PrefsStore,
    user_id: u64,
    unallocated_flow_id: Uuid,
) -> UserPrefs {
    let mut prefs = prefs_store.get_or_default(user_id).await;
    if (prefs.last_flow_id.is_none() || prefs.default_flow_id.is_none())
        && let Ok(updated) = prefs_store
            .update(user_id, |prefs| {
                if prefs.last_flow_id.is_none() {
                    prefs.last_flow_id = Some(unallocated_flow_id);
                }
                if prefs.default_flow_id.is_none() {
                    prefs.default_flow_id = Some(unallocated_flow_id);
                }
            })
            .await
    {
        prefs = updated;
    }
    prefs
}

pub(crate) async fn edit_or_send(
    bot: &dyn BotClient,
    chat_id: ChatId,
    cfg: &ConfigParameters,
    text: String,
    kb: InlineKeyboardMarkup,
) -> ResponseResult<()> {
    let session = cfg.sessions.get(chat_id).await;
    if let Some(message_id) = session.hub_message_id
        && bot
            .edit_message_text(chat_id, message_id, &text, kb.clone())
            .await
            .is_ok()
    {
        return Ok(());
    }

    let sent = bot.send_message(chat_id, &text, Some(kb)).await?;
    cfg.sessions
        .update(chat_id, |s| s.hub_message_id = Some(sent))
        .await;
    Ok(())
}

pub(crate) async fn resolve_main_vault_id(
    api: &dyn ApiGateway,
    telegram_user_id: u64,
) -> Result<String, ApiError> {
    let vault = api.vault_get_main(telegram_user_id).await?;
    vault.id.ok_or(ApiError::Server {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: ErrorCode::Unknown,
        message: "vault id missing".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::MockApi;
    use api_types::{error::ErrorCode, vault::Vault};

    #[tokio::test]
    async fn resolve_main_vault_id_returns_id() {
        let api = MockApi::new();
        let mut guard = match api.vault_get_main.lock() {
            Ok(guard) => guard,
            Err(_) => panic!("mock lock"),
        };
        *guard = Some(Ok(Vault {
            id: Some("vault-1".to_string()),
            name: Some("Main".to_string()),
            currency: None,
            owner: None,
        }));

        let id = match resolve_main_vault_id(&api, 42).await {
            Ok(id) => id,
            Err(err) => panic!("expected vault id: {err:?}"),
        };
        assert_eq!(id, "vault-1");
    }

    #[tokio::test]
    async fn resolve_main_vault_id_fails_when_missing() {
        let api = MockApi::new();
        let mut guard = match api.vault_get_main.lock() {
            Ok(guard) => guard,
            Err(_) => panic!("mock lock"),
        };
        *guard = Some(Ok(Vault {
            id: None,
            name: Some("Main".to_string()),
            currency: None,
            owner: None,
        }));

        let err = match resolve_main_vault_id(&api, 42).await {
            Ok(_) => panic!("expected missing id"),
            Err(err) => err,
        };
        match err {
            ApiError::Server { status, code, .. } => {
                assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(code, ErrorCode::Unknown);
            }
            ApiError::Network(_) => panic!("expected server error"),
        }
    }
}
