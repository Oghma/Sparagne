//! Commands and command handler functions.
use reqwest::StatusCode;
use teloxide::{prelude::*, Bot};

use crate::commands::HandleUserAccount;

pub mod entry;

pub async fn handle_pair_user(
    bot: Bot,
    cfg: super::ConfigParameters,
    msg: Message,
    cmd: HandleUserAccount,
) -> ResponseResult<()> {
    let telegram_id = msg.from().map(|user| user.id.to_string()).unwrap();

    match cmd {
        HandleUserAccount::Pair { code } => {
            let response = cfg
                .client
                .post(cfg.server.to_string() + "/user/pair")
                .json(&server::types::user::PairUser { code, telegram_id })
                .send()
                .await
                .unwrap();

            let user_response = match response.status() {
                StatusCode::CREATED => "Account paired",
                _ => {
                    tracing::debug!("{:?}", response);
                    tracing::debug!("body: {}", response.text().await.unwrap());

                    "Connection problems with the server. Retry later!"
                }
            };

            bot.send_message(msg.chat.id, user_response).await?;
        }
        HandleUserAccount::UnPair => {
            let response = cfg
                .client
                .delete(cfg.server + "/user/pair")
                .header("telegram-user-id", telegram_id)
                .send()
                .await
                .unwrap();

            let user_response = match response.status() {
                StatusCode::ACCEPTED => "Account unpaired",
                _ => {
                    tracing::debug!("{:?}", response);
                    tracing::debug!("body: {}", response.text().await.unwrap());

                    "Connection problems with the server. Retry later!"
                }
            };
            bot.send_message(msg.chat.id, user_response).await?;
        }
    }

    Ok(())
}
