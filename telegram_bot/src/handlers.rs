//! Commands and command handler functions.
use reqwest::{Client, StatusCode};
use teloxide::{prelude::*, utils::command::BotCommands, Bot};

use crate::{commands::HandleUserAccount, get_check, post_check};

// TODO: Avoid to hardcode italian strings and commands. Generalize
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Commandi supportati:")]
pub enum UserCommands {
    #[command(description = "Mostra questo messaggio.")]
    Help,
    #[command(description = "Inserisce una nuova entrata.", parse_with = "split")]
    Entrata {
        amount: f64,
        category: String,
        note: String,
    },
    #[command(description = "Inserisce una nuova entrata.", parse_with = "split")]
    Uscita {
        amount: f64,
        category: String,
        note: String,
    },
    #[command(description = "Lista di tutte le entrate e uscite")]
    Sommario,
}

pub async fn handle_user_commands(
    bot: Bot,
    cfg: super::ConfigParameters,
    msg: Message,
    cmd: UserCommands,
) -> ResponseResult<()> {
    match cmd {
        UserCommands::Help => {
            bot.send_message(msg.chat.id, UserCommands::descriptions().to_string())
                .await?;
        }
        UserCommands::Entrata {
            amount,
            category,
            note,
        } => {
            send_entry(
                &cfg.client,
                &cfg.server,
                amount,
                &category,
                &note,
                &msg,
                &bot,
            )
            .await?;
        }
        UserCommands::Uscita {
            amount,
            category,
            note,
        } => {
            send_entry(
                &cfg.client,
                &cfg.server,
                amount,
                &category,
                &note,
                &msg,
                &bot,
            )
            .await?;
        }
        UserCommands::Sommario => {
            bot.send_message(msg.chat.id, "TODO".to_string()).await?;
        }
    };

    Ok(())
}

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

async fn send_entry(
    client: &Client,
    url: &str,
    amount: f64,
    category: &str,
    note: &str,
    msg: &Message,
    bot: &Bot,
) -> ResponseResult<()> {
    let user_id = &msg.from().map(|user| user.id.to_string()).unwrap();

    let (user_response, response) = get_check!(
        client,
        format!("{}/vault", url),
        user_id,
        &server::types::vault::Vault {
            id: None,
            name: Some("Main".to_string()),
        },
        "",
        "Problemi di connessione con il server. Riprova più tardi!"
    );

    let vault = match response {
        None => {
            bot.send_message(msg.chat.id, user_response).await?;
            return Ok(());
        }
        Some(response) => response.json::<server::types::vault::Vault>().await?,
    };

    let success_str = if amount >= 0f64 {
        "Entrata inserita"
    } else {
        "Uscita inserita"
    };
    let (user_response, _) = post_check!(
        client,
        format!("{}/entry", url),
        user_id,
        &server::types::entry::EntryNew {
            vault_id: vault.id.unwrap(),
            amount,
            category: category.to_string(),
            note: note.to_string(),
            cash_flow: "Main".to_string()
        },
        StatusCode::CREATED,
        success_str,
        "Problemi di connessione con il server. Riprova più tardi!"
    );

    bot.send_message(msg.chat.id, user_response).await?;
    Ok(())
}
