//! Handler for managing user entries
use engine::CashFlow;
use reqwest::{Client, StatusCode};
use teloxide::{
    dispatching::{UpdateFilterExt, UpdateHandler},
    prelude::*,
    utils::command::BotCommands,
    RequestError,
};

use crate::{
    commands::{split_entry, UserCommands},
    post_check,
};
use crate::{get_check, ConfigParameters};

/// Build the schema for UserCommands commands
pub fn schema() -> UpdateHandler<RequestError> {
    Update::filter_message()
        .branch(
            dptree::entry()
                .filter_command::<UserCommands>()
                .endpoint(handle_user_commands),
        )
        .branch(
            dptree::filter_map(|msg: Message| {
                msg.text().and_then(|text| {
                    split_entry(text.to_string())
                        .and_then(|expense| {
                            Ok(UserCommands::Uscita {
                                amount: expense.0,
                                category: expense.1,
                                note: expense.2,
                            })
                        })
                        .ok()
                })
            })
            .endpoint(handle_user_commands),
        )
}

pub async fn handle_user_commands(
    bot: Bot,
    cfg: ConfigParameters,
    msg: Message,
    cmd: UserCommands,
) -> ResponseResult<()> {
    match cmd {
        UserCommands::Help => {
            let income_info = "– Per registrare una nuova entrata utilizza il comando \\entrata.";
            let expense_info = "– Per registrare una nuova uscita è possibile utilizzare il comando \\uscita o inserirla direttamente";
            let example = "Per esempio\n1.1 Bar Caffè al bar";

            bot.send_message(msg.chat.id, UserCommands::descriptions().to_string())
                .await?;
            bot.send_message(
                msg.chat.id,
                format!("{income_info}\n{expense_info}\n\n{example}"),
            )
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
                -amount,
                &category,
                &note,
                &msg,
                &bot,
            )
            .await?;
        }
        UserCommands::Sommario => {
            let user_id = &msg.from().map(|user| user.id.to_string()).unwrap();

            let (user_response, response) = get_check!(
                cfg.client,
                format!("{}/vault", cfg.server),
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

            let (user_response, response) = get_check!(
                cfg.client,
                format!("{}/cashFlow", cfg.server),
                user_id,
                &server::types::cash_flow::CashFlowGet {
                    name: "Main".to_string(),
                    vault_id: vault.id.unwrap()
                },
                "",
                "Problemi di connessione con il server. Riprova più tardi!"
            );

            let flow = match response {
                None => {
                    bot.send_message(msg.chat.id, user_response).await?;
                    return Ok(());
                }
                Some(response) => response.json::<CashFlow>().await?,
            };

            let mut user_response = String::from("Ultime 10 voci:\n\n");
            flow.entries
                .iter()
                .take(10)
                .enumerate()
                .for_each(|(index, entry)| {
                    let index = (index + 1).to_string();
                    let row = if entry.amount >= 0.0 {
                        format!("{index}. 🟢 {}\n", entry)
                    } else {
                        format!("{index}. 🔴 {}\n", entry)
                    };

                    user_response.push_str(&row);
                });

            bot.send_message(msg.chat.id, user_response).await?;
        }
    };

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
