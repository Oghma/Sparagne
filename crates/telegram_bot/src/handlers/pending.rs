use engine::{Currency as EngineCurrency, Money};
use reqwest::StatusCode;
use teloxide::{
    prelude::*,
    types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup, Message},
};
use uuid::Uuid;

use crate::{
    ConfigParameters,
    api::ApiError,
    i18n::{self, TextKey},
    parsing::{ParseError, QuickKind, parse_quick_add, suggest_category},
    state::{DraftCreate, MAX_TEMPLATES, PendingAction, TransactionTemplate},
    text,
    use_cases::{home, shared, wizard},
};

use super::templates;

pub(super) async fn handle_pending_message(
    bot: &Bot,
    msg: &Message,
    cfg: &ConfigParameters,
    user_id: u64,
    pending: PendingAction,
    locale: i18n::Locale,
) -> ResponseResult<bool> {
    let chat_id = msg.chat.id;
    match pending {
        PendingAction::PairCode => {
            let Some(code) = msg.text().map(str::trim).filter(|c| !c.is_empty()) else {
                return Ok(true);
            };
            if let Err(err) = cfg.api.pair_user(user_id, code).await {
                bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                    .await?;
                return Ok(true);
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

            home::show_home(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
        PendingAction::WizardDraft { kind } => {
            let Some(text) = msg.text() else {
                return Ok(true);
            };

            let input = match wizard::normalize_wizard_input(locale, kind, text) {
                Ok(v) => v,
                Err(err) => {
                    bot.send_message(chat_id, err).await?;
                    return Ok(true);
                }
            };

            let mut parsed = match parse_quick_add(&input, EngineCurrency::Eur) {
                Ok(v) => v,
                Err(ParseError::Empty) => return Ok(true),
                Err(ParseError::TooManyTags) => {
                    bot.send_message(chat_id, i18n::t(locale, TextKey::TooManyTags))
                        .await?;
                    return Ok(true);
                }
                Err(ParseError::InvalidAmount) => {
                    bot.send_message(chat_id, i18n::t(locale, TextKey::InvalidAmountExample))
                        .await?;
                    return Ok(true);
                }
            };

            cfg.sessions.update(chat_id, |s| s.pending = None).await;

            let prefs = cfg.prefs.get_or_default(user_id).await;

            // Smart category suggestion: if no category specified, suggest based on note
            if parsed.category.is_none()
                && let Some(suggested) =
                    suggest_category(parsed.note.as_deref(), &prefs.category_hints)
            {
                parsed.category = Some(suggested.to_string());
            }

            let category = parsed.category;
            let Some(wallet_id) = prefs.default_wallet_id else {
                home::show_wallet_picker(bot, chat_id, user_id, cfg, locale, "wiz:cancel").await?;
                return Ok(true);
            };

            let vault_ref = shared::vault_ref_from_prefs(&prefs);
            let snapshot = match cfg.api.vault_snapshot(user_id, &vault_ref).await {
                Ok(s) => s,
                Err(err) => {
                    bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                        .await?;
                    return Ok(true);
                }
            };
            if !snapshot.wallets.iter().any(|w| w.id == wallet_id) {
                let _ = cfg
                    .prefs
                    .update(user_id, |p| p.default_wallet_id = None)
                    .await;
                home::show_wallet_picker(bot, chat_id, user_id, cfg, locale, "wiz:cancel").await?;
                return Ok(true);
            }

            let flow_id = prefs
                .last_flow_id
                .filter(|id| snapshot.flows.iter().any(|f| f.id == *id))
                .or(Some(snapshot.unallocated_flow_id));
            let idempotency_key = format!("tg:{}:{}", msg.chat.id.0, msg.id.0);
            let occurred_at = shared::now_rome();

            let created = match kind {
                QuickKind::Expense => {
                    cfg.api
                        .create_expense(
                            user_id,
                            &api_types::transaction::ExpenseNew {
                                vault_id: snapshot.id.clone(),
                                amount_minor: parsed.amount_minor,
                                flow_id,
                                wallet_id: Some(wallet_id),
                                category_id: None,
                                category,
                                note: parsed.note,
                                idempotency_key: Some(idempotency_key),
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
                                vault_id: snapshot.id.clone(),
                                amount_minor: parsed.amount_minor,
                                flow_id,
                                wallet_id: Some(wallet_id),
                                category_id: None,
                                category,
                                note: parsed.note,
                                idempotency_key: Some(idempotency_key),
                                occurred_at,
                            },
                        )
                        .await
                }
            };

            match created {
                Ok(created) => {
                    let currency = shared::engine_currency(snapshot.currency);
                    let signed_minor = match kind {
                        QuickKind::Expense => -parsed.amount_minor,
                        QuickKind::Income => parsed.amount_minor,
                    };
                    let saved_msg = i18n::format(
                        locale,
                        TextKey::QuickAddSaved,
                        &[("amount", &Money::new(signed_minor).format(currency))],
                    );
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
                Err(ApiError::Server { status, .. }) if status == StatusCode::CONFLICT => {
                    bot.send_message(chat_id, i18n::t(locale, TextKey::AlreadySaved))
                        .await?;
                }
                Err(err) => {
                    bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                        .await?;
                }
            }

            wizard::show_wizard(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
        PendingAction::EditAmount { tx_id } => {
            let Some(text) = msg.text() else {
                return Ok(true);
            };
            let money = match Money::parse_major(text, EngineCurrency::Eur) {
                Ok(v) => v,
                Err(_) => {
                    bot.send_message(chat_id, i18n::t(locale, TextKey::InvalidAmountExampleShort))
                        .await?;
                    return Ok(true);
                }
            };
            let amount_minor = match money.minor().checked_abs() {
                Some(v) => v,
                None => {
                    bot.send_message(chat_id, i18n::t(locale, TextKey::InvalidAmount))
                        .await?;
                    return Ok(true);
                }
            };
            if amount_minor == 0 {
                bot.send_message(chat_id, i18n::t(locale, TextKey::InvalidAmountPositive))
                    .await?;
                return Ok(true);
            }

            let prefs = cfg.prefs.get_or_default(user_id).await;
            let vault_ref = shared::vault_ref_from_prefs(&prefs);
            let vault_id = match shared::resolve_vault_id(&cfg.api, user_id, &vault_ref).await {
                Ok(v) => v,
                Err(err) => {
                    bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                        .await?;
                    return Ok(true);
                }
            };
            if let Err(err) = cfg
                .api
                .update_transaction(
                    user_id,
                    tx_id,
                    &api_types::transaction::TransactionUpdate {
                        vault_id,
                        amount_minor: Some(amount_minor),
                        wallet_id: None,
                        flow_id: None,
                        from_wallet_id: None,
                        to_wallet_id: None,
                        from_flow_id: None,
                        to_flow_id: None,
                        category_id: None,
                        category: None,
                        note: None,
                        occurred_at: None,
                    },
                )
                .await
            {
                bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                    .await?;
                return Ok(true);
            }

            cfg.sessions.update(chat_id, |s| s.pending = None).await;
            bot.send_message(chat_id, i18n::t(locale, TextKey::EditAmountUpdated))
                .await?;
            home::show_home(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
        PendingAction::EditNote { tx_id } => {
            let note = msg
                .text()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty());

            let prefs = cfg.prefs.get_or_default(user_id).await;
            let vault_ref = shared::vault_ref_from_prefs(&prefs);
            let vault_id = match shared::resolve_vault_id(&cfg.api, user_id, &vault_ref).await {
                Ok(v) => v,
                Err(err) => {
                    bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                        .await?;
                    return Ok(true);
                }
            };
            if let Err(err) = cfg
                .api
                .update_transaction(
                    user_id,
                    tx_id,
                    &api_types::transaction::TransactionUpdate {
                        vault_id,
                        amount_minor: None,
                        wallet_id: None,
                        flow_id: None,
                        from_wallet_id: None,
                        to_wallet_id: None,
                        from_flow_id: None,
                        to_flow_id: None,
                        category_id: None,
                        category: None,
                        note,
                        occurred_at: None,
                    },
                )
                .await
            {
                bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                    .await?;
                return Ok(true);
            }

            cfg.sessions.update(chat_id, |s| s.pending = None).await;
            bot.send_message(chat_id, i18n::t(locale, TextKey::EditNoteUpdated))
                .await?;
            home::show_home(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
        PendingAction::WalletForQuickAdd(_) => Ok(false),
        PendingAction::TemplateCreate => {
            let Some(text) = msg.text() else {
                return Ok(true);
            };
            // Parse template: "name | amount [#category] [note]"
            let Some((name, quick_part)) = text.split_once('|') else {
                bot.send_message(chat_id, i18n::t(locale, TextKey::TemplateInvalid))
                    .await?;
                return Ok(true);
            };
            let name = name.trim();
            let quick_part = quick_part.trim();

            if name.is_empty() || quick_part.is_empty() {
                bot.send_message(chat_id, i18n::t(locale, TextKey::TemplateInvalid))
                    .await?;
                return Ok(true);
            }

            let parsed = match parse_quick_add(quick_part, EngineCurrency::Eur) {
                Ok(v) => v,
                Err(_) => {
                    bot.send_message(chat_id, i18n::t(locale, TextKey::TemplateInvalid))
                        .await?;
                    return Ok(true);
                }
            };

            let template = TransactionTemplate {
                name: name.to_string(),
                amount_minor: parsed.amount_minor,
                category: parsed.category,
                note: parsed.note,
                kind: parsed.kind,
            };

            let result = cfg
                .prefs
                .update(user_id, |p| {
                    if p.templates.len() < MAX_TEMPLATES {
                        p.templates.push(template.clone());
                    }
                })
                .await;
            if result.is_err() {
                bot.send_message(chat_id, i18n::t(locale, TextKey::PreferencesSaveError))
                    .await?;
                return Ok(true);
            }

            cfg.sessions.update(chat_id, |s| s.pending = None).await;
            bot.send_message(chat_id, i18n::t(locale, TextKey::TemplateCreated))
                .await?;
            templates::show_template_list(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
    }
}

pub(super) async fn handle_quick_add(
    bot: &Bot,
    msg: &Message,
    cfg: &ConfigParameters,
    user_id: u64,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let Some(text) = msg.text() else {
        return Ok(());
    };
    let mut parsed = match parse_quick_add(text, EngineCurrency::Eur) {
        Ok(v) => v,
        Err(ParseError::Empty) => return Ok(()),
        Err(ParseError::TooManyTags) => {
            bot.send_message(msg.chat.id, i18n::t(locale, TextKey::TooManyTags))
                .await?;
            return Ok(());
        }
        Err(ParseError::InvalidAmount) => {
            bot.send_message(msg.chat.id, i18n::t(locale, TextKey::InvalidAmountExample))
                .await?;
            return Ok(());
        }
    };

    let prefs = cfg.prefs.get_or_default(user_id).await;

    // Smart category suggestion: if no category specified, suggest based on note
    if parsed.category.is_none()
        && let Some(suggested) = suggest_category(parsed.note.as_deref(), &prefs.category_hints)
    {
        parsed.category = Some(suggested.to_string());
    }

    let idempotency_key = format!("tg:{}:{}", msg.chat.id.0, msg.id.0);
    let draft: DraftCreate = (parsed, idempotency_key).into();

    let Some(wallet_id) = prefs.default_wallet_id else {
        cfg.sessions
            .update(msg.chat.id, |s| {
                s.pending = Some(PendingAction::WalletForQuickAdd(draft.clone()))
            })
            .await;
        home::show_wallet_picker(bot, msg.chat.id, user_id, cfg, locale, "nav:home").await?;
        return Ok(());
    };

    finalize_quick_add(bot, msg.chat.id, user_id, cfg, wallet_id, draft, locale).await
}

pub(super) async fn finalize_quick_add(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    wallet_id: Uuid,
    draft: DraftCreate,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let prefs = cfg.prefs.get_or_default(user_id).await;
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
        home::show_wallet_picker(bot, chat_id, user_id, cfg, locale, "nav:home").await?;
        return Ok(());
    }

    let currency = shared::engine_currency(snapshot.currency);
    let flow_id = match prefs
        .last_flow_id
        .filter(|id| snapshot.flows.iter().any(|f| f.id == *id))
    {
        Some(id) => id,
        None => {
            let id = snapshot.unallocated_flow_id;
            let _ = cfg
                .prefs
                .update(user_id, |p| {
                    p.last_flow_id = Some(id);
                    p.default_flow_id = Some(id);
                })
                .await;
            id
        }
    };

    let occurred_at = shared::now_rome();
    let vault_id = snapshot.id.clone();

    let created = match draft.kind {
        QuickKind::Expense => {
            cfg.api
                .create_expense(
                    user_id,
                    &api_types::transaction::ExpenseNew {
                        vault_id,
                        amount_minor: draft.amount_minor,
                        flow_id: Some(flow_id),
                        wallet_id: Some(wallet_id),
                        category_id: None,
                        category: draft.category.clone(),
                        note: draft.note.clone(),
                        idempotency_key: Some(draft.idempotency_key.clone()),
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
                        amount_minor: draft.amount_minor,
                        flow_id: Some(flow_id),
                        wallet_id: Some(wallet_id),
                        category_id: None,
                        category: draft.category.clone(),
                        note: draft.note.clone(),
                        idempotency_key: Some(draft.idempotency_key.clone()),
                        occurred_at,
                    },
                )
                .await
        }
    };

    match created {
        Ok(created) => {
            let signed_minor = match draft.kind {
                QuickKind::Expense => -draft.amount_minor,
                QuickKind::Income => draft.amount_minor,
            };

            let mut saved_msg = i18n::format(
                locale,
                TextKey::QuickAddSaved,
                &[("amount", &Money::new(signed_minor).format(currency))],
            );
            if let Some(category) = draft.category.as_deref() {
                saved_msg.push_str(&format!(" \u{2022} {category}"));
            }
            if let Some(note) = draft.note.as_deref() {
                saved_msg.push_str(&format!(" \u{2022} {note}"));
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
        Err(ApiError::Server { status, .. }) if status == StatusCode::CONFLICT => {
            bot.send_message(chat_id, i18n::t(locale, TextKey::AlreadySaved))
                .await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                .await?;
        }
    }

    Ok(())
}
