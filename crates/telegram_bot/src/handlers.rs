use api_types::{
    membership::{MemberUpsert, MemberView, MembershipRole},
    vault::FlowView,
};
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
    routing::{CallbackAction, Command},
    state::{DraftCreate, PendingAction},
    ui,
    use_cases::{home, list, shared, stats, wizard},
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
    cfg.sessions
        .update(chat_id, |s| {
            s.display_name = Some(display_name_from_telegram(from))
        })
        .await;

    // If we are waiting for an input (pair/edit), handle it first.
    if let Some(pending) = cfg.sessions.get(chat_id).await.pending
        && handle_pending_message(&bot, &msg, &cfg, user_id, pending).await?
    {
        return Ok(());
    }

    let Some(text) = msg.text() else {
        return Ok(());
    };

    if let Some(cmd) = crate::routing::parse_command(text) {
        match cmd {
            Command::Start { code } => {
                if let Some(code) = code.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
                    if let Err(err) = cfg.api.pair_user(user_id, code).await {
                        bot.send_message(chat_id, shared::user_message_for_api_error(err))
                            .await?;
                        return Ok(());
                    }

                    cfg.sessions.update(chat_id, |s| s.pending = None).await;
                    let display_name = cfg
                        .sessions
                        .get(chat_id)
                        .await
                        .display_name
                        .unwrap_or_else(|| "Sparagne".to_string());
                    bot.send_message(chat_id, welcome_text(&display_name))
                        .await?;
                    home::show_home(&bot, chat_id, user_id, &cfg).await?;
                    return Ok(());
                }

                home::show_home(&bot, chat_id, user_id, &cfg).await?;
                return Ok(());
            }
            Command::Home => {
                cfg.sessions.update(chat_id, |s| s.wizard = None).await;
                home::show_home(&bot, chat_id, user_id, &cfg).await?;
                return Ok(());
            }
            Command::Help => {
                bot.send_message(chat_id, help_text()).await?;
                return Ok(());
            }
            Command::Categories => {
                list_categories(&bot, chat_id, user_id, &cfg).await?;
                return Ok(());
            }
            Command::MembersList => {
                list_vault_members(&bot, chat_id, user_id, &cfg).await?;
                return Ok(());
            }
            Command::MembersAdd { username, role } => {
                add_vault_member(&bot, chat_id, user_id, &cfg, &username, role).await?;
                return Ok(());
            }
            Command::MembersRemove { username } => {
                remove_vault_member(&bot, chat_id, user_id, &cfg, &username).await?;
                return Ok(());
            }
            Command::MembersHelp => {
                bot.send_message(chat_id, members_help_text()).await?;
                return Ok(());
            }
            Command::FlowMembersList { flow } => {
                list_flow_members(&bot, chat_id, user_id, &cfg, &flow).await?;
                return Ok(());
            }
            Command::FlowMembersAdd {
                flow,
                username,
                role,
            } => {
                add_flow_member(&bot, chat_id, user_id, &cfg, &flow, &username, role).await?;
                return Ok(());
            }
            Command::FlowMembersRemove { flow, username } => {
                remove_flow_member(&bot, chat_id, user_id, &cfg, &flow, &username).await?;
                return Ok(());
            }
            Command::FlowMembersHelp => {
                bot.send_message(chat_id, flow_members_help_text()).await?;
                return Ok(());
            }
            Command::MergeCategory {
                confirm,
                from,
                into,
            } => {
                merge_category(&bot, chat_id, user_id, &cfg, confirm, &from, &into).await?;
                return Ok(());
            }
            Command::MergeCategoryHelp => {
                bot.send_message(chat_id, merge_category_help_text())
                    .await?;
                return Ok(());
            }
            Command::VaultDelete { confirm } => {
                delete_vault(&bot, chat_id, user_id, &cfg, confirm).await?;
                return Ok(());
            }
        }
    }

    if crate::routing::looks_like_quick_add(text) {
        handle_quick_add(&bot, &msg, &cfg, user_id).await?;
    }

    Ok(())
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
    cfg.sessions
        .update(chat_id, |s| {
            s.display_name = Some(display_name_from_telegram(&q.from))
        })
        .await;

    let _ = bot.answer_callback_query(q.id.clone()).await;

    let Some(data) = q.data.as_deref() else {
        return Ok(());
    };

    let Some(action) = crate::routing::parse_callback_action(data) else {
        return Ok(());
    };

    match action {
        CallbackAction::NavHome => {
            cfg.sessions.update(chat_id, |s| s.wizard = None).await;
            home::show_home(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::NavWizard => {
            wizard::show_wizard(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::ShowList => {
            list::show_list(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::HomePair => {
            cfg.sessions
                .update(chat_id, |s| s.pending = Some(PendingAction::PairCode))
                .await;
            bot.send_message(chat_id, "Inserisci il codice di pairing:")
                .await?;
        }
        CallbackAction::HomePickWallet => {
            home::show_wallet_picker(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::HomePickFlow => {
            home::show_flow_picker(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::HomeExpense => {
            wizard::start_wizard(&bot, chat_id, user_id, &cfg, QuickKind::Expense).await?;
        }
        CallbackAction::HomeIncome => {
            wizard::start_wizard(&bot, chat_id, user_id, &cfg, QuickKind::Income).await?;
        }
        CallbackAction::HomeRefund => {
            wizard::start_wizard(&bot, chat_id, user_id, &cfg, QuickKind::Refund).await?;
        }
        CallbackAction::HomeStats => {
            stats::show_stats(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::WizClose => {
            cfg.sessions.update(chat_id, |s| s.wizard = None).await;
            home::show_home(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::WizPickWallet => {
            home::show_wallet_picker(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::WizPickFlow => {
            home::show_flow_picker(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::WizInput => {
            let kind = cfg
                .sessions
                .get(chat_id)
                .await
                .wizard
                .as_ref()
                .map(|w| w.kind);
            let Some(kind) = kind else {
                home::show_home(&bot, chat_id, user_id, &cfg).await?;
                return Ok(());
            };

            cfg.sessions
                .update(chat_id, |s| {
                    s.pending = Some(PendingAction::WizardDraft { kind })
                })
                .await;
            bot.send_message(chat_id, wizard::wizard_prompt(kind))
                .await?;
        }
        CallbackAction::WizCatNone | CallbackAction::WizCatReset => {
            cfg.sessions
                .update(chat_id, |s| {
                    if let Some(w) = &mut s.wizard {
                        w.category = None;
                    }
                })
                .await;
            wizard::show_wizard(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::WizCatIndex(idx) => {
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
            wizard::show_wizard(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::WizRecent(tx_id) => {
            list::repeat_transaction(&bot, chat_id, user_id, &cfg, tx_id, q.id.0.as_str()).await?;
            wizard::show_wizard(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::PrefsToggleVoided => {
            let updated = cfg
                .prefs
                .update(user_id, |p| p.include_voided = !p.include_voided)
                .await;
            if updated.is_err() {
                bot.send_message(chat_id, "Errore nel salvataggio delle preferenze.")
                    .await?;
            }
            list::show_list(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::ListNext => {
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
            list::show_list(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::ListPrev => {
            cfg.sessions
                .update(chat_id, |s| {
                    if let Some(list) = &mut s.list {
                        list.current = list.cursors.pop().unwrap_or(None);
                    }
                })
                .await;
            list::show_list(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::WalletSet(wallet_id) => {
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
                home::show_home(&bot, chat_id, user_id, &cfg).await?;
                return Ok(());
            }

            if cfg.sessions.get(chat_id).await.wizard.is_some() {
                wizard::show_wizard(&bot, chat_id, user_id, &cfg).await?;
            } else {
                home::show_home(&bot, chat_id, user_id, &cfg).await?;
            }
        }
        CallbackAction::FlowSet(flow_id) => {
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
                wizard::show_wizard(&bot, chat_id, user_id, &cfg).await?;
            } else {
                home::show_home(&bot, chat_id, user_id, &cfg).await?;
            }
        }
        CallbackAction::TxDetail(tx_id) => {
            list::show_detail(&bot, chat_id, user_id, &cfg, tx_id).await?;
        }
        CallbackAction::TxVoid(tx_id) => {
            let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
                Ok(vault_id) => vault_id,
                Err(err) => {
                    bot.send_message(chat_id, shared::user_message_for_api_error(err))
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
                bot.send_message(chat_id, shared::user_message_for_api_error(err))
                    .await?;
                return Ok(());
            }

            bot.send_message(chat_id, "✅ Voce annullata (void).")
                .await?;
            list::show_list(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::TxEdit(tx_id) => {
            let (text, kb) = ui::render_edit_menu(tx_id);
            shared::edit_or_send(&bot, chat_id, &cfg, text, kb).await?;
        }
        CallbackAction::TxEditAmount(tx_id) => {
            cfg.sessions
                .update(chat_id, |s| {
                    s.pending = Some(PendingAction::EditAmount { tx_id })
                })
                .await;
            bot.send_message(chat_id, "Invia il nuovo importo (es: 10.50):")
                .await?;
        }
        CallbackAction::TxEditNote(tx_id) => {
            cfg.sessions
                .update(chat_id, |s| {
                    s.pending = Some(PendingAction::EditNote { tx_id })
                })
                .await;
            bot.send_message(chat_id, "Invia la nuova nota (vuoto per rimuovere):")
                .await?;
        }
        CallbackAction::TxRepeat(tx_id) => {
            list::repeat_transaction(&bot, chat_id, user_id, &cfg, tx_id, q.id.0.as_str()).await?;
        }
        CallbackAction::Noop => {}
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
                bot.send_message(chat_id, shared::user_message_for_api_error(err))
                    .await?;
                return Ok(true);
            }

            cfg.sessions.update(chat_id, |s| s.pending = None).await;
            let display_name = cfg
                .sessions
                .get(chat_id)
                .await
                .display_name
                .unwrap_or_else(|| "Sparagne".to_string());
            bot.send_message(chat_id, welcome_text(&display_name))
                .await?;
            home::show_home(bot, chat_id, user_id, cfg).await?;
            Ok(true)
        }
        PendingAction::WizardDraft { kind } => {
            let Some(text) = msg.text() else {
                return Ok(true);
            };

            let input = match wizard::normalize_wizard_input(kind, text) {
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
                home::show_wallet_picker(bot, chat_id, user_id, cfg).await?;
                return Ok(true);
            };

            let snapshot = match cfg.api.vault_snapshot_main(user_id).await {
                Ok(s) => s,
                Err(err) => {
                    bot.send_message(chat_id, shared::user_message_for_api_error(err))
                        .await?;
                    return Ok(true);
                }
            };

            let flow_id = prefs.last_flow_id.or(Some(snapshot.unallocated_flow_id));
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
                QuickKind::Refund => {
                    cfg.api
                        .create_refund(
                            user_id,
                            &api_types::transaction::Refund {
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
                    bot.send_message(chat_id, shared::user_message_for_api_error(err))
                        .await?;
                }
            }

            wizard::show_wizard(bot, chat_id, user_id, cfg).await?;
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

            let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
                Ok(v) => v,
                Err(err) => {
                    bot.send_message(chat_id, shared::user_message_for_api_error(err))
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
                bot.send_message(chat_id, shared::user_message_for_api_error(err))
                    .await?;
                return Ok(true);
            }

            cfg.sessions.update(chat_id, |s| s.pending = None).await;
            bot.send_message(chat_id, "✅ Importo aggiornato.").await?;
            home::show_home(bot, chat_id, user_id, cfg).await?;
            Ok(true)
        }
        PendingAction::EditNote { tx_id } => {
            let note = msg
                .text()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty());

            let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
                Ok(v) => v,
                Err(err) => {
                    bot.send_message(chat_id, shared::user_message_for_api_error(err))
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
                bot.send_message(chat_id, shared::user_message_for_api_error(err))
                    .await?;
                return Ok(true);
            }

            cfg.sessions.update(chat_id, |s| s.pending = None).await;
            bot.send_message(chat_id, "✅ Nota aggiornata.").await?;
            home::show_home(bot, chat_id, user_id, cfg).await?;
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
        home::show_wallet_picker(bot, msg.chat.id, user_id, cfg).await?;
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
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let currency = shared::engine_currency(snapshot.currency);
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
        QuickKind::Refund => {
            cfg.api
                .create_refund(
                    user_id,
                    &api_types::transaction::Refund {
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

            let kb = InlineKeyboardMarkup::new(vec![
                vec![
                    InlineKeyboardButton::callback(
                        "↩ Undo",
                        format!("tx:void:{id}", id = created.id),
                    ),
                    InlineKeyboardButton::callback(
                        "✏️ Edit",
                        format!("tx:edit:{id}", id = created.id),
                    ),
                ],
                vec![InlineKeyboardButton::callback(
                    "📌 Ripeti",
                    format!("tx:repeat:{id}", id = created.id),
                )],
            ]);

            bot.send_message(chat_id, saved_msg)
                .reply_markup(kb)
                .await?;
        }
        Err(ApiError::Server { status, .. }) if status == StatusCode::CONFLICT => {
            bot.send_message(chat_id, "✅ Già salvato.").await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
        }
    }

    Ok(())
}

async fn resolve_accessible_flows(
    api: &ApiClient,
    telegram_user_id: u64,
) -> Result<Vec<FlowView>, ApiError> {
    match api.vault_snapshot_main(telegram_user_id).await {
        Ok(snapshot) => Ok(snapshot.flows),
        Err(ApiError::Server { status, .. }) if status == StatusCode::NOT_FOUND => {
            let response = api.flows_shared_main(telegram_user_id).await?;
            Ok(response.flows)
        }
        Err(err) => Err(err),
    }
}

async fn list_categories(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
) -> ResponseResult<()> {
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let response = match cfg
        .api
        .categories_list(
            user_id,
            &api_types::category::CategoryList {
                vault_id,
                include_archived: Some(true),
            },
        )
        .await
    {
        Ok(resp) => resp,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let text = render_category_list(&response.categories);
    bot.send_message(chat_id, text).await?;
    Ok(())
}

async fn list_vault_members(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
) -> ResponseResult<()> {
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let response = match cfg.api.vault_members_list(user_id, &vault_id).await {
        Ok(resp) => resp,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let text = render_members_list("Membri vault", &response.members);
    bot.send_message(chat_id, text).await?;
    Ok(())
}

async fn add_vault_member(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    username: &str,
    role: MembershipRole,
) -> ResponseResult<()> {
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let payload = MemberUpsert {
        username: username.to_string(),
        role,
    };
    match cfg
        .api
        .vault_member_upsert(user_id, &vault_id, &payload)
        .await
    {
        Ok(()) => {
            bot.send_message(
                chat_id,
                format!("✅ Membro salvato: {username} ({})", role_label(role)),
            )
            .await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
        }
    }
    Ok(())
}

async fn remove_vault_member(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    username: &str,
) -> ResponseResult<()> {
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    match cfg
        .api
        .vault_member_remove(user_id, &vault_id, username)
        .await
    {
        Ok(()) => {
            bot.send_message(chat_id, format!("✅ Membro rimosso: {username}"))
                .await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
        }
    }
    Ok(())
}

async fn delete_vault(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    confirm: bool,
) -> ResponseResult<()> {
    if !confirm {
        bot.send_message(chat_id, vault_delete_help_text()).await?;
        return Ok(());
    }

    match cfg.api.vault_delete_main(user_id).await {
        Ok(()) => {
            bot.send_message(chat_id, "✅ Vault eliminato.").await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
        }
    }
    Ok(())
}

async fn list_flow_members(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    flow_name: &str,
) -> ResponseResult<()> {
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let flows = match resolve_accessible_flows(&cfg.api, user_id).await {
        Ok(resp) => resp,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };
    let Some(flow) = find_flow_by_name(&flows, flow_name) else {
        bot.send_message(chat_id, flow_not_found_text(flow_name, &flows))
            .await?;
        return Ok(());
    };

    let response = match cfg.api.flow_members_list(user_id, &vault_id, flow.id).await {
        Ok(resp) => resp,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let text = render_members_list(&format!("Membri flow \"{}\"", flow.name), &response.members);
    bot.send_message(chat_id, text).await?;
    Ok(())
}

async fn add_flow_member(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    flow_name: &str,
    username: &str,
    role: MembershipRole,
) -> ResponseResult<()> {
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let flows = match resolve_accessible_flows(&cfg.api, user_id).await {
        Ok(resp) => resp,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };
    let Some(flow) = find_flow_by_name(&flows, flow_name) else {
        bot.send_message(chat_id, flow_not_found_text(flow_name, &flows))
            .await?;
        return Ok(());
    };

    let payload = MemberUpsert {
        username: username.to_string(),
        role,
    };
    match cfg
        .api
        .flow_member_upsert(user_id, &vault_id, flow.id, &payload)
        .await
    {
        Ok(()) => {
            bot.send_message(
                chat_id,
                format!("✅ Membro salvato: {username} ({})", role_label(role)),
            )
            .await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
        }
    }
    Ok(())
}

async fn remove_flow_member(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    flow_name: &str,
    username: &str,
) -> ResponseResult<()> {
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let flows = match resolve_accessible_flows(&cfg.api, user_id).await {
        Ok(resp) => resp,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };
    let Some(flow) = find_flow_by_name(&flows, flow_name) else {
        bot.send_message(chat_id, flow_not_found_text(flow_name, &flows))
            .await?;
        return Ok(());
    };

    match cfg
        .api
        .flow_member_remove(user_id, &vault_id, flow.id, username)
        .await
    {
        Ok(()) => {
            bot.send_message(chat_id, format!("✅ Membro rimosso: {username}"))
                .await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
        }
    }
    Ok(())
}

async fn merge_category(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    confirm: bool,
    from: &str,
    into: &str,
) -> ResponseResult<()> {
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let categories = match cfg
        .api
        .categories_list(
            user_id,
            &api_types::category::CategoryList {
                vault_id: vault_id.clone(),
                include_archived: Some(true),
            },
        )
        .await
    {
        Ok(resp) => resp.categories,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let Some(from_category) = match_category_by_input(&categories, from) else {
        bot.send_message(chat_id, format!("Categoria sorgente non trovata: {from}"))
            .await?;
        bot.send_message(chat_id, "Usa /categories per vedere la lista.")
            .await?;
        return Ok(());
    };
    let Some(into_category) = match_category_by_input(&categories, into) else {
        bot.send_message(
            chat_id,
            format!("Categoria destinazione non trovata: {into}"),
        )
        .await?;
        bot.send_message(chat_id, "Usa /categories per vedere la lista.")
            .await?;
        return Ok(());
    };

    let preview = match cfg
        .api
        .categories_merge_preview(
            user_id,
            from_category.id,
            &api_types::category::CategoryMergePreview {
                vault_id: vault_id.clone(),
                into_category_id: into_category.id,
            },
        )
        .await
    {
        Ok(resp) => resp,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    if !preview.ok {
        let text = render_merge_conflicts(from_category, into_category, &preview);
        bot.send_message(chat_id, text).await?;
        return Ok(());
    }

    if !confirm {
        let text = format!(
            "Ok, posso unire \"{}\" -> \"{}\".\nConferma con:\n/merge_category confirm {} -> {}",
            from_category.name, into_category.name, from_category.name, into_category.name
        );
        bot.send_message(chat_id, text).await?;
        return Ok(());
    }

    let merged = cfg
        .api
        .categories_merge(
            user_id,
            from_category.id,
            &api_types::category::CategoryMerge {
                vault_id,
                into_category_id: into_category.id,
            },
        )
        .await;
    match merged {
        Ok(_) => {
            bot.send_message(
                chat_id,
                format!(
                    "Unione completata: \"{}\" -> \"{}\".",
                    from_category.name, into_category.name
                ),
            )
            .await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
        }
    }
    Ok(())
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

fn render_category_list(categories: &[api_types::category::CategoryView]) -> String {
    if categories.is_empty() {
        return "Nessuna categoria. Aggiungi una transazione con #categoria per iniziare."
            .to_string();
    }

    let mut lines = Vec::with_capacity(categories.len() + 2);
    lines.push("Categorie:".to_string());
    for category in categories {
        let mut line = format!("- {}", category.name);
        if category.is_system {
            line.push_str(" [system]");
        }
        if category.archived {
            line.push_str(" [archived]");
        }
        lines.push(line);
    }
    lines.join("\n")
}

fn render_members_list(title: &str, members: &[MemberView]) -> String {
    if members.is_empty() {
        return format!("{title}:\n- Nessun membro.");
    }
    let mut lines = Vec::with_capacity(members.len() + 1);
    lines.push(format!("{title}:"));
    for member in members {
        lines.push(format!(
            "- {} ({})",
            member.username,
            role_label(member.role)
        ));
    }
    lines.join("\n")
}

fn role_label(role: MembershipRole) -> &'static str {
    match role {
        MembershipRole::Owner => "owner",
        MembershipRole::Editor => "editor",
        MembershipRole::Viewer => "viewer",
    }
}

fn normalize_flow_label(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn find_flow_by_name<'a>(flows: &'a [FlowView], name: &str) -> Option<&'a FlowView> {
    let needle = normalize_flow_label(name);
    flows
        .iter()
        .filter(|flow| !flow.archived && !flow.is_unallocated)
        .find(|flow| normalize_flow_label(&flow.name) == needle)
}

fn flow_not_found_text(name: &str, flows: &[FlowView]) -> String {
    let flows = flows
        .iter()
        .filter(|flow| !flow.archived && !flow.is_unallocated)
        .map(|flow| flow.name.as_str())
        .collect::<Vec<_>>();
    if flows.is_empty() {
        return format!("Flow \"{name}\" non trovato. Nessun flow condivisibile.");
    }
    let mut lines = Vec::with_capacity(flows.len() + 2);
    lines.push(format!("Flow \"{name}\" non trovato."));
    lines.push("Flow disponibili:".to_string());
    for flow in flows {
        lines.push(format!("- {flow}"));
    }
    lines.join("\n")
}

fn render_merge_conflicts(
    from: &api_types::category::CategoryView,
    into: &api_types::category::CategoryView,
    preview: &api_types::category::CategoryMergePreviewResponse,
) -> String {
    let mut lines = Vec::with_capacity(preview.conflicts.len() + 2);
    lines.push(format!(
        "Merge non possibile: \"{}\" -> \"{}\".",
        from.name, into.name
    ));
    lines.push("Conflitti:".to_string());
    for conflict in &preview.conflicts {
        lines.push(format!("- {}", merge_conflict_label(conflict)));
    }
    lines.join("\n")
}

fn merge_conflict_label(conflict: &api_types::category::CategoryMergeConflict) -> String {
    match conflict.kind.as_str() {
        "same_category" => "Le categorie sono identiche.".to_string(),
        "source_system" => format!("La categoria \"{}\" e' di sistema.", conflict.value),
        "target_archived" => format!("La categoria \"{}\" e' archiviata.", conflict.value),
        "alias_conflict" => format!("Alias in conflitto: {}", conflict.value),
        "name_conflict" => format!("Nome in conflitto: {}", conflict.value),
        _ => format!("Conflitto: {} ({})", conflict.kind, conflict.value),
    }
}

fn match_category_by_input<'a>(
    categories: &'a [api_types::category::CategoryView],
    input: &str,
) -> Option<&'a api_types::category::CategoryView> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(id) = Uuid::parse_str(trimmed) {
        return categories.iter().find(|category| category.id == id);
    }

    let needle = normalize_category_label(trimmed);
    categories
        .iter()
        .find(|category| normalize_category_label(&category.name) == needle)
}

fn normalize_category_label(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn welcome_text(display_name: &str) -> String {
    format!(
        "Benvenuto, {display_name}!\n\nOra puoi inserire voci al volo scrivendo ad esempio:\n\n12.50 bar caffè\n+1000 stipendio\nr 5.20 amazon\n\nImposta i default (wallet/flow) usando i bottoni."
    )
}

fn help_text() -> &'static str {
    "Esempi:\n\n12.50 bar caffè\n-12.50 bar caffè\n+1000 stipendio\nr 5.20 amazon\n\n#tag opzionale (max 1): 12.50 bar #food caffè\n\nComandi:\n/home\n/categories\n/merge_category <da> -> <a>\n/merge_category confirm <da> -> <a>\n/members\n/members add <username> <owner|editor|viewer>\n/members remove <username>\n/flow_members <flow>\n/flow_members add <flow> <username> <owner|editor|viewer>\n/flow_members remove <flow> <username>\n/vault_delete\n/vault_delete confirm"
}

fn merge_category_help_text() -> &'static str {
    "Uso:\n/merge_category <da> -> <a>\n/merge_category confirm <da> -> <a>"
}

fn members_help_text() -> &'static str {
    "Uso:\n/members\n/members add <username> <owner|editor|viewer>\n/members remove <username>"
}

fn flow_members_help_text() -> &'static str {
    "Uso:\n/flow_members <flow>\n/flow_members add <flow> <username> <owner|editor|viewer>\n/flow_members remove <flow> <username>\n\nNota: il flow può contenere spazi."
}

fn vault_delete_help_text() -> &'static str {
    "Uso:\n/vault_delete confirm\n\nAttenzione: elimina il vault Main e tutti i dati."
}

fn display_name_from_telegram(user: &User) -> String {
    if let Some(username) = user.username.as_deref().filter(|u| !u.is_empty()) {
        format!("@{username}")
    } else if !user.first_name.is_empty() {
        user.first_name.clone()
    } else {
        "Sparagne".to_string()
    }
}
