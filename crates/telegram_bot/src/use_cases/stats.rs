use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use teloxide::prelude::*;

use crate::{
    ConfigParameters, bot_client::BotClient, i18n, state::ScreenContext, ui, use_cases::shared,
};

pub(crate) async fn show_stats(
    bot: &dyn BotClient,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let prefs = cfg.prefs.get_or_default(user_id).await;
    let vault_ref = shared::vault_ref_from_prefs(&prefs);
    let stats = match cfg.api.stats_get(user_id, &vault_ref).await {
        Ok(s) => s,
        Err(err) => {
            shared::send_api_error(bot, chat_id, locale, err).await?;
            return Ok(());
        }
    };

    let currency = shared::engine_currency(stats.currency);
    let vault_id = match shared::resolve_vault_id(&cfg.api, user_id, &vault_ref).await {
        Ok(id) => id,
        Err(err) => {
            shared::send_api_error(bot, chat_id, locale, err).await?;
            return Ok(());
        }
    };

    // Get current month name
    let now = shared::now_rome();
    let month_name = month_name_localized(locale, now.month());
    let month_year = format!("{} {}", month_name, now.year());

    let category_breakdown = match month_range(now.date_naive()) {
        Some((from, to)) => {
            match fetch_category_breakdown(cfg, user_id, &vault_id, from, to, locale).await {
                Ok(v) => v,
                Err(err) => {
                    shared::send_api_error(bot, chat_id, locale, err).await?;
                    return Ok(());
                }
            }
        }
        None => Vec::new(),
    };

    cfg.sessions
        .update(chat_id, |s| s.current_screen = ScreenContext::Stats)
        .await;

    let (text, kb) =
        ui::stats::render_stats(locale, currency, &stats, &month_year, &category_breakdown);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}

fn month_name_localized(locale: i18n::Locale, month: u32) -> &'static str {
    match locale {
        i18n::Locale::It => match month {
            1 => "Gennaio",
            2 => "Febbraio",
            3 => "Marzo",
            4 => "Aprile",
            5 => "Maggio",
            6 => "Giugno",
            7 => "Luglio",
            8 => "Agosto",
            9 => "Settembre",
            10 => "Ottobre",
            11 => "Novembre",
            12 => "Dicembre",
            _ => "?",
        },
        i18n::Locale::En => match month {
            1 => "January",
            2 => "February",
            3 => "March",
            4 => "April",
            5 => "May",
            6 => "June",
            7 => "July",
            8 => "August",
            9 => "September",
            10 => "October",
            11 => "November",
            12 => "December",
            _ => "?",
        },
    }
}

fn month_range(
    now: NaiveDate,
) -> Option<(
    chrono::DateTime<chrono::FixedOffset>,
    chrono::DateTime<chrono::FixedOffset>,
)> {
    let start = NaiveDate::from_ymd_opt(now.year(), now.month(), 1)?;
    let (next_year, next_month) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    let next_start = NaiveDate::from_ymd_opt(next_year, next_month, 1)?;
    let from = shared::rome_start_of_day(start)?;
    let to = shared::rome_start_of_day(next_start)?;
    Some((from, to))
}

async fn fetch_category_breakdown(
    cfg: &ConfigParameters,
    user_id: u64,
    vault_id: &str,
    from: chrono::DateTime<chrono::FixedOffset>,
    to: chrono::DateTime<chrono::FixedOffset>,
    locale: i18n::Locale,
) -> Result<Vec<(String, i64)>, crate::api::ApiError> {
    let mut cursor: Option<String> = None;
    let mut totals: HashMap<String, i64> = HashMap::new();

    loop {
        let list = cfg
            .api
            .transactions_list(
                user_id,
                &api_types::transaction::TransactionList {
                    vault_id: vault_id.to_string(),
                    flow_id: None,
                    wallet_id: None,
                    limit: Some(200),
                    cursor,
                    from: Some(from),
                    to: Some(to),
                    kinds: Some(vec![api_types::transaction::TransactionKind::Expense]),
                    include_voided: Some(false),
                    include_transfers: Some(false),
                },
            )
            .await?;

        for tx in list.transactions {
            let category = match tx.category.as_deref().filter(|c| !c.is_empty()) {
                Some(name) => name.to_string(),
                None => uncategorized_label(locale).to_string(),
            };
            let entry = totals.entry(category).or_insert(0);
            *entry += tx.amount_minor.saturating_abs();
        }

        match list.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }

    let mut breakdown: Vec<(String, i64)> = totals.into_iter().collect();
    breakdown.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    Ok(breakdown)
}

fn uncategorized_label(locale: i18n::Locale) -> &'static str {
    match locale {
        i18n::Locale::It => "Senza categoria",
        i18n::Locale::En => "Uncategorized",
    }
}
