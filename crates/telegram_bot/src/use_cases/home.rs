use reqwest::StatusCode;
use teloxide::prelude::*;

use crate::{ConfigParameters, api::ApiError, state::PendingAction, ui, use_cases::shared};

pub(crate) async fn show_home(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
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
                bot.send_message(chat_id, "Per fare pairing: /start <codice>")
                    .await?;
                cfg.sessions
                    .update(chat_id, |s| s.pending = Some(PendingAction::PairCode))
                    .await;
            } else {
                bot.send_message(chat_id, shared::user_message_for_api_error(err))
                    .await?;
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
    let (text, kb) = ui::render_home(&display_name, &snapshot, &prefs);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}

pub(crate) async fn show_wallet_picker(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
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
                bot.send_message(chat_id, "Per fare pairing: /start <codice>")
                    .await?;
            } else {
                bot.send_message(chat_id, shared::user_message_for_api_error(err))
                    .await?;
            }
            return Ok(());
        }
    };
    let (text, kb) = ui::render_wallet_picker(&snapshot, back_callback);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}

pub(crate) async fn show_flow_picker(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
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
                bot.send_message(chat_id, "Per fare pairing: /start <codice>")
                    .await?;
            } else {
                bot.send_message(chat_id, shared::user_message_for_api_error(err))
                    .await?;
            }
            return Ok(());
        }
    };
    let (text, kb) = ui::render_flow_picker(&snapshot, back_callback);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}
