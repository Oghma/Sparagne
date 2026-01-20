use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
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
use api_types::{error::ErrorCode, vault::Vault};
use uuid::Uuid;

pub(crate) fn user_message_for_api_error(locale: i18n::Locale, err: ApiError) -> String {
    match err {
        ApiError::Network(_) => i18n::t(locale, TextKey::ApiNetworkError).to_string(),
        ApiError::Server {
            status,
            code: _,
            message,
        } => match status {
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
    }
}

pub(crate) async fn send_api_error(
    bot: &dyn BotClient,
    chat_id: ChatId,
    locale: i18n::Locale,
    err: ApiError,
) -> ResponseResult<()> {
    // Determine if we need recovery hint before consuming the error
    let needs_hint = match &err {
        ApiError::Network(_) => true,
        ApiError::Server { status, .. } => {
            *status != StatusCode::UNAUTHORIZED
                && *status != StatusCode::FORBIDDEN
                && *status != StatusCode::BAD_REQUEST
        }
    };

    let mut text = user_message_for_api_error(locale, err);

    // Add recovery hint for errors that aren't auth-related (which already suggest
    // pairing)
    if needs_hint {
        text.push_str(i18n::t(locale, TextKey::ErrorRecoveryHint));
    }

    bot.send_message(chat_id, &text, None).await?;
    Ok(())
}

pub(crate) fn now_rome() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&Rome).fixed_offset()
}

pub(crate) fn rome_start_of_day(date: NaiveDate) -> Option<DateTime<FixedOffset>> {
    rome_datetime(date, 0, 0, 0, false)
}

pub(crate) fn rome_end_of_day(date: NaiveDate) -> Option<DateTime<FixedOffset>> {
    rome_datetime(date, 23, 59, 59, true)
}

fn rome_datetime(
    date: NaiveDate,
    hour: u32,
    min: u32,
    sec: u32,
    prefer_latest: bool,
) -> Option<DateTime<FixedOffset>> {
    let naive = date.and_hms_opt(hour, min, sec)?;
    match Rome.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.fixed_offset()),
        chrono::LocalResult::Ambiguous(early, late) => Some(if prefer_latest {
            late.fixed_offset()
        } else {
            early.fixed_offset()
        }),
        chrono::LocalResult::None => None,
    }
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

pub(crate) async fn resolve_vault_id(
    api: &dyn ApiGateway,
    telegram_user_id: u64,
    vault: &Vault,
) -> Result<String, ApiError> {
    let vault = api.vault_get(telegram_user_id, vault).await?;
    vault.id.ok_or(ApiError::Server {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: ErrorCode::Unknown,
        message: "vault id missing".to_string(),
    })
}

pub(crate) async fn list_categories(
    api: &dyn ApiGateway,
    telegram_user_id: u64,
    vault: &Vault,
) -> Result<Vec<api_types::category::CategoryView>, ApiError> {
    let vault_id = resolve_vault_id(api, telegram_user_id, vault).await?;
    let resp = api
        .categories_list(
            telegram_user_id,
            &api_types::category::CategoryList {
                vault_id,
                include_archived: None,
            },
        )
        .await?;
    Ok(resp.categories)
}

pub(crate) fn vault_ref_from_value(value: &str) -> Vault {
    let trimmed = value.trim();
    if let Some(id) = trimmed
        .strip_prefix("id:")
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        Vault {
            id: Some(id.to_string()),
            name: None,
            currency: None,
            owner: None,
        }
    } else {
        let name = if trimmed.is_empty() { "Main" } else { trimmed };
        Vault {
            id: None,
            name: Some(name.to_string()),
            currency: None,
            owner: None,
        }
    }
}

pub(crate) fn vault_ref_from_prefs(prefs: &UserPrefs) -> Vault {
    vault_ref_from_value(&prefs.active_vault_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::MockApi;
    use api_types::{error::ErrorCode, vault::Vault};

    #[tokio::test]
    async fn resolve_vault_id_returns_id() {
        let api = MockApi::new();
        {
            let mut guard = match api.vault_get.lock() {
                Ok(guard) => guard,
                Err(_) => panic!("mock lock"),
            };
            *guard = Some(Ok(Vault {
                id: Some("vault-1".to_string()),
                name: Some("Main".to_string()),
                currency: None,
                owner: None,
            }));
        } // guard dropped here before async call

        let payload = Vault {
            id: None,
            name: Some("Main".to_string()),
            currency: None,
            owner: None,
        };
        let id = match resolve_vault_id(&api, 42, &payload).await {
            Ok(id) => id,
            Err(err) => panic!("expected vault id: {err:?}"),
        };
        assert_eq!(id, "vault-1");
    }

    #[tokio::test]
    async fn resolve_vault_id_fails_when_missing() {
        let api = MockApi::new();
        {
            let mut guard = match api.vault_get.lock() {
                Ok(guard) => guard,
                Err(_) => panic!("mock lock"),
            };
            *guard = Some(Ok(Vault {
                id: None,
                name: Some("Main".to_string()),
                currency: None,
                owner: None,
            }));
        } // guard dropped here before async call

        let payload = Vault {
            id: None,
            name: Some("Main".to_string()),
            currency: None,
            owner: None,
        };
        let err = match resolve_vault_id(&api, 42, &payload).await {
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
