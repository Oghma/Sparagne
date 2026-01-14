use engine::{Currency as EngineCurrency, Money};
use reqwest::StatusCode;
use teloxide::{
    prelude::*,
    types::{CallbackQuery, ChatId, InlineKeyboardButton, InlineKeyboardMarkup, User},
};
use uuid::Uuid;

use crate::{
    ConfigParameters,
    api::ApiError,
    i18n::{self, TextKey},
    parsing::{ParseError, QuickKind, parse_quick_add},
    routing::{CallbackAction, Command},
    state::{DraftCreate, PendingAction},
    text, ui,
    use_cases::{admin, home, list, shared, stats, wizard},
};

pub(crate) async fn handle_message(
    bot: Bot,
    msg: Message,
    cfg: ConfigParameters,
) -> ResponseResult<()> {
    if !is_allowed(&cfg, msg.from.as_ref()) {
        return Ok(());
    }

    let locale = i18n::default_locale();
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
                    bot.send_message(chat_id, text::welcome_text(&display_name))
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
                bot.send_message(chat_id, text::help_text()).await?;
                return Ok(());
            }
            Command::Categories => {
                admin::list_categories(&bot, chat_id, user_id, &cfg).await?;
                return Ok(());
            }
            Command::MembersList => {
                admin::list_vault_members(&bot, chat_id, user_id, &cfg).await?;
                return Ok(());
            }
            Command::MembersAdd { username, role } => {
                admin::add_vault_member(&bot, chat_id, user_id, &cfg, &username, role).await?;
                return Ok(());
            }
            Command::MembersRemove { username } => {
                admin::remove_vault_member(&bot, chat_id, user_id, &cfg, &username).await?;
                return Ok(());
            }
            Command::MembersHelp => {
                bot.send_message(chat_id, text::members_help_text()).await?;
                return Ok(());
            }
            Command::FlowMembersList { flow } => {
                admin::list_flow_members(&bot, chat_id, user_id, &cfg, &flow).await?;
                return Ok(());
            }
            Command::FlowMembersAdd {
                flow,
                username,
                role,
            } => {
                admin::add_flow_member(&bot, chat_id, user_id, &cfg, &flow, &username, role)
                    .await?;
                return Ok(());
            }
            Command::FlowMembersRemove { flow, username } => {
                admin::remove_flow_member(&bot, chat_id, user_id, &cfg, &flow, &username).await?;
                return Ok(());
            }
            Command::FlowMembersHelp => {
                bot.send_message(chat_id, text::flow_members_help_text())
                    .await?;
                return Ok(());
            }
            Command::MergeCategory {
                confirm,
                from,
                into,
            } => {
                admin::merge_category(&bot, chat_id, user_id, &cfg, confirm, &from, &into).await?;
                return Ok(());
            }
            Command::MergeCategoryHelp => {
                bot.send_message(chat_id, text::merge_category_help_text())
                    .await?;
                return Ok(());
            }
            Command::VaultDelete { confirm } => {
                admin::delete_vault(&bot, chat_id, user_id, &cfg, confirm).await?;
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

    let locale = i18n::default_locale();
    let Some(message) = q.message.as_ref() else {
        return Ok(());
    };
    let chat_id = message.chat().id;
    let user_id = q.from.id.0;
    cfg.sessions
        .update(chat_id, |s| {
            s.display_name = Some(text::display_name_from_telegram(&q.from))
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
            bot.send_message(chat_id, i18n::t(locale, TextKey::PairingPrompt))
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
                bot.send_message(chat_id, i18n::t(locale, TextKey::PreferencesSaveError))
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
                bot.send_message(chat_id, i18n::t(locale, TextKey::PreferencesSaveError))
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
                bot.send_message(chat_id, i18n::t(locale, TextKey::PreferencesSaveError))
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

            bot.send_message(chat_id, i18n::t(locale, TextKey::TransactionVoided))
                .await?;
            list::show_list(&bot, chat_id, user_id, &cfg).await?;
        }
        CallbackAction::TxEdit(tx_id) => {
            let (text, kb) = ui::detail::render_edit_menu(tx_id);
            shared::edit_or_send(&bot, chat_id, &cfg, text, kb).await?;
        }
        CallbackAction::TxEditAmount(tx_id) => {
            cfg.sessions
                .update(chat_id, |s| {
                    s.pending = Some(PendingAction::EditAmount { tx_id })
                })
                .await;
            bot.send_message(chat_id, i18n::t(locale, TextKey::EditAmountPrompt))
                .await?;
        }
        CallbackAction::TxEditNote(tx_id) => {
            cfg.sessions
                .update(chat_id, |s| {
                    s.pending = Some(PendingAction::EditNote { tx_id })
                })
                .await;
            bot.send_message(chat_id, i18n::t(locale, TextKey::EditNotePrompt))
                .await?;
        }
        CallbackAction::TxRepeat(tx_id) => {
            list::repeat_transaction(&bot, chat_id, user_id, &cfg, tx_id, q.id.0.as_str()).await?;
        }
        CallbackAction::Noop => {}
    }

    Ok(())
}

fn is_allowed(cfg: &ConfigParameters, user: Option<&User>) -> bool {
    let Some(allowed) = cfg.allowed_users.as_ref() else {
        return true;
    };
    let Some(user) = user else {
        return false;
    };
    allowed.contains(&user.id)
}

async fn handle_pending_message(
    bot: &Bot,
    msg: &Message,
    cfg: &ConfigParameters,
    user_id: u64,
    pending: PendingAction,
) -> ResponseResult<bool> {
    let locale = i18n::default_locale();
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
            bot.send_message(chat_id, text::welcome_text(&display_name))
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
                        InlineKeyboardButton::callback(
                            format!("\u{1f4cc} {}", i18n::t(locale, TextKey::DetailBtnRepeat)),
                            format!("tx:repeat:{id}", id = created.id),
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
            bot.send_message(chat_id, i18n::t(locale, TextKey::EditAmountUpdated))
                .await?;
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
            bot.send_message(chat_id, i18n::t(locale, TextKey::EditNoteUpdated))
                .await?;
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
    let locale = i18n::default_locale();
    let Some(text) = msg.text() else {
        return Ok(());
    };
    let parsed = match parse_quick_add(text, EngineCurrency::Eur) {
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
    let locale = i18n::default_locale();
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

            let kb = InlineKeyboardMarkup::new(vec![
                vec![
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
                ],
                vec![InlineKeyboardButton::callback(
                    format!("\u{1f4cc} {}", i18n::t(locale, TextKey::DetailBtnRepeat)),
                    format!("tx:repeat:{id}", id = created.id),
                )],
            ]);

            bot.send_message(chat_id, saved_msg)
                .reply_markup(kb)
                .await?;
        }
        Err(ApiError::Server { status, .. }) if status == StatusCode::CONFLICT => {
            bot.send_message(chat_id, i18n::t(locale, TextKey::AlreadySaved))
                .await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
        }
    }

    Ok(())
}
