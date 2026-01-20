use teloxide::prelude::*;

use crate::{
    ConfigParameters,
    i18n::{self, TextKey},
    routing::{self, Command},
    text, ui,
    use_cases::{export, home, shared},
};

use super::{is_allowed, pending, templates};

pub(crate) async fn handle_message(
    bot: Bot,
    msg: Message,
    cfg: ConfigParameters,
) -> ResponseResult<()> {
    if !is_allowed(&cfg, msg.from.as_ref()) {
        return Ok(());
    }

    let locale = msg
        .from
        .as_ref()
        .map(|user| i18n::resolve_locale(user.language_code.as_deref()))
        .unwrap_or_else(i18n::default_locale);
    let Some(from) = msg.from.as_ref() else {
        bot.send_message(msg.chat.id, i18n::t(locale, TextKey::UnknownUser))
            .await?;
        return Ok(());
    };
    let user_id = from.id.0;
    let chat_id = msg.chat.id;
    cfg.sessions
        .update(chat_id, |s| {
            s.display_name = Some(text::display_name_from_telegram(from))
        })
        .await;

    // If we are waiting for an input (pair/edit), handle it first.
    if let Some(pending) = cfg.sessions.get(chat_id).await.pending
        && pending::handle_pending_message(&bot, &msg, &cfg, user_id, pending, locale).await?
    {
        return Ok(());
    }

    let Some(text) = msg.text() else {
        return Ok(());
    };

    if let Some(cmd) = routing::parse_command(text) {
        match cmd {
            Command::Start { code } => {
                if let Some(code) = code.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
                    if let Err(err) = cfg.api.pair_user(user_id, code).await {
                        bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                            .await?;
                        return Ok(());
                    }

                    cfg.sessions.update(chat_id, |s| s.pending = None).await;

                    // Show pairing success
                    bot.send_message(chat_id, text::pairing_success(locale))
                        .await?;

                    // Check if this is a first-time user (no wallet set yet)
                    let prefs = cfg.prefs.get_or_default(user_id).await;
                    if prefs.default_wallet_id.is_none() {
                        let display_name = cfg
                            .sessions
                            .get(chat_id)
                            .await
                            .display_name
                            .unwrap_or_else(|| "Sparagne".to_string());
                        bot.send_message(chat_id, text::first_time_welcome(locale, &display_name))
                            .await?;
                    }

                    home::show_home(&bot, chat_id, user_id, &cfg, locale).await?;
                    return Ok(());
                }

                let display_name = cfg
                    .sessions
                    .get(chat_id)
                    .await
                    .display_name
                    .unwrap_or_else(|| "Sparagne".to_string());
                bot.send_message(chat_id, text::welcome_text(locale, &display_name))
                    .await?;
                home::show_home(&bot, chat_id, user_id, &cfg, locale).await?;
                return Ok(());
            }
            Command::Home => {
                cfg.sessions.update(chat_id, |s| s.wizard = None).await;
                home::show_home(&bot, chat_id, user_id, &cfg, locale).await?;
                return Ok(());
            }
            Command::Help => {
                let help_text = text::help_text(locale);
                let extra = i18n::t(locale, TextKey::PairingRequired);
                bot.send_message(chat_id, format!("{help_text}\n\n{extra}"))
                    .await?;
                return Ok(());
            }
            Command::Categories => {
                let prefs = cfg.prefs.get_or_default(user_id).await;
                let vault_ref = shared::vault_ref_from_prefs(&prefs);
                let cats = match shared::list_categories(&cfg.api, user_id, &vault_ref).await {
                    Ok(c) => c,
                    Err(err) => {
                        shared::send_api_error(&bot, chat_id, locale, err).await?;
                        return Ok(());
                    }
                };
                let (text, kb) = ui::categories::render_categories(locale, &cats);
                shared::edit_or_send(&bot, chat_id, &cfg, text, kb).await?;
                return Ok(());
            }
            Command::Export => {
                export::handle_export(&bot, chat_id, user_id, &cfg, locale).await?;
                return Ok(());
            }
            Command::Template => {
                templates::show_template_list(&bot, chat_id, user_id, &cfg, locale).await?;
                return Ok(());
            }
            Command::Vault { value } => {
                let _ = value;
                home::show_vault_picker(&bot, chat_id, user_id, &cfg, locale, "nav:home").await?;
                return Ok(());
            }
        }
    }

    if routing::looks_like_quick_add(text) {
        pending::handle_quick_add(&bot, &msg, &cfg, user_id, locale).await?;
    }

    Ok(())
}
