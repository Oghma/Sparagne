//! Handler for user statistcs commands

use engine::{Currency, Money};
use reqwest::StatusCode;
use teloxide::{RequestError, dispatching::UpdateHandler, prelude::*};

use crate::{ConfigParameters, commands::UserStatisticsCommands, get_check};

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
    let user_id = match msg.from.as_ref() {
        Some(user) => user.id.to_string(),
        None => {
            bot.send_message(msg.chat.id, "Impossibile identificare l'utente.")
                .await?;
            return Ok(());
        }
    };

    match cmd {
        UserStatisticsCommands::Stats => {
            let (user_response, response) = get_check!(
                cfg.client,
                format!("{}/stats/get", cfg.server),
                user_id,
                &api_types::vault::Vault {
                    id: None,
                    name: Some("Main".to_string()),
                    currency: None,
                },
                "",
                "Problemi di connessione con il server. Riprova più tardi!"
            );

            let stats = match response {
                None => {
                    bot.send_message(msg.chat.id, user_response).await?;
                    return Ok(());
                }
                Some(response) => response.json::<api_types::stats::Statistic>().await?,
            };

            let currency = match stats.currency {
                api_types::Currency::Eur => Currency::Eur,
            };

            let response = format!(
                "Bilancio: {}\nTotale entrate: {}\nTotale uscite: {}",
                Money::new(stats.balance_minor).format(currency),
                Money::new(stats.total_income_minor).format(currency),
                Money::new(stats.total_expenses_minor).format(currency),
            );

            bot.send_message(msg.chat.id, response).await?;
        }
    };

    Ok(())
}
