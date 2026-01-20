use teloxide::{prelude::*, types::CallbackQuery};
use uuid::Uuid;

use crate::{
    ConfigParameters,
    i18n::{self, TextKey},
    parsing::QuickKind,
    routing::CallbackAction,
    state::{MAX_TEMPLATES, PendingAction, Session},
    text, ui,
    use_cases::{home, list, shared, stats, wizard},
};

use super::{is_allowed, pending, templates};

pub(crate) async fn handle_callback(
    bot: Bot,
    q: CallbackQuery,
    cfg: ConfigParameters,
) -> ResponseResult<()> {
    if !is_allowed(&cfg, Some(&q.from)) {
        return Ok(());
    }

    let locale = i18n::resolve_locale(q.from.language_code.as_deref());
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

    if handle_nav_action(&bot, chat_id, user_id, &cfg, locale, &action).await? {
        return Ok(());
    }
    if handle_home_action(&bot, chat_id, user_id, &cfg, locale, &action).await? {
        return Ok(());
    }
    if handle_list_action(&bot, chat_id, user_id, &cfg, locale, &action).await? {
        return Ok(());
    }
    if handle_tx_action(&bot, chat_id, user_id, &cfg, locale, &action).await? {
        return Ok(());
    }
    if handle_wizard_action(&bot, chat_id, user_id, &cfg, locale, &action).await? {
        return Ok(());
    }
    if handle_template_action(&bot, chat_id, user_id, &cfg, locale, &action).await? {
        return Ok(());
    }
    if handle_vault_action(&bot, chat_id, user_id, &cfg, locale, &action).await? {
        return Ok(());
    }

    Ok(())
}

async fn handle_nav_action(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
    action: &CallbackAction,
) -> ResponseResult<bool> {
    match action {
        CallbackAction::NavHome => {
            cfg.sessions.update(chat_id, |s| s.wizard = None).await;
            home::show_home(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

async fn handle_home_action(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
    action: &CallbackAction,
) -> ResponseResult<bool> {
    match action {
        CallbackAction::StartExpense => {
            wizard::start_wizard(bot, chat_id, user_id, cfg, QuickKind::Expense, locale).await?;
            Ok(true)
        }
        CallbackAction::StartIncome => {
            wizard::start_wizard(bot, chat_id, user_id, cfg, QuickKind::Income, locale).await?;
            Ok(true)
        }
        CallbackAction::ShowHistory => {
            list::show_list(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
        CallbackAction::ShowStats => {
            stats::show_stats(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
        CallbackAction::ShowHelp => {
            let screen = cfg.sessions.get(chat_id).await.current_screen;
            bot.send_message(chat_id, text::contextual_help(locale, screen))
                .await?;
            Ok(true)
        }
        CallbackAction::PickWallet => {
            home::show_wallet_picker(bot, chat_id, user_id, cfg, locale, "nav:home").await?;
            Ok(true)
        }
        CallbackAction::PickFlow => {
            home::show_flow_picker(bot, chat_id, user_id, cfg, locale, "nav:home").await?;
            Ok(true)
        }
        CallbackAction::WalletSet(wallet_id) => {
            let updated = cfg
                .prefs
                .update(user_id, |p| p.default_wallet_id = Some(*wallet_id))
                .await;
            if updated.is_err() {
                bot.send_message(chat_id, i18n::t(locale, TextKey::PreferencesSaveError))
                    .await?;
            }

            let pending = cfg.sessions.get(chat_id).await.pending;
            if let Some(PendingAction::WalletForQuickAdd(draft)) = pending {
                cfg.sessions.update(chat_id, |s| s.pending = None).await;
                pending::finalize_quick_add(bot, chat_id, user_id, cfg, *wallet_id, draft, locale)
                    .await?;
                home::show_home(bot, chat_id, user_id, cfg, locale).await?;
                return Ok(true);
            }

            if cfg.sessions.get(chat_id).await.wizard.is_some() {
                wizard::show_wizard(bot, chat_id, user_id, cfg, locale).await?;
            } else {
                home::show_home(bot, chat_id, user_id, cfg, locale).await?;
            }
            Ok(true)
        }
        CallbackAction::FlowSet(flow_id) => {
            let updated = cfg
                .prefs
                .update(user_id, |p| {
                    p.default_flow_id = Some(*flow_id);
                    p.last_flow_id = Some(*flow_id);
                })
                .await;
            if updated.is_err() {
                bot.send_message(chat_id, i18n::t(locale, TextKey::PreferencesSaveError))
                    .await?;
            }
            if cfg.sessions.get(chat_id).await.wizard.is_some() {
                wizard::show_wizard(bot, chat_id, user_id, cfg, locale).await?;
            } else {
                home::show_home(bot, chat_id, user_id, cfg, locale).await?;
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

async fn handle_list_action(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
    action: &CallbackAction,
) -> ResponseResult<bool> {
    match action {
        CallbackAction::ListNext => {
            cfg.sessions.update(chat_id, apply_list_next).await;
            list::show_list(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
        CallbackAction::ListPrev => {
            cfg.sessions.update(chat_id, apply_list_prev).await;
            list::show_list(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
        CallbackAction::ToggleVoided => {
            let updated = cfg
                .prefs
                .update(user_id, |p| p.include_voided = !p.include_voided)
                .await;
            if updated.is_err() {
                bot.send_message(chat_id, i18n::t(locale, TextKey::PreferencesSaveError))
                    .await?;
                return Ok(true);
            }
            list::show_list(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
        CallbackAction::ListShowFilters => {
            list::show_filters(bot, chat_id, cfg, locale).await?;
            Ok(true)
        }
        CallbackAction::ListFilterKind(kind) => {
            list::set_filter_kind(bot, chat_id, user_id, cfg, *kind, locale).await?;
            Ok(true)
        }
        CallbackAction::ListFilterClear => {
            list::clear_filters(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
        CallbackAction::TxDetail(idx) => {
            list::show_detail_by_index(bot, chat_id, user_id, cfg, *idx, locale).await?;
            Ok(true)
        }
        CallbackAction::TxDetailById(tx_id) => {
            list::show_detail(bot, chat_id, user_id, cfg, *tx_id, locale).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

async fn handle_tx_action(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
    action: &CallbackAction,
) -> ResponseResult<bool> {
    match action {
        CallbackAction::TxVoidConfirm(tx_id) => {
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
            let detail = match cfg
                .api
                .transaction_get_detail(
                    user_id,
                    &api_types::transaction::TransactionGet {
                        vault_id,
                        id: *tx_id,
                    },
                )
                .await
            {
                Ok(d) => d,
                Err(err) => {
                    bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                        .await?;
                    return Ok(true);
                }
            };

            let currency = shared::engine_currency(detail.transaction.currency);
            let (text, kb) = ui::detail::render_void_confirm(locale, currency, &detail);
            shared::edit_or_send(bot, chat_id, cfg, text, kb).await?;
            Ok(true)
        }
        CallbackAction::TxVoid(tx_id) => {
            let prefs = cfg.prefs.get_or_default(user_id).await;
            let vault_ref = shared::vault_ref_from_prefs(&prefs);
            let vault_id = match shared::resolve_vault_id(&cfg.api, user_id, &vault_ref).await {
                Ok(vault_id) => vault_id,
                Err(err) => {
                    bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                        .await?;
                    return Ok(true);
                }
            };
            let voided = cfg
                .api
                .void_transaction(
                    user_id,
                    *tx_id,
                    &api_types::transaction::TransactionVoid {
                        vault_id,
                        voided_at: None,
                    },
                )
                .await;
            if let Err(err) = voided {
                bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                    .await?;
                return Ok(true);
            }

            bot.send_message(chat_id, i18n::t(locale, TextKey::TransactionVoided))
                .await?;
            list::show_list(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
        CallbackAction::TxRepeat(tx_id) => {
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

            let detail = match cfg
                .api
                .transaction_get_detail(
                    user_id,
                    &api_types::transaction::TransactionGet {
                        vault_id: vault_id.clone(),
                        id: *tx_id,
                    },
                )
                .await
            {
                Ok(d) => d,
                Err(err) => {
                    bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                        .await?;
                    return Ok(true);
                }
            };

            let tx = &detail.transaction;
            let occurred_at = shared::now_rome();

            use api_types::transaction::{LegTarget, TransactionKind};
            let mut wallet_id: Option<Uuid> = None;
            let mut flow_id: Option<Uuid> = None;
            for leg in &detail.legs {
                match &leg.target {
                    LegTarget::Wallet { wallet_id: wid } => wallet_id = Some(*wid),
                    LegTarget::Flow { flow_id: fid } => flow_id = Some(*fid),
                }
            }

            let result = match tx.kind {
                TransactionKind::Expense => {
                    cfg.api
                        .create_expense(
                            user_id,
                            &api_types::transaction::ExpenseNew {
                                vault_id,
                                amount_minor: tx.amount_minor,
                                flow_id,
                                wallet_id,
                                category_id: None,
                                category: tx.category.clone(),
                                note: tx.note.clone(),
                                idempotency_key: None,
                                occurred_at,
                            },
                        )
                        .await
                }
                TransactionKind::Income => {
                    cfg.api
                        .create_income(
                            user_id,
                            &api_types::transaction::IncomeNew {
                                vault_id,
                                amount_minor: tx.amount_minor,
                                flow_id,
                                wallet_id,
                                category_id: None,
                                category: tx.category.clone(),
                                note: tx.note.clone(),
                                idempotency_key: None,
                                occurred_at,
                            },
                        )
                        .await
                }
                TransactionKind::Refund
                | TransactionKind::TransferWallet
                | TransactionKind::TransferFlow => {
                    bot.send_message(chat_id, i18n::t(locale, TextKey::ApiForbidden))
                        .await?;
                    return Ok(true);
                }
            };

            match result {
                Ok(_) => {
                    bot.send_message(chat_id, i18n::t(locale, TextKey::RepeatSuccess))
                        .await?;
                    home::show_home(bot, chat_id, user_id, cfg, locale).await?;
                }
                Err(err) => {
                    bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                        .await?;
                }
            }
            Ok(true)
        }
        CallbackAction::TxEdit(tx_id) => {
            let (text, kb) = ui::detail::render_edit_menu(locale, *tx_id);
            shared::edit_or_send(bot, chat_id, cfg, text, kb).await?;
            Ok(true)
        }
        CallbackAction::TxEditAmount(tx_id) => {
            cfg.sessions
                .update(chat_id, |s| {
                    s.pending = Some(PendingAction::EditAmount { tx_id: *tx_id })
                })
                .await;
            bot.send_message(chat_id, i18n::t(locale, TextKey::EditAmountPrompt))
                .await?;
            Ok(true)
        }
        CallbackAction::TxEditNote(tx_id) => {
            cfg.sessions
                .update(chat_id, |s| {
                    s.pending = Some(PendingAction::EditNote { tx_id: *tx_id })
                })
                .await;
            bot.send_message(chat_id, i18n::t(locale, TextKey::EditNotePrompt))
                .await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

async fn handle_wizard_action(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
    action: &CallbackAction,
) -> ResponseResult<bool> {
    match action {
        CallbackAction::WizardInput => {
            let kind = cfg
                .sessions
                .get(chat_id)
                .await
                .wizard
                .as_ref()
                .map(|w| w.kind);
            let Some(kind) = kind else {
                home::show_home(bot, chat_id, user_id, cfg, locale).await?;
                return Ok(true);
            };

            cfg.sessions
                .update(chat_id, |s| {
                    s.pending = Some(PendingAction::WizardDraft { kind })
                })
                .await;
            bot.send_message(chat_id, wizard::wizard_prompt(locale, kind))
                .await?;
            Ok(true)
        }
        CallbackAction::WizardCancel => {
            cfg.sessions.update(chat_id, |s| s.wizard = None).await;
            home::show_home(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
        CallbackAction::WizardPickWallet => {
            home::show_wallet_picker(bot, chat_id, user_id, cfg, locale, "wiz:cancel").await?;
            Ok(true)
        }
        CallbackAction::WizardPickFlow => {
            home::show_flow_picker(bot, chat_id, user_id, cfg, locale, "wiz:cancel").await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

async fn handle_template_action(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
    action: &CallbackAction,
) -> ResponseResult<bool> {
    match action {
        CallbackAction::TemplateList => {
            templates::show_template_list(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
        CallbackAction::TemplateCreate => {
            let prefs = cfg.prefs.get_or_default(user_id).await;
            if prefs.templates.len() >= MAX_TEMPLATES {
                bot.send_message(chat_id, i18n::t(locale, TextKey::TemplateMaxReached))
                    .await?;
                return Ok(true);
            }
            cfg.sessions
                .update(chat_id, |s| s.pending = Some(PendingAction::TemplateCreate))
                .await;
            bot.send_message(chat_id, i18n::t(locale, TextKey::TemplateCreatePrompt))
                .await?;
            Ok(true)
        }
        CallbackAction::TemplateUse(idx) => {
            templates::use_template(bot, chat_id, user_id, cfg, *idx, locale).await?;
            Ok(true)
        }
        CallbackAction::TemplateDelete(idx) => {
            templates::delete_template(bot, chat_id, user_id, cfg, *idx, locale).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

async fn handle_vault_action(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
    action: &CallbackAction,
) -> ResponseResult<bool> {
    match action {
        CallbackAction::VaultSet(vault_id) => {
            let payload = api_types::vault::Vault {
                id: Some(vault_id.to_string()),
                name: None,
                currency: None,
                owner: None,
            };
            let vault = match cfg.api.vault_get(user_id, &payload).await {
                Ok(vault) => vault,
                Err(err) => {
                    bot.send_message(chat_id, shared::user_message_for_api_error(locale, err))
                        .await?;
                    return Ok(true);
                }
            };
            let vault_name = vault.name.clone().unwrap_or_else(|| vault_id.to_string());
            let updated = cfg
                .prefs
                .update(user_id, |p| {
                    p.active_vault_name = format!("id:{vault_id}");
                    p.default_wallet_id = None;
                    p.default_flow_id = None;
                    p.last_flow_id = None;
                })
                .await;
            if updated.is_err() {
                bot.send_message(chat_id, i18n::t(locale, TextKey::PreferencesSaveError))
                    .await?;
                return Ok(true);
            }
            let msg = i18n::format(
                locale,
                TextKey::VaultSetConfirmation,
                &[("vault", vault_name.as_str())],
            );
            bot.send_message(chat_id, msg).await?;
            home::show_home(bot, chat_id, user_id, cfg, locale).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn apply_list_next(session: &mut Session) {
    if let Some(list) = &mut session.list
        && list.next.is_some()
    {
        list.cursors.push(list.current.clone());
        list.current = list.next.clone();
    }
}

fn apply_list_prev(session: &mut Session) {
    if let Some(list) = &mut session.list {
        list.current = list.cursors.pop().unwrap_or(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ListFilters, ListSession};
    use uuid::Uuid;

    #[test]
    fn apply_list_next_advances_cursor() {
        let wallet_id = Uuid::new_v4();
        let mut session = Session {
            list: Some(ListSession {
                wallet_id,
                cursors: vec![None],
                current: Some("cur".to_string()),
                next: Some("next".to_string()),
                tx_ids: Vec::new(),
                filters: ListFilters::default(),
            }),
            ..Session::default()
        };

        apply_list_next(&mut session);
        let list = match session.list {
            Some(list) => list,
            None => panic!("missing list session"),
        };
        assert_eq!(list.current.as_deref(), Some("next"));
        assert_eq!(list.cursors.len(), 2);
        assert_eq!(list.cursors[1].as_deref(), Some("cur"));
    }

    #[test]
    fn apply_list_prev_pops_cursor() {
        let wallet_id = Uuid::new_v4();
        let mut session = Session {
            list: Some(ListSession {
                wallet_id,
                cursors: vec![Some("prev".to_string())],
                current: Some("cur".to_string()),
                next: Some("next".to_string()),
                tx_ids: Vec::new(),
                filters: ListFilters::default(),
            }),
            ..Session::default()
        };

        apply_list_prev(&mut session);
        let list = match session.list {
            Some(list) => list,
            None => panic!("missing list session"),
        };
        assert_eq!(list.current.as_deref(), Some("prev"));
        assert!(list.cursors.is_empty());
    }
}
