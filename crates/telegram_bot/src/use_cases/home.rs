use reqwest::StatusCode;
use teloxide::prelude::*;

use crate::{
    ConfigParameters,
    api::ApiError,
    bot_client::BotClient,
    i18n::{self, TextKey},
    state::{PendingAction, ScreenContext},
    ui,
    use_cases::shared,
};
use api_types::vault::VaultList;

pub(crate) async fn show_home(
    bot: &dyn BotClient,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let prefs = cfg.prefs.get_or_default(user_id).await;
    let vault_ref = shared::vault_ref_from_prefs(&prefs);
    let snapshot = match cfg.api.vault_snapshot(user_id, &vault_ref).await {
        Ok(s) => s,
        Err(err) => {
            let needs_pairing = matches!(
                err,
                ApiError::Server { status, .. }
                    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN
            );
            if needs_pairing {
                bot.send_message(chat_id, i18n::t(locale, TextKey::PairingPrompt), None)
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
    cfg.sessions
        .update(chat_id, |s| s.current_screen = ScreenContext::Home)
        .await;
    let (text, kb) = ui::home::render_home(locale, &display_name, &snapshot, &prefs);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}

pub(crate) async fn show_wallet_picker(
    bot: &dyn BotClient,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
    back_callback: &str,
) -> ResponseResult<()> {
    let prefs = cfg.prefs.get_or_default(user_id).await;
    let vault_ref = shared::vault_ref_from_prefs(&prefs);
    let snapshot = match cfg.api.vault_snapshot(user_id, &vault_ref).await {
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
                bot.send_message(chat_id, i18n::t(locale, TextKey::PairingPrompt), None)
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
    back_callback: &str,
) -> ResponseResult<()> {
    let prefs = cfg.prefs.get_or_default(user_id).await;
    let vault_ref = shared::vault_ref_from_prefs(&prefs);
    let snapshot = match cfg.api.vault_snapshot(user_id, &vault_ref).await {
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
                bot.send_message(chat_id, i18n::t(locale, TextKey::PairingPrompt), None)
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

pub(crate) async fn show_vault_picker(
    bot: &dyn BotClient,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
    back_callback: &str,
) -> ResponseResult<()> {
    let list = match cfg.api.vault_list(user_id, &VaultList::default()).await {
        Ok(list) => list,
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
                bot.send_message(chat_id, i18n::t(locale, TextKey::PairingPrompt), None)
                    .await?;
            } else {
                shared::send_api_error(bot, chat_id, locale, err).await?;
            }
            return Ok(());
        }
    };

    let mut vaults = list.vaults;
    vaults.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let (text, kb) = ui::home::render_vault_picker(locale, &vaults, back_callback);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}
