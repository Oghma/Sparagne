//! Handler for managing user settings

use reqwest::StatusCode;
use teloxide::{RequestError, dispatching::UpdateHandler, prelude::*};

use crate::{ConfigParameters, commands::HandleUserAccount};

use super::entry::send_help_message;

/// Build the schema for `HandleUserAccount` commands
pub fn schema() -> UpdateHandler<RequestError> {
    Update::filter_message()
        .filter_command::<HandleUserAccount>()
        .endpoint(handle_pair_user)
}

async fn handle_pair_user(
    bot: Bot,
    cfg: ConfigParameters,
    msg: Message,
    cmd: HandleUserAccount,
) -> ResponseResult<()> {
    let telegram_id = match msg.from.as_ref() {
        Some(user) => user.id.to_string(),
        None => {
            bot.send_message(msg.chat.id, "Impossibile identificare l'utente.")
                .await?;
            return Ok(());
        }
    };

    match cmd {
        HandleUserAccount::Pair { code } => {
            let response = match cfg
                .client
                .post(cfg.server.to_string() + "/user/pair")
                .json(&api_types::user::PairUser { code, telegram_id })
                .send()
                .await
            {
                Ok(response) => response,
                Err(err) => {
                    tracing::debug!("request failed: {err}");
                    bot.send_message(
                        msg.chat.id,
                        "Connection problems with the server. Retry later!",
                    )
                    .await?;
                    return Ok(());
                }
            };

            match response.status() {
                StatusCode::CREATED => {
                    bot.send_message(msg.chat.id, "Account paired").await?;
                    send_help_message(bot, msg).await?;
                }
                _ => {
                    tracing::debug!("{:?}", response);
                    match response.text().await {
                        Ok(body) => tracing::debug!("body: {body}"),
                        Err(err) => tracing::debug!("body read failed: {err}"),
                    }
                    bot.send_message(
                        msg.chat.id,
                        "Connection problems with the server. Retry later!",
                    )
                    .await?;
                }
            };
        }
        HandleUserAccount::UnPair => {
            let response = match cfg
                .client
                .delete(cfg.server + "/user/pair")
                .header("telegram-user-id", telegram_id)
                .send()
                .await
            {
                Ok(response) => response,
                Err(err) => {
                    tracing::debug!("request failed: {err}");
                    bot.send_message(
                        msg.chat.id,
                        "Connection problems with the server. Retry later!",
                    )
                    .await?;
                    return Ok(());
                }
            };

            let user_response = match response.status() {
                StatusCode::ACCEPTED => "Account unpaired",
                _ => {
                    tracing::debug!("{:?}", response);
                    match response.text().await {
                        Ok(body) => tracing::debug!("body: {body}"),
                        Err(err) => tracing::debug!("body read failed: {err}"),
                    }

                    "Connection problems with the server. Retry later!"
                }
            };
            bot.send_message(msg.chat.id, user_response).await?;
        }
    }

    Ok(())
}
