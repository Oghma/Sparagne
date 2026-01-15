use reqwest::StatusCode;
use teloxide::prelude::*;
use uuid::Uuid;

use crate::{
    ConfigParameters,
    api::ApiError,
    bot_client::BotClient,
    i18n::{self, TextKey},
    state::{ListSession, PendingAction},
    ui,
    use_cases::{home, shared},
};

pub(crate) async fn show_list(
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
    let currency = shared::engine_currency(snapshot.currency);
    let prefs = cfg.prefs.get_or_default(user_id).await;
    let Some(wallet_id) = prefs.default_wallet_id else {
        bot.send_message(
            chat_id,
            i18n::t(locale, TextKey::DefaultWalletMissing),
            None,
        )
        .await?;
        home::show_wallet_picker(bot, chat_id, user_id, cfg, locale).await?;
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
            shared::send_api_error(bot, chat_id, locale, err).await?;
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
            s.list = Some(ListSession {
                wallet_id,
                cursors,
                current,
                next: list.next_cursor.clone(),
            });
        })
        .await;

    let (text, kb) = ui::list::render_list(
        locale,
        currency,
        &list,
        prefs.include_voided,
        has_prev,
        has_next,
    );
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}

pub(crate) async fn show_detail(
    bot: &dyn BotClient,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    tx_id: Uuid,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let Some((_vault_id, detail)) =
        fetch_detail_with_vault(bot, chat_id, user_id, cfg, tx_id, locale).await?
    else {
        return Ok(());
    };
    cfg.sessions
        .update(chat_id, |s| s.last_detail_tx = Some(tx_id))
        .await;
    let currency = shared::engine_currency(detail.transaction.currency);
    let (text, kb) = ui::detail::render_detail(locale, currency, &detail);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}

pub(crate) async fn repeat_transaction(
    bot: &dyn BotClient,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    tx_id: Uuid,
    callback_id: &str,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let Some((vault_id, detail)) =
        fetch_detail_with_vault(bot, chat_id, user_id, cfg, tx_id, locale).await?
    else {
        return Ok(());
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
        bot.send_message(chat_id, i18n::t(locale, TextKey::RepeatNoWallet), None)
            .await?;
        return Ok(());
    };

    let occurred_at = shared::now_rome();
    let idempotency_key = format!("tgcb:{}:{callback_id}", chat_id.0);
    let amount_minor = detail.transaction.amount_minor;
    let category_id = Some(detail.transaction.category_id);
    let category = detail.transaction.category.clone();
    let note = detail.transaction.note.clone();

    let created = match detail.transaction.kind {
        api_types::transaction::TransactionKind::Income => {
            cfg.api
                .create_income(
                    user_id,
                    &api_types::transaction::IncomeNew {
                        vault_id: vault_id.clone(),
                        amount_minor,
                        flow_id,
                        wallet_id: Some(wallet_id),
                        category_id,
                        category,
                        note,
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
                        vault_id: vault_id.clone(),
                        amount_minor,
                        flow_id,
                        wallet_id: Some(wallet_id),
                        category_id,
                        category,
                        note,
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
                        vault_id: vault_id.clone(),
                        amount_minor,
                        flow_id,
                        wallet_id: Some(wallet_id),
                        category_id,
                        category,
                        note,
                        idempotency_key: Some(idempotency_key),
                        occurred_at,
                    },
                )
                .await
        }
        _ => {
            bot.send_message(chat_id, i18n::t(locale, TextKey::RepeatUnsupported), None)
                .await?;
            return Ok(());
        }
    };

    match created {
        Ok(_) => {
            bot.send_message(chat_id, i18n::t(locale, TextKey::RepeatSuccess), None)
                .await?;
        }
        Err(ApiError::Server { status, .. }) if status == StatusCode::CONFLICT => {
            bot.send_message(chat_id, i18n::t(locale, TextKey::AlreadySaved), None)
                .await?;
        }
        Err(err) => {
            shared::send_api_error(bot, chat_id, locale, err).await?;
        }
    };

    Ok(())
}

async fn fetch_detail_with_vault(
    bot: &dyn BotClient,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    tx_id: Uuid,
    locale: i18n::Locale,
) -> ResponseResult<Option<(String, api_types::transaction::TransactionDetailResponse)>> {
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(vault_id) => vault_id,
        Err(err) => {
            shared::send_api_error(bot, chat_id, locale, err).await?;
            return Ok(None);
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
        Ok(detail) => detail,
        Err(err) => {
            shared::send_api_error(bot, chat_id, locale, err).await?;
            return Ok(None);
        }
    };

    Ok(Some((vault_id, detail)))
}
