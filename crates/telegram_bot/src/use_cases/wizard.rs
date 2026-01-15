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
            s.wizard = Some(WizardSession { kind });
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
            shared::send_api_error(bot, chat_id, locale, err).await?;
            return Ok(());
        }
    };

    let prefs =
        shared::ensure_flow_defaults(&cfg.prefs, user_id, snapshot.unallocated_flow_id).await;

    // Ensure wallet is set
    if prefs.default_wallet_id.is_none() {
        home::show_wallet_picker(bot, chat_id, user_id, cfg, locale, "wiz:cancel").await?;
        return Ok(());
    }

    let (text, kb) = ui::wizard::render_wizard(locale, &snapshot, &prefs, &wizard);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}

pub(crate) fn wizard_prompt(locale: i18n::Locale, kind: QuickKind) -> &'static str {
    match kind {
        QuickKind::Expense => i18n::t(locale, TextKey::WizardPromptExpense),
        QuickKind::Income => i18n::t(locale, TextKey::WizardPromptIncome),
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
            // Remove + prefix if present (treat as expense anyway)
            let cleaned = trimmed.strip_prefix('+').unwrap_or(trimmed);
            Ok(cleaned.to_string())
        }
        QuickKind::Income => {
            // Ensure + prefix for income
            if trimmed.starts_with('+') {
                Ok(trimmed.to_string())
            } else {
                Ok(format!("+{trimmed}"))
            }
        }
    }
}
