//! Handler for user statistcs commands

use reqwest::StatusCode;
use teloxide::{dispatching::UpdateHandler, prelude::*, RequestError};

use crate::{commands::UserStatisticsCommands, get_check, ConfigParameters};

/// Build the schema for Statistics commands
pub fn schema() -> UpdateHandler<RequestError> {
    Update::filter_message()
        .filter_command::<UserStatisticsCommands>()
        .endpoint(handle_statistics)
}

async fn handle_statistics(
    bot: Bot,
    cfg: ConfigParameters,
    msg: Message,
    cmd: UserStatisticsCommands,
) -> ResponseResult<()> {
    let user_id = &msg.from().map(|user| user.id.to_string()).unwrap();

    match cmd {
        UserStatisticsCommands::Stats => {
            let (user_response, response) = get_check!(
                cfg.client,
                format!("{}/stats", cfg.server),
                user_id,
                &server::types::vault::Vault {
                    id: None,
                    name: Some("Main".to_string())
                },
                "",
                "Problemi di connessione con il server. Riprova più tardi!"
            );

            let stats = match response {
                None => {
                    bot.send_message(msg.chat.id, user_response).await?;
                    return Ok(());
                }
                Some(response) => response.json::<server::types::stats::Statistic>().await?,
            };

            let response = format!(
                "Bilancio: {}€\nTotale entrate: {}€\nTotale uscite: {}€",
                stats.balance, stats.total_income, stats.total_expenses
            );

            bot.send_message(msg.chat.id, response).await?;
        }
    };

    Ok(())
}
