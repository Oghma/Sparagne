use engine::{Currency as EngineCurrency, Money};
use teloxide::{
    prelude::*,
    types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup},
};

use crate::{
    ConfigParameters,
    i18n::{self, TextKey},
    parsing::QuickKind,
    ui,
    use_cases::{home, shared},
};

pub(super) async fn show_template_list(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let prefs = cfg.prefs.get_or_default(user_id).await;
    let currency = EngineCurrency::Eur; // Default currency for display
    let (text, kb) = ui::template::render_template_list(locale, currency, &prefs.templates);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}

pub(super) async fn use_template(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    idx: usize,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let prefs = cfg.prefs.get_or_default(user_id).await;
    let Some(template) = prefs.templates.get(idx) else {
        // Invalid index, show list
        show_template_list(bot, chat_id, user_id, cfg, locale).await?;
        return Ok(());
    };

    let Some(wallet_id) = prefs.default_wallet_id else {
        bot.send_message(chat_id, i18n::t(locale, TextKey::DefaultWalletMissing))
            .await?;
        home::show_wallet_picker(bot, chat_id, user_id, cfg, locale, "tpl:list").await?;
        return Ok(());
    };

    let vault_ref = shared::vault_ref_from_prefs(&prefs);
    let snapshot = match cfg.api.vault_snapshot(user_id, &vault_ref).await {
        Ok(s) => s,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                .await?;
            return Ok(());
        }
    };
    if !snapshot.wallets.iter().any(|w| w.id == wallet_id) {
        let _ = cfg
            .prefs
            .update(user_id, |p| p.default_wallet_id = None)
            .await;
        bot.send_message(chat_id, i18n::t(locale, TextKey::DefaultWalletMissing))
            .await?;
        home::show_wallet_picker(bot, chat_id, user_id, cfg, locale, "tpl:list").await?;
        return Ok(());
    }

    let flow_id = prefs
        .last_flow_id
        .filter(|id| snapshot.flows.iter().any(|f| f.id == *id))
        .unwrap_or(snapshot.unallocated_flow_id);
    let occurred_at = shared::now_rome();
    let vault_id = snapshot.id.clone();
    let currency = shared::engine_currency(snapshot.currency);

    let created = match template.kind {
        QuickKind::Expense => {
            cfg.api
                .create_expense(
                    user_id,
                    &api_types::transaction::ExpenseNew {
                        vault_id,
                        amount_minor: template.amount_minor,
                        flow_id: Some(flow_id),
                        wallet_id: Some(wallet_id),
                        category_id: None,
                        category: template.category.clone(),
                        note: template.note.clone(),
                        idempotency_key: None,
                        occurred_at,
                    },
                )
                .await
        }
        QuickKind::Income => {
            cfg.api
                .create_income(
                    user_id,
                    &api_types::transaction::IncomeNew {
                        vault_id,
                        amount_minor: template.amount_minor,
                        flow_id: Some(flow_id),
                        wallet_id: Some(wallet_id),
                        category_id: None,
                        category: template.category.clone(),
                        note: template.note.clone(),
                        idempotency_key: None,
                        occurred_at,
                    },
                )
                .await
        }
    };

    match created {
        Ok(created) => {
            let signed_minor = match template.kind {
                QuickKind::Expense => -template.amount_minor,
                QuickKind::Income => template.amount_minor,
            };

            let saved_body = i18n::format(
                locale,
                TextKey::QuickAddSaved,
                &[("amount", &Money::new(signed_minor).format(currency))],
            );
            let mut saved_msg =
                format!("{}\n{}", i18n::t(locale, TextKey::TemplateUsed), saved_body);
            if let Some(category) = template.category.as_deref() {
                saved_msg.push_str(&format!(" • {category}"));
            }
            if let Some(note) = template.note.as_deref() {
                saved_msg.push_str(&format!(" • {note}"));
            }

            let kb = InlineKeyboardMarkup::new(vec![vec![
                InlineKeyboardButton::callback(
                    format!("\u{21a9} {}", i18n::t(locale, TextKey::QuickAddUndo)),
                    format!("tx:void:{id}", id = created.id),
                ),
                InlineKeyboardButton::callback(
                    format!(
                        "\u{270f}\u{fe0f} {}",
                        i18n::t(locale, TextKey::DetailBtnEdit)
                    ),
                    format!("tx:edit:{id}", id = created.id),
                ),
            ]]);

            bot.send_message(chat_id, saved_msg)
                .reply_markup(kb)
                .await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                .await?;
        }
    }

    Ok(())
}

pub(super) async fn delete_template(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    idx: usize,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let result = cfg
        .prefs
        .update(user_id, |p| {
            if idx < p.templates.len() {
                p.templates.remove(idx);
            }
        })
        .await;
    if result.is_err() {
        bot.send_message(chat_id, i18n::t(locale, TextKey::PreferencesSaveError))
            .await?;
        return Ok(());
    }

    bot.send_message(chat_id, i18n::t(locale, TextKey::TemplateDeleted))
        .await?;
    show_template_list(bot, chat_id, user_id, cfg, locale).await
}
