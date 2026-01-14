use teloxide::prelude::*;

use crate::{ConfigParameters, ui, use_cases::shared};

pub(crate) async fn show_stats(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
) -> ResponseResult<()> {
    let stats = match cfg.api.stats_get_main(user_id).await {
        Ok(s) => s,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };
    let currency = shared::engine_currency(stats.currency);
    let (text, kb) = ui::render_stats(currency, &stats);
    shared::edit_or_send(bot, chat_id, cfg, text, kb).await
}
