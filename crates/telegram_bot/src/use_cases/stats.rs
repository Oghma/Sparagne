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
            let text = shared::user_message_for_api_error(locale, err);
            bot.send_message(chat_id, &text, None).await?;
            return Ok(());
        }
    };
    let currency = shared::engine_currency(stats.currency);
    let (text, kb) = ui::stats::render_stats(locale, currency, &stats);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}
