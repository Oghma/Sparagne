use reqwest::StatusCode;
use teloxide::prelude::*;

use crate::{
    ConfigParameters,
    api::ApiError,
    bot_client::BotClient,
    i18n::{self, TextKey},
    state::PendingAction,
    ui,
    use_cases::shared,
};

pub(crate) async fn show_home(
    bot: &dyn BotClient,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let snapshot = match cfg.api.vault_snapshot_main(user_id).await {
        Ok(s) => s,
        Err(err) => {
            let needs_pairing = matches!(
                err,
                ApiError::Server { status, .. }
                    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN
            );
            if needs_pairing {
                bot.send_message(chat_id, i18n::t(locale, TextKey::PairingRequired), None)
                    .await?;
                cfg.sessions
                    .update(chat_id, |s| s.pending = Some(PendingAction::PairCode))
                    .await;
            } else {
                shared::send_api_error(bot, chat_id, locale, err).await?;
            }
            return Ok(());
        }
    };
    let prefs =
        shared::ensure_flow_defaults(&cfg.prefs, user_id, snapshot.unallocated_flow_id).await;
    let display_name = cfg
        .sessions
        .get(chat_id)
        .await
        .display_name
        .unwrap_or_else(|| "Sparagne".to_string());
    let (text, kb) = ui::home::render_home(locale, &display_name, &snapshot, &prefs);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}

pub(crate) async fn show_wallet_picker(
    bot: &dyn BotClient,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let back_callback = if cfg.sessions.get(chat_id).await.wizard.is_some() {
        "nav:wizard"
    } else {
        "nav:home"
    };
    let snapshot = match cfg.api.vault_snapshot_main(user_id).await {
        Ok(s) => s,
        Err(err) => {
            let needs_pairing = matches!(
                err,
                ApiError::Server { status, .. }
                    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN
            );
            if needs_pairing {
                cfg.sessions
                    .update(chat_id, |s| s.pending = Some(PendingAction::PairCode))
                    .await;
                bot.send_message(chat_id, i18n::t(locale, TextKey::PairingRequired), None)
                    .await?;
            } else {
                shared::send_api_error(bot, chat_id, locale, err).await?;
            }
            return Ok(());
        }
    };
    let (text, kb) = ui::home::render_wallet_picker(locale, &snapshot, back_callback);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}

pub(crate) async fn show_flow_picker(
    bot: &dyn BotClient,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let back_callback = if cfg.sessions.get(chat_id).await.wizard.is_some() {
        "nav:wizard"
    } else {
        "nav:home"
    };
    let snapshot = match cfg.api.vault_snapshot_main(user_id).await {
        Ok(s) => s,
        Err(err) => {
            let needs_pairing = matches!(
                err,
                ApiError::Server { status, .. }
                    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN
            );
            if needs_pairing {
                cfg.sessions
                    .update(chat_id, |s| s.pending = Some(PendingAction::PairCode))
                    .await;
                bot.send_message(chat_id, i18n::t(locale, TextKey::PairingRequired), None)
                    .await?;
            } else {
                shared::send_api_error(bot, chat_id, locale, err).await?;
            }
            return Ok(());
        }
    };
    let (text, kb) = ui::home::render_flow_picker(locale, &snapshot, back_callback);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}

pub(crate) async fn show_settings(
    bot: &dyn BotClient,
    chat_id: ChatId,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let (text, kb) = ui::home::render_settings(locale);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}

pub(crate) async fn show_commands(
    bot: &dyn BotClient,
    chat_id: ChatId,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let (text, kb) = ui::home::render_commands(locale);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}
