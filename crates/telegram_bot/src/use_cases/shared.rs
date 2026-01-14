use chrono::{DateTime, FixedOffset, Utc};
use chrono_tz::Europe::Rome;
use engine::Currency as EngineCurrency;
use reqwest::StatusCode;
use teloxide::{prelude::*, types::InlineKeyboardMarkup};

use crate::{
    ConfigParameters,
    api::{ApiClient, ApiError},
    state::{PrefsStore, UserPrefs},
};
use api_types::error::ErrorCode;
use uuid::Uuid;

pub(crate) fn user_message_for_api_error(err: ApiError) -> String {
    match err {
        ApiError::Network(_) => {
            "Problemi di connessione con il server. Riprova pi\u{f9} tardi!".to_string()
        }
        ApiError::Server {
            status,
            code,
            message,
        } => match code {
            ErrorCode::MembershipLastOwner => {
                "Non puoi rimuovere l'ultimo owner del flow.".to_string()
            }
            ErrorCode::MembershipOwnerImmutable => {
                "Non puoi cambiare il ruolo dell'owner del vault.".to_string()
            }
            ErrorCode::MembershipOwnerRemoveForbidden => {
                "Non puoi rimuovere l'owner del vault.".to_string()
            }
            _ => match status {
                StatusCode::UNAUTHORIZED => {
                    "Non autorizzato. Usa /start per fare il pairing.".to_string()
                }
                StatusCode::FORBIDDEN => "Operazione non permessa.".to_string(),
                StatusCode::NOT_FOUND => {
                    "Risorsa non trovata. Prova a reimpostare i default.".to_string()
                }
                StatusCode::CONFLICT => "Richiesta duplicata (gi\u{e0} salvata).".to_string(),
                StatusCode::BAD_REQUEST => {
                    if message == "user not found" {
                        "Codice di pairing non valido (o stai usando un database diverso da quello del server).".to_string()
                    } else {
                        message
                    }
                }
                StatusCode::UNPROCESSABLE_ENTITY => message,
                _ => "Errore server.".to_string(),
            },
        },
    }
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
    bot: &Bot,
    chat_id: ChatId,
    cfg: &ConfigParameters,
    text: String,
    kb: InlineKeyboardMarkup,
) -> ResponseResult<()> {
    let session = cfg.sessions.get(chat_id).await;
    if let Some(message_id) = session.hub_message_id
        && bot
            .edit_message_text(chat_id, message_id, text.clone())
            .reply_markup(kb.clone())
            .await
            .is_ok()
    {
        return Ok(());
    }

    let sent = bot.send_message(chat_id, text).reply_markup(kb).await?;
    cfg.sessions
        .update(chat_id, |s| s.hub_message_id = Some(sent.id))
        .await;
    Ok(())
}

pub(crate) async fn resolve_main_vault_id(
    api: &ApiClient,
    telegram_user_id: u64,
) -> Result<String, ApiError> {
    let vault = api.vault_get_main(telegram_user_id).await?;
    vault.id.ok_or(ApiError::Server {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: ErrorCode::Unknown,
        message: "vault id missing".to_string(),
    })
}
