use chrono::{DateTime, FixedOffset, Utc};
use chrono_tz::Europe::Rome;
use engine::{Currency as EngineCurrency, Money};
use reqwest::StatusCode;
use teloxide::{
    prelude::*,
    types::{CallbackQuery, ChatId, InlineKeyboardButton, InlineKeyboardMarkup, User},
};
use uuid::Uuid;

use crate::{
    ConfigParameters,
    api::{ApiClient, ApiError},
    parsing::{ParseError, QuickKind, parse_quick_add},
    state::{DraftCreate, PendingAction, WizardSession},
    ui,
};

pub(crate) async fn handle_message(
    bot: Bot,
    msg: Message,
    cfg: ConfigParameters,
) -> ResponseResult<()> {
    if !is_allowed(&cfg, msg.from.as_ref()) {
        return Ok(());
    }

    let Some(from) = msg.from.as_ref() else {
        bot.send_message(msg.chat.id, "Impossibile identificare l'utente.")
            .await?;
        return Ok(());
    };
    let user_id = from.id.0;
    let chat_id = msg.chat.id;

    // If we are waiting for an input (pair/edit), handle it first.
    if let Some(pending) = cfg.sessions.get(chat_id).await.pending
        && handle_pending_message(&bot, &msg, &cfg, user_id, pending).await?
    {
        return Ok(());
    }

    let Some(text) = msg.text() else {
        return Ok(());
    };

    if let Some(cmd) = parse_command(text) {
        match cmd {
            Command::Start { code } => {
                if let Some(code) = code.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
                    if let Err(err) = cfg.api.pair_user(user_id, code).await {
                        bot.send_message(chat_id, user_message_for_api_error(err))
                            .await?;
                        return Ok(());
                    }

                    cfg.sessions.update(chat_id, |s| s.pending = None).await;
                    bot.send_message(chat_id, welcome_text()).await?;
                    show_home(&bot, chat_id, user_id, &cfg).await?;
                    return Ok(());
                }

                show_home(&bot, chat_id, user_id, &cfg).await?;
                return Ok(());
            }
            Command::Home => {
                cfg.sessions.update(chat_id, |s| s.wizard = None).await;
                show_home(&bot, chat_id, user_id, &cfg).await?;
                return Ok(());
            }
            Command::Help => {
                bot.send_message(chat_id, help_text()).await?;
                return Ok(());
            }
        }
    }

    if looks_like_quick_add(text) {
        handle_quick_add(&bot, &msg, &cfg, user_id).await?;
    }

    Ok(())
}

async fn start_wizard(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    kind: QuickKind,
) -> ResponseResult<()> {
    cfg.sessions
        .update(chat_id, |s| {
            s.wizard = Some(WizardSession {
                kind,
                category: None,
                categories: Vec::new(),
            });
        })
        .await;
    show_wizard(bot, chat_id, user_id, cfg).await
}

async fn show_wizard(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
) -> ResponseResult<()> {
    let session = cfg.sessions.get(chat_id).await;
    let Some(wizard) = session.wizard else {
        return show_home(bot, chat_id, user_id, cfg).await;
    };

    let snapshot = match cfg.api.vault_snapshot_main(user_id).await {
        Ok(s) => s,
        Err(err) => {
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };
    let currency = engine_currency(snapshot.currency);

    let mut prefs = cfg.prefs.get_or_default(user_id).await;
    if (prefs.last_flow_id.is_none() || prefs.default_flow_id.is_none())
        && let Ok(updated) = cfg
            .prefs
            .update(user_id, |p| {
                if p.last_flow_id.is_none() {
                    p.last_flow_id = Some(snapshot.unallocated_flow_id);
                }
                if p.default_flow_id.is_none() {
                    p.default_flow_id = Some(snapshot.unallocated_flow_id);
                }
            })
            .await
    {
        prefs = updated;
    }
    let Some(wallet_id) = prefs.default_wallet_id else {
        show_wallet_picker(bot, chat_id, user_id, cfg).await?;
        return Ok(());
    };

    let kind_filter = match wizard.kind {
        QuickKind::Expense => api_types::transaction::TransactionKind::Expense,
        QuickKind::Income => api_types::transaction::TransactionKind::Income,
        QuickKind::Refund => api_types::transaction::TransactionKind::Refund,
    };

    let recents = match cfg
        .api
        .transactions_list(
            user_id,
            &api_types::transaction::TransactionList {
                vault_id: snapshot.id.clone(),
                flow_id: None,
                wallet_id: Some(wallet_id),
                limit: Some(6),
                cursor: None,
                from: None,
                to: None,
                kinds: Some(vec![kind_filter]),
                include_voided: Some(false),
                include_transfers: Some(false),
            },
        )
        .await
    {
        Ok(v) => v,
        Err(err) => {
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let mut categories: Vec<String> = Vec::new();
    for tx in &recents.transactions {
        let Some(cat) = tx.category.as_deref() else {
            continue;
        };
        if categories.iter().any(|c| c == cat) {
            continue;
        }
        categories.push(cat.to_string());
        if categories.len() >= 6 {
            break;
        }
    }

    let session = cfg
        .sessions
        .update(chat_id, |s| {
            if let Some(w) = &mut s.wizard {
                w.categories = categories;
            }
        })
        .await;
    let Some(wizard) = session.wizard else {
        return show_home(bot, chat_id, user_id, cfg).await;
    };

    let (text, kb) = ui::render_wizard(currency, &snapshot, &prefs, &wizard, &recents.transactions);
    edit_or_send(bot, chat_id, cfg, text, kb).await
}

pub(crate) async fn handle_callback(
    bot: Bot,
    q: CallbackQuery,
    cfg: ConfigParameters,
) -> ResponseResult<()> {
    if !is_allowed(&cfg, Some(&q.from)) {
        return Ok(());
    }

    let Some(message) = q.message.as_ref() else {
        return Ok(());
    };
    let chat_id = message.chat().id;
    let user_id = q.from.id.0;

    let _ = bot.answer_callback_query(q.id.clone()).await;

    let Some(data) = q.data.as_deref() else {
        return Ok(());
    };

    if data == "nav:home" {
        cfg.sessions.update(chat_id, |s| s.wizard = None).await;
        show_home(&bot, chat_id, user_id, &cfg).await?;
    } else if data == "nav:wizard" {
        show_wizard(&bot, chat_id, user_id, &cfg).await?;
    } else if data == "home:pair" {
        cfg.sessions
            .update(chat_id, |s| s.pending = Some(PendingAction::PairCode))
            .await;
        bot.send_message(chat_id, "Inserisci il codice di pairing:")
            .await?;
    } else if data == "home:pick_wallet" {
        show_wallet_picker(&bot, chat_id, user_id, &cfg).await?;
    } else if data == "home:pick_flow" {
        show_flow_picker(&bot, chat_id, user_id, &cfg).await?;
    } else if data == "home:expense" {
        start_wizard(&bot, chat_id, user_id, &cfg, QuickKind::Expense).await?;
    } else if data == "home:income" {
        start_wizard(&bot, chat_id, user_id, &cfg, QuickKind::Income).await?;
    } else if data == "home:refund" {
        start_wizard(&bot, chat_id, user_id, &cfg, QuickKind::Refund).await?;
    } else if data == "home:list" || data == "nav:list" {
        show_list(&bot, chat_id, user_id, &cfg).await?;
    } else if data == "home:stats" {
        show_stats(&bot, chat_id, user_id, &cfg).await?;
    } else if data == "wiz:close" {
        cfg.sessions.update(chat_id, |s| s.wizard = None).await;
        show_home(&bot, chat_id, user_id, &cfg).await?;
    } else if data == "wiz:pick_wallet" {
        show_wallet_picker(&bot, chat_id, user_id, &cfg).await?;
    } else if data == "wiz:pick_flow" {
        show_flow_picker(&bot, chat_id, user_id, &cfg).await?;
    } else if data == "wiz:input" {
        let kind = cfg
            .sessions
            .get(chat_id)
            .await
            .wizard
            .as_ref()
            .map(|w| w.kind);
        let Some(kind) = kind else {
            show_home(&bot, chat_id, user_id, &cfg).await?;
            return Ok(());
        };

        cfg.sessions
            .update(chat_id, |s| {
                s.pending = Some(PendingAction::WizardDraft { kind })
            })
            .await;
        bot.send_message(chat_id, wizard_prompt(kind)).await?;
    } else if data == "wiz:cat:none" || data == "wiz:cat:reset" {
        cfg.sessions
            .update(chat_id, |s| {
                if let Some(w) = &mut s.wizard {
                    w.category = None;
                }
            })
            .await;
        show_wizard(&bot, chat_id, user_id, &cfg).await?;
    } else if let Some(idx) = data.strip_prefix("wiz:cat:") {
        let Ok(idx) = idx.parse::<usize>() else {
            return Ok(());
        };
        cfg.sessions
            .update(chat_id, |s| {
                let Some(w) = &mut s.wizard else {
                    return;
                };
                let Some(cat) = w.categories.get(idx).cloned() else {
                    return;
                };
                w.category = Some(cat);
            })
            .await;
        show_wizard(&bot, chat_id, user_id, &cfg).await?;
    } else if let Some(tx_id) = data.strip_prefix("wiz:recent:") {
        let Ok(tx_id) = Uuid::parse_str(tx_id) else {
            bot.send_message(chat_id, "Transazione non valida.").await?;
            return Ok(());
        };
        repeat_transaction(&bot, chat_id, user_id, &cfg, tx_id, q.id.0.as_str()).await?;
        show_wizard(&bot, chat_id, user_id, &cfg).await?;
    } else if data == "prefs:toggle_voided" {
        let updated = cfg
            .prefs
            .update(user_id, |p| p.include_voided = !p.include_voided)
            .await;
        if updated.is_err() {
            bot.send_message(chat_id, "Errore nel salvataggio delle preferenze.")
                .await?;
        }
        show_list(&bot, chat_id, user_id, &cfg).await?;
    } else if data == "list:next" {
        cfg.sessions
            .update(chat_id, |s| {
                if let Some(list) = &mut s.list
                    && list.next.is_some()
                {
                    list.cursors.push(list.current.clone());
                    list.current = list.next.clone();
                }
            })
            .await;
        show_list(&bot, chat_id, user_id, &cfg).await?;
    } else if data == "list:prev" {
        cfg.sessions
            .update(chat_id, |s| {
                if let Some(list) = &mut s.list {
                    list.current = list.cursors.pop().unwrap_or(None);
                }
            })
            .await;
        show_list(&bot, chat_id, user_id, &cfg).await?;
    } else if let Some(wallet_id) = data.strip_prefix("wallet:set:") {
        let Ok(wallet_id) = Uuid::parse_str(wallet_id) else {
            bot.send_message(chat_id, "Wallet non valido.").await?;
            return Ok(());
        };

        let updated = cfg
            .prefs
            .update(user_id, |p| p.default_wallet_id = Some(wallet_id))
            .await;
        if updated.is_err() {
            bot.send_message(chat_id, "Errore nel salvataggio delle preferenze.")
                .await?;
        }

        let pending = cfg.sessions.get(chat_id).await.pending;
        if let Some(PendingAction::WalletForQuickAdd(draft)) = pending {
            cfg.sessions.update(chat_id, |s| s.pending = None).await;
            finalize_quick_add(&bot, chat_id, user_id, &cfg, wallet_id, draft).await?;
            show_home(&bot, chat_id, user_id, &cfg).await?;
            return Ok(());
        }

        if cfg.sessions.get(chat_id).await.wizard.is_some() {
            show_wizard(&bot, chat_id, user_id, &cfg).await?;
        } else {
            show_home(&bot, chat_id, user_id, &cfg).await?;
        }
    } else if let Some(flow_id) = data.strip_prefix("flow:set:") {
        let Ok(flow_id) = Uuid::parse_str(flow_id) else {
            bot.send_message(chat_id, "Flow non valido.").await?;
            return Ok(());
        };

        let updated = cfg
            .prefs
            .update(user_id, |p| {
                p.last_flow_id = Some(flow_id);
                p.default_flow_id = Some(flow_id);
            })
            .await;
        if updated.is_err() {
            bot.send_message(chat_id, "Errore nel salvataggio delle preferenze.")
                .await?;
        }
        if cfg.sessions.get(chat_id).await.wizard.is_some() {
            show_wizard(&bot, chat_id, user_id, &cfg).await?;
        } else {
            show_home(&bot, chat_id, user_id, &cfg).await?;
        }
    } else if let Some(tx_id) = data.strip_prefix("tx:detail:") {
        let Ok(tx_id) = Uuid::parse_str(tx_id) else {
            bot.send_message(chat_id, "Transazione non valida.").await?;
            return Ok(());
        };
        show_detail(&bot, chat_id, user_id, &cfg, tx_id).await?;
    } else if let Some(tx_id) = data.strip_prefix("tx:void:") {
        let Ok(tx_id) = Uuid::parse_str(tx_id) else {
            bot.send_message(chat_id, "Transazione non valida.").await?;
            return Ok(());
        };

        let vault_id = match resolve_main_vault_id(&cfg.api, user_id).await {
            Ok(vault_id) => vault_id,
            Err(err) => {
                bot.send_message(chat_id, user_message_for_api_error(err))
                    .await?;
                return Ok(());
            }
        };

        let voided = cfg
            .api
            .void_transaction(
                user_id,
                tx_id,
                &api_types::transaction::TransactionVoid {
                    vault_id,
                    voided_at: None,
                },
            )
            .await;
        if let Err(err) = voided {
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?;
            return Ok(());
        }

        bot.send_message(chat_id, "✅ Voce annullata (void).")
            .await?;
        show_list(&bot, chat_id, user_id, &cfg).await?;
    } else if let Some(tx_id) = data.strip_prefix("tx:edit:") {
        let Ok(tx_id) = Uuid::parse_str(tx_id) else {
            bot.send_message(chat_id, "Transazione non valida.").await?;
            return Ok(());
        };
        let (text, kb) = ui::render_edit_menu(tx_id);
        edit_or_send(&bot, chat_id, &cfg, text, kb).await?;
    } else if let Some(tx_id) = data.strip_prefix("tx:edit_amount:") {
        let Ok(tx_id) = Uuid::parse_str(tx_id) else {
            bot.send_message(chat_id, "Transazione non valida.").await?;
            return Ok(());
        };
        cfg.sessions
            .update(chat_id, |s| {
                s.pending = Some(PendingAction::EditAmount { tx_id })
            })
            .await;
        bot.send_message(chat_id, "Invia il nuovo importo (es: 10.50):")
            .await?;
    } else if let Some(tx_id) = data.strip_prefix("tx:edit_note:") {
        let Ok(tx_id) = Uuid::parse_str(tx_id) else {
            bot.send_message(chat_id, "Transazione non valida.").await?;
            return Ok(());
        };
        cfg.sessions
            .update(chat_id, |s| {
                s.pending = Some(PendingAction::EditNote { tx_id })
            })
            .await;
        bot.send_message(chat_id, "Invia la nuova nota (vuoto per rimuovere):")
            .await?;
    } else if let Some(tx_id) = data.strip_prefix("tx:repeat:") {
        let Ok(tx_id) = Uuid::parse_str(tx_id) else {
            bot.send_message(chat_id, "Transazione non valida.").await?;
            return Ok(());
        };
        repeat_transaction(&bot, chat_id, user_id, &cfg, tx_id, q.id.0.as_str()).await?;
    }

    Ok(())
}

async fn handle_pending_message(
    bot: &Bot,
    msg: &Message,
    cfg: &ConfigParameters,
    user_id: u64,
    pending: PendingAction,
) -> ResponseResult<bool> {
    let chat_id = msg.chat.id;
    match pending {
        PendingAction::PairCode => {
            let Some(code) = msg.text().map(str::trim).filter(|c| !c.is_empty()) else {
                return Ok(true);
            };
            if let Err(err) = cfg.api.pair_user(user_id, code).await {
                bot.send_message(chat_id, user_message_for_api_error(err))
                    .await?;
                return Ok(true);
            }

            cfg.sessions.update(chat_id, |s| s.pending = None).await;
            bot.send_message(chat_id, welcome_text()).await?;
            show_home(bot, chat_id, user_id, cfg).await?;
            Ok(true)
        }
        PendingAction::WizardDraft { kind } => {
            let Some(text) = msg.text() else {
                return Ok(true);
            };

            let input = match normalize_wizard_input(kind, text) {
                Ok(v) => v,
                Err(err) => {
                    bot.send_message(chat_id, err).await?;
                    return Ok(true);
                }
            };

            let parsed = match parse_quick_add(&input, EngineCurrency::Eur) {
                Ok(v) => v,
                Err(ParseError::Empty) => return Ok(true),
                Err(ParseError::TooManyTags) => {
                    bot.send_message(chat_id, "Troppi tag: massimo 1.").await?;
                    return Ok(true);
                }
                Err(ParseError::InvalidAmount) => {
                    bot.send_message(chat_id, "Importo non valido (es: 10 o 10.50).")
                        .await?;
                    return Ok(true);
                }
            };

            let session = cfg.sessions.get(chat_id).await;
            let selected_category = session.wizard.as_ref().and_then(|w| w.category.clone());
            let category = parsed.category.or(selected_category);

            cfg.sessions.update(chat_id, |s| s.pending = None).await;

            let prefs = cfg.prefs.get_or_default(user_id).await;
            let Some(wallet_id) = prefs.default_wallet_id else {
                show_wallet_picker(bot, chat_id, user_id, cfg).await?;
                return Ok(true);
            };

            let snapshot = match cfg.api.vault_snapshot_main(user_id).await {
                Ok(s) => s,
                Err(err) => {
                    bot.send_message(chat_id, user_message_for_api_error(err))
                        .await?;
                    return Ok(true);
                }
            };

            let flow_id = prefs.last_flow_id.or(Some(snapshot.unallocated_flow_id));
            let idempotency_key = format!("tg:{}:{}", msg.chat.id.0, msg.id.0);
            let occurred_at = now_rome();

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
                                category,
                                note: parsed.note,
                                idempotency_key: Some(idempotency_key),
                                occurred_at,
                            },
                        )
                        .await
                }
                QuickKind::Refund => {
                    cfg.api
                        .create_refund(
                            user_id,
                            &api_types::transaction::Refund {
                                vault_id: snapshot.id.clone(),
                                amount_minor: parsed.amount_minor,
                                flow_id,
                                wallet_id: Some(wallet_id),
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
                    let currency = engine_currency(snapshot.currency);
                    let signed_minor = match kind {
                        QuickKind::Expense => -parsed.amount_minor,
                        QuickKind::Income | QuickKind::Refund => parsed.amount_minor,
                    };
                    let saved_msg =
                        format!("✅ Salvato: {}", Money::new(signed_minor).format(currency));
                    let kb = InlineKeyboardMarkup::new(vec![vec![
                        InlineKeyboardButton::callback(
                            "↩ Undo",
                            format!("tx:void:{id}", id = created.id),
                        ),
                        InlineKeyboardButton::callback(
                            "✏️ Edit",
                            format!("tx:edit:{id}", id = created.id),
                        ),
                        InlineKeyboardButton::callback(
                            "📌 Ripeti",
                            format!("tx:repeat:{id}", id = created.id),
                        ),
                    ]]);
                    bot.send_message(chat_id, saved_msg)
                        .reply_markup(kb)
                        .await?;
                }
                Err(ApiError::Server { status, .. }) if status == StatusCode::CONFLICT => {
                    bot.send_message(chat_id, "✅ Già salvato.").await?;
                }
                Err(err) => {
                    bot.send_message(chat_id, user_message_for_api_error(err))
                        .await?;
                }
            }

            show_wizard(bot, chat_id, user_id, cfg).await?;
            Ok(true)
        }
        PendingAction::EditAmount { tx_id } => {
            let Some(text) = msg.text() else {
                return Ok(true);
            };
            let money = match Money::parse_major(text, EngineCurrency::Eur) {
                Ok(v) => v,
                Err(_) => {
                    bot.send_message(chat_id, "Importo non valido (es: 10 o 10.50)")
                        .await?;
                    return Ok(true);
                }
            };
            let amount_minor = match money.minor().checked_abs() {
                Some(v) => v,
                None => {
                    bot.send_message(chat_id, "Importo non valido.").await?;
                    return Ok(true);
                }
            };
            if amount_minor == 0 {
                bot.send_message(chat_id, "Importo non valido (deve essere > 0).")
                    .await?;
                return Ok(true);
            }

            let vault_id = match resolve_main_vault_id(&cfg.api, user_id).await {
                Ok(v) => v,
                Err(err) => {
                    bot.send_message(chat_id, user_message_for_api_error(err))
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
                        category: None,
                        note: None,
                        occurred_at: None,
                    },
                )
                .await
            {
                bot.send_message(chat_id, user_message_for_api_error(err))
                    .await?;
                return Ok(true);
            }

            cfg.sessions.update(chat_id, |s| s.pending = None).await;
            bot.send_message(chat_id, "✅ Importo aggiornato.").await?;
            show_home(bot, chat_id, user_id, cfg).await?;
            Ok(true)
        }
        PendingAction::EditNote { tx_id } => {
            let note = msg
                .text()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty());

            let vault_id = match resolve_main_vault_id(&cfg.api, user_id).await {
                Ok(v) => v,
                Err(err) => {
                    bot.send_message(chat_id, user_message_for_api_error(err))
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
                        category: None,
                        note,
                        occurred_at: None,
                    },
                )
                .await
            {
                bot.send_message(chat_id, user_message_for_api_error(err))
                    .await?;
                return Ok(true);
            }

            cfg.sessions.update(chat_id, |s| s.pending = None).await;
            bot.send_message(chat_id, "✅ Nota aggiornata.").await?;
            show_home(bot, chat_id, user_id, cfg).await?;
            Ok(true)
        }
        PendingAction::WalletForQuickAdd(_) => Ok(false),
    }
}

async fn handle_quick_add(
    bot: &Bot,
    msg: &Message,
    cfg: &ConfigParameters,
    user_id: u64,
) -> ResponseResult<()> {
    let Some(text) = msg.text() else {
        return Ok(());
    };
    let parsed = match parse_quick_add(text, EngineCurrency::Eur) {
        Ok(v) => v,
        Err(ParseError::Empty) => return Ok(()),
        Err(ParseError::TooManyTags) => {
            bot.send_message(msg.chat.id, "Troppi tag: massimo 1.")
                .await?;
            return Ok(());
        }
        Err(ParseError::InvalidAmount) => {
            bot.send_message(msg.chat.id, "Importo non valido (es: 10 o 10.50).")
                .await?;
            return Ok(());
        }
    };

    let prefs = cfg.prefs.get_or_default(user_id).await;
    let idempotency_key = format!("tg:{}:{}", msg.chat.id.0, msg.id.0);
    let draft: DraftCreate = (parsed, idempotency_key).into();

    let Some(wallet_id) = prefs.default_wallet_id else {
        cfg.sessions
            .update(msg.chat.id, |s| {
                s.pending = Some(PendingAction::WalletForQuickAdd(draft.clone()))
            })
            .await;
        show_wallet_picker(bot, msg.chat.id, user_id, cfg).await?;
        return Ok(());
    };

    finalize_quick_add(bot, msg.chat.id, user_id, cfg, wallet_id, draft).await
}

async fn finalize_quick_add(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    wallet_id: Uuid,
    draft: DraftCreate,
) -> ResponseResult<()> {
    let snapshot = match cfg.api.vault_snapshot_main(user_id).await {
        Ok(s) => s,
        Err(err) => {
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let currency = engine_currency(snapshot.currency);
    let prefs = cfg.prefs.get_or_default(user_id).await;
    let flow_id = match prefs.last_flow_id {
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

    let occurred_at = now_rome();
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
                        category: draft.category.clone(),
                        note: draft.note.clone(),
                        idempotency_key: Some(draft.idempotency_key.clone()),
                        occurred_at,
                    },
                )
                .await
        }
        QuickKind::Refund => {
            cfg.api
                .create_refund(
                    user_id,
                    &api_types::transaction::Refund {
                        vault_id,
                        amount_minor: draft.amount_minor,
                        flow_id: Some(flow_id),
                        wallet_id: Some(wallet_id),
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
                QuickKind::Income | QuickKind::Refund => draft.amount_minor,
            };

            let saved_msg = format!(
                "✅ Salvato: {}{}{}",
                Money::new(signed_minor).format(currency),
                draft
                    .category
                    .as_deref()
                    .map(|c| format!(" • {c}"))
                    .unwrap_or_default(),
                draft
                    .note
                    .as_deref()
                    .map(|n| format!(" • {n}"))
                    .unwrap_or_default(),
            );

            let kb = InlineKeyboardMarkup::new(vec![vec![
                InlineKeyboardButton::callback("↩ Undo", format!("tx:void:{id}", id = created.id)),
                InlineKeyboardButton::callback("✏️ Edit", format!("tx:edit:{id}", id = created.id)),
            ]]);

            bot.send_message(chat_id, saved_msg)
                .reply_markup(kb)
                .await?;
        }
        Err(ApiError::Server { status, .. }) if status == StatusCode::CONFLICT => {
            bot.send_message(chat_id, "✅ Già salvato.").await?;
        }
        Err(err) => {
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?;
        }
    }

    Ok(())
}

async fn show_home(
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
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?;
            if needs_pairing {
                cfg.sessions
                    .update(chat_id, |s| s.pending = Some(PendingAction::PairCode))
                    .await;
                bot.send_message(chat_id, "Per fare pairing: /start <codice>")
                    .await?;
            }
            return Ok(());
        }
    };
    let mut prefs = cfg.prefs.get_or_default(user_id).await;
    if (prefs.last_flow_id.is_none() || prefs.default_flow_id.is_none())
        && let Ok(updated) = cfg
            .prefs
            .update(user_id, |p| {
                if p.last_flow_id.is_none() {
                    p.last_flow_id = Some(snapshot.unallocated_flow_id);
                }
                if p.default_flow_id.is_none() {
                    p.default_flow_id = Some(snapshot.unallocated_flow_id);
                }
            })
            .await
    {
        prefs = updated;
    }
    let (text, kb) = ui::render_home(&snapshot, &prefs);
    edit_or_send(bot, chat_id, cfg, text, kb).await
}

async fn show_wallet_picker(
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
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?;
            if needs_pairing {
                cfg.sessions
                    .update(chat_id, |s| s.pending = Some(PendingAction::PairCode))
                    .await;
            }
            return Ok(());
        }
    };
    let (text, kb) = ui::render_wallet_picker(&snapshot, back_callback);
    edit_or_send(bot, chat_id, cfg, text, kb).await
}

async fn show_flow_picker(
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
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?;
            if needs_pairing {
                cfg.sessions
                    .update(chat_id, |s| s.pending = Some(PendingAction::PairCode))
                    .await;
            }
            return Ok(());
        }
    };
    let (text, kb) = ui::render_flow_picker(&snapshot, back_callback);
    edit_or_send(bot, chat_id, cfg, text, kb).await
}

async fn show_list(
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
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?;
            if needs_pairing {
                cfg.sessions
                    .update(chat_id, |s| s.pending = Some(PendingAction::PairCode))
                    .await;
            }
            return Ok(());
        }
    };
    let currency = engine_currency(snapshot.currency);
    let prefs = cfg.prefs.get_or_default(user_id).await;
    let Some(wallet_id) = prefs.default_wallet_id else {
        bot.send_message(chat_id, "Imposta prima un wallet di default.")
            .await?;
        show_wallet_picker(bot, chat_id, user_id, cfg).await?;
        return Ok(());
    };

    let session = cfg.sessions.get(chat_id).await;
    let (cursor, cursor_stack_len) = match session.list.as_ref() {
        Some(list) if list.wallet_id == wallet_id => (list.current.clone(), list.cursors.len()),
        _ => (None, 0),
    };

    let list = match cfg
        .api
        .transactions_list(
            user_id,
            &api_types::transaction::TransactionList {
                vault_id: snapshot.id.clone(),
                flow_id: None,
                wallet_id: Some(wallet_id),
                limit: Some(10),
                cursor,
                from: None,
                to: None,
                kinds: None,
                include_voided: Some(prefs.include_voided),
                include_transfers: Some(false),
            },
        )
        .await
    {
        Ok(v) => v,
        Err(err) => {
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let has_prev = cursor_stack_len > 0;
    let has_next = list.next_cursor.is_some();
    cfg.sessions
        .update(chat_id, |s| {
            let (cursors, current) = match s.list.as_ref() {
                Some(prev) if prev.wallet_id == wallet_id => {
                    (prev.cursors.clone(), prev.current.clone())
                }
                _ => (Vec::new(), None),
            };
            s.list = Some(crate::state::ListSession {
                wallet_id,
                cursors,
                current,
                next: list.next_cursor.clone(),
            });
        })
        .await;

    let (text, kb) = ui::render_list(currency, &list, prefs.include_voided, has_prev, has_next);
    edit_or_send(bot, chat_id, cfg, text, kb).await
}

async fn show_detail(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    tx_id: Uuid,
) -> ResponseResult<()> {
    let vault_id = match resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(v) => v,
        Err(err) => {
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };
    let detail = match cfg
        .api
        .transaction_get_detail(
            user_id,
            &api_types::transaction::TransactionGet {
                vault_id,
                id: tx_id,
            },
        )
        .await
    {
        Ok(v) => v,
        Err(err) => {
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };
    cfg.sessions
        .update(chat_id, |s| s.last_detail_tx = Some(tx_id))
        .await;
    let currency = engine_currency(detail.transaction.currency);
    let (text, kb) = ui::render_detail(currency, &detail);
    edit_or_send(bot, chat_id, cfg, text, kb).await
}

async fn show_stats(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
) -> ResponseResult<()> {
    let stats = match cfg.api.stats_get_main(user_id).await {
        Ok(s) => s,
        Err(err) => {
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };
    let currency = engine_currency(stats.currency);
    let (text, kb) = ui::render_stats(currency, &stats);
    edit_or_send(bot, chat_id, cfg, text, kb).await
}

async fn repeat_transaction(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    tx_id: Uuid,
    callback_id: &str,
) -> ResponseResult<()> {
    let vault_id = match resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(v) => v,
        Err(err) => {
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };
    let detail = match cfg
        .api
        .transaction_get_detail(
            user_id,
            &api_types::transaction::TransactionGet {
                vault_id: vault_id.clone(),
                id: tx_id,
            },
        )
        .await
    {
        Ok(v) => v,
        Err(err) => {
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let wallet_id = detail.legs.iter().find_map(|leg| match leg.target {
        api_types::transaction::LegTarget::Wallet { wallet_id } => Some(wallet_id),
        _ => None,
    });
    let flow_id = detail.legs.iter().find_map(|leg| match leg.target {
        api_types::transaction::LegTarget::Flow { flow_id } => Some(flow_id),
        _ => None,
    });

    let Some(wallet_id) = wallet_id else {
        bot.send_message(chat_id, "Transazione senza wallet: non posso ripeterla.")
            .await?;
        return Ok(());
    };

    let occurred_at = now_rome();
    let idempotency_key = format!("tgcb:{}:{callback_id}", chat_id.0);

    let created = match detail.transaction.kind {
        api_types::transaction::TransactionKind::Income => {
            cfg.api
                .create_income(
                    user_id,
                    &api_types::transaction::IncomeNew {
                        vault_id,
                        amount_minor: detail.transaction.amount_minor,
                        flow_id,
                        wallet_id: Some(wallet_id),
                        category: detail.transaction.category.clone(),
                        note: detail.transaction.note.clone(),
                        idempotency_key: Some(idempotency_key),
                        occurred_at,
                    },
                )
                .await
        }
        api_types::transaction::TransactionKind::Expense => {
            cfg.api
                .create_expense(
                    user_id,
                    &api_types::transaction::ExpenseNew {
                        vault_id,
                        amount_minor: detail.transaction.amount_minor,
                        flow_id,
                        wallet_id: Some(wallet_id),
                        category: detail.transaction.category.clone(),
                        note: detail.transaction.note.clone(),
                        idempotency_key: Some(idempotency_key),
                        occurred_at,
                    },
                )
                .await
        }
        api_types::transaction::TransactionKind::Refund => {
            cfg.api
                .create_refund(
                    user_id,
                    &api_types::transaction::Refund {
                        vault_id,
                        amount_minor: detail.transaction.amount_minor,
                        flow_id,
                        wallet_id: Some(wallet_id),
                        category: detail.transaction.category.clone(),
                        note: detail.transaction.note.clone(),
                        idempotency_key: Some(idempotency_key),
                        occurred_at,
                    },
                )
                .await
        }
        _ => {
            bot.send_message(chat_id, "Ripetizione non supportata per questo tipo.")
                .await?;
            return Ok(());
        }
    };

    match created {
        Ok(_) => bot.send_message(chat_id, "✅ Ripetuta.").await?,
        Err(ApiError::Server { status, .. }) if status == StatusCode::CONFLICT => {
            bot.send_message(chat_id, "✅ Già salvato.").await?
        }
        Err(err) => {
            bot.send_message(chat_id, user_message_for_api_error(err))
                .await?
        }
    };

    Ok(())
}

async fn edit_or_send(
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

async fn resolve_main_vault_id(api: &ApiClient, telegram_user_id: u64) -> Result<String, ApiError> {
    let vault = api.vault_get_main(telegram_user_id).await?;
    vault.id.ok_or(ApiError::Server {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "vault id missing".to_string(),
    })
}

fn is_allowed(cfg: &ConfigParameters, from: Option<&User>) -> bool {
    let Some(from) = from else {
        return false;
    };
    match &cfg.allowed_users {
        None => true,
        Some(ids) => ids.contains(&from.id),
    }
}

fn parse_command(text: &str) -> Option<Command> {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let mut parts = trimmed.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let arg = parts.next().map(|s| s.to_string());

    match cmd {
        "/start" => Some(Command::Start { code: arg }),
        "/home" => Some(Command::Home),
        "/help" => Some(Command::Help),
        _ => None,
    }
}

fn looks_like_quick_add(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with('r')
        || trimmed.starts_with('R')
        || trimmed.starts_with('+')
        || trimmed.starts_with('-')
        || trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
}

fn user_message_for_api_error(err: ApiError) -> String {
    match err {
        ApiError::Network(_) => {
            "Problemi di connessione con il server. Riprova più tardi!".to_string()
        }
        ApiError::Server { status, message } => match status {
            reqwest::StatusCode::UNAUTHORIZED => {
                "Non autorizzato. Usa /start per fare il pairing.".to_string()
            }
            reqwest::StatusCode::FORBIDDEN => "Operazione non permessa.".to_string(),
            reqwest::StatusCode::NOT_FOUND => {
                "Risorsa non trovata. Prova a reimpostare i default.".to_string()
            }
            reqwest::StatusCode::CONFLICT => "Richiesta duplicata (già salvata).".to_string(),
            reqwest::StatusCode::UNPROCESSABLE_ENTITY => message,
            _ => "Errore server.".to_string(),
        },
    }
}

fn now_rome() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&Rome).fixed_offset()
}

fn engine_currency(currency: api_types::Currency) -> EngineCurrency {
    match currency {
        api_types::Currency::Eur => EngineCurrency::Eur,
    }
}

fn welcome_text() -> &'static str {
    "Benvenuto!\n\nOra puoi inserire voci al volo scrivendo ad esempio:\n\n12.50 bar caffè\n+1000 stipendio\nr 5.20 amazon\n\nImposta i default (wallet/flow) usando i bottoni."
}

fn help_text() -> &'static str {
    "Esempi:\n\n12.50 bar caffè\n-12.50 bar caffè\n+1000 stipendio\nr 5.20 amazon\n\n#tag opzionale (max 1): 12.50 bar #food caffè"
}

fn wizard_prompt(kind: QuickKind) -> &'static str {
    match kind {
        QuickKind::Expense => {
            "Invia una uscita, es:\n\n12.50 bar caffè\n12.50 bar #food caffè\n\n(oppure scrivi direttamente nella chat senza usare il wizard)"
        }
        QuickKind::Income => {
            "Invia una entrata, es:\n\n1000 stipendio\n+1000 #salary stipendio\n\n(oppure scrivi direttamente nella chat senza usare il wizard)"
        }
        QuickKind::Refund => {
            "Invia un rimborso/storno, es:\n\nr 5.20 amazon\nr 5.20 #shopping amazon\n\n(oppure scrivi direttamente nella chat senza usare il wizard)"
        }
    }
}

fn normalize_wizard_input(kind: QuickKind, raw: &str) -> Result<String, &'static str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Testo vuoto.");
    }
    match kind {
        QuickKind::Expense => {
            if trimmed.starts_with('+') {
                return Err("Selezionato: uscita. Rimuovi il '+' (es: 12.50 bar).");
            }
            if trimmed.starts_with('r') || trimmed.starts_with('R') {
                return Err("Selezionato: uscita. Per refund usa il bottone “Refund”.");
            }
            Ok(trimmed.to_string())
        }
        QuickKind::Income => {
            if trimmed.starts_with('r') || trimmed.starts_with('R') {
                return Err("Selezionato: entrata. Rimuovi 'r' (es: 1000 stipendio).");
            }
            if trimmed.starts_with('+') {
                Ok(trimmed.to_string())
            } else {
                Ok(format!("+{trimmed}"))
            }
        }
        QuickKind::Refund => {
            if trimmed.starts_with('r') || trimmed.starts_with('R') {
                Ok(trimmed.to_string())
            } else {
                Ok(format!("r {trimmed}"))
            }
        }
    }
}

#[derive(Debug, Clone)]
enum Command {
    Start { code: Option<String> },
    Home,
    Help,
}
