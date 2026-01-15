use api_types::transaction::TransactionKind;
use teloxide::prelude::*;

use crate::{
    ConfigParameters,
    bot_client::BotClient,
    i18n::{self, TextKey},
    parsing::QuickKind,
    state::WizardSession,
    ui,
    use_cases::{home, shared},
};

pub(crate) async fn start_wizard(
    bot: &dyn BotClient,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    kind: QuickKind,
    locale: i18n::Locale,
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
    show_wizard(bot, chat_id, user_id, cfg, locale).await
}

pub(crate) async fn show_wizard(
    bot: &dyn BotClient,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let session = cfg.sessions.get(chat_id).await;
    let Some(wizard) = session.wizard else {
        return home::show_home(bot, chat_id, user_id, cfg, locale).await;
    };

    let snapshot = match cfg.api.vault_snapshot_main(user_id).await {
        Ok(s) => s,
        Err(err) => {
            let text = shared::user_message_for_api_error(locale, err);
            bot.send_message(chat_id, &text, None).await?;
            return Ok(());
        }
    };
    let currency = shared::engine_currency(snapshot.currency);

    let prefs =
        shared::ensure_flow_defaults(&cfg.prefs, user_id, snapshot.unallocated_flow_id).await;
    let Some(wallet_id) = prefs.default_wallet_id else {
        home::show_wallet_picker(bot, chat_id, user_id, cfg, locale).await?;
        return Ok(());
    };

    let kind_filter = match wizard.kind {
        QuickKind::Expense => TransactionKind::Expense,
        QuickKind::Income => TransactionKind::Income,
        QuickKind::Refund => TransactionKind::Refund,
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
            let text = shared::user_message_for_api_error(locale, err);
            bot.send_message(chat_id, &text, None).await?;
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
        return home::show_home(bot, chat_id, user_id, cfg, locale).await;
    };

    let (text, kb) = ui::wizard::render_wizard(
        locale,
        currency,
        &snapshot,
        &prefs,
        &wizard,
        &recents.transactions,
    );
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}

pub(crate) fn wizard_prompt(locale: i18n::Locale, kind: QuickKind) -> &'static str {
    match kind {
        QuickKind::Expense => i18n::t(locale, TextKey::WizardPromptExpense),
        QuickKind::Income => i18n::t(locale, TextKey::WizardPromptIncome),
        QuickKind::Refund => i18n::t(locale, TextKey::WizardPromptRefund),
    }
}

pub(crate) fn normalize_wizard_input(
    locale: i18n::Locale,
    kind: QuickKind,
    raw: &str,
) -> Result<String, &'static str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(i18n::t(locale, TextKey::WizardErrorEmpty));
    }
    match kind {
        QuickKind::Expense => {
            if trimmed.starts_with('+') {
                return Err(i18n::t(locale, TextKey::WizardErrorExpensePlus));
            }
            if trimmed.starts_with('r') || trimmed.starts_with('R') {
                return Err(i18n::t(locale, TextKey::WizardErrorExpenseRefund));
            }
            Ok(trimmed.to_string())
        }
        QuickKind::Income => {
            if trimmed.starts_with('r') || trimmed.starts_with('R') {
                return Err(i18n::t(locale, TextKey::WizardErrorIncomeRefund));
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
