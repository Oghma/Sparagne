use chrono::Datelike;
use teloxide::prelude::*;

use crate::{ConfigParameters, bot_client::BotClient, i18n, ui, use_cases::shared};

pub(crate) async fn show_stats(
    bot: &dyn BotClient,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    let stats = match cfg.api.stats_get_main(user_id).await {
        Ok(s) => s,
        Err(err) => {
            shared::send_api_error(bot, chat_id, locale, err).await?;
            return Ok(());
        }
    };

    let currency = shared::engine_currency(stats.currency);

    // Get current month name
    let now = chrono::Local::now();
    let month_name = month_name_localized(locale, now.month());
    let month_year = format!("{} {}", month_name, now.year());

    // Category breakdown - for now empty, would need API support
    // In future: fetch transactions and aggregate by category
    let category_breakdown: Vec<(String, i64)> = Vec::new();

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
