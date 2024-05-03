//! Handler for managing user entries
use engine::CashFlow;
use reqwest::{Client, StatusCode};
use teloxide::{
    dispatching::{dialogue::InMemStorage, UpdateFilterExt, UpdateHandler},
    prelude::*,
    utils::command::BotCommands,
    RequestError,
};

use crate::{
    commands::{split_entry, EntryCommands, UserStatisticsCommands},
    delete_check, post_check,
};
use crate::{get_check, ConfigParameters};

use super::{GlobalDialogue, GlobalState};

/// Build the schema for EntryCommands commands
pub fn schema() -> UpdateHandler<RequestError> {
    Update::filter_message()
        .enter_dialogue::<Update, InMemStorage<GlobalState>, GlobalState>()
        .branch(
            dptree::entry()
                .filter_command::<EntryCommands>()
                .branch(
                    dptree::case![EntryCommands::Elimina]
                        .branch(dptree::case![GlobalState::Idle].endpoint(handle_delete_list)),
                )
                .branch(dptree::entry().endpoint(handle_user_commands)),
        )
        .branch(dptree::case![GlobalState::InDelete(entries)].endpoint(handle_delete_entry))
        .branch(
            // Handle expenses inserted without /expense command
            dptree::filter_map(|msg: Message| {
                msg.text().and_then(|text| {
                    split_entry(text.to_string())
                        .map(|expense| EntryCommands::Uscita {
                            amount: expense.0,
                            category: expense.1,
                            note: expense.2,
                        })
                        .ok()
                })
            })
            .endpoint(handle_user_commands),
        )
}

async fn handle_user_commands(
    bot: Bot,
    cfg: ConfigParameters,
    msg: Message,
    cmd: EntryCommands,
) -> ResponseResult<()> {
    match cmd {
        EntryCommands::Help => {
            let help_message = format!(
                "Sparagne! Per monitorare il tuo budget!\n{}\n{}",
                EntryCommands::descriptions(),
                UserStatisticsCommands::descriptions(),
            );

            let income_info = "– Per registrare una nuova entrata utilizza il comando \\entrata importo nome categoria note";
            let income_example = "Per esempio\n\\entrata 1000 stipendio Stipendio di gennaio";

            let expense_info = "– Per registrare una nuova uscita è possibile utilizzare il comando \\uscita o inserirla direttamente";
            let expense_example =
                "Per esempio\n1.1 Bar Caffè al bar o \\uscita 1.1 Bar Caffè al bar";

            bot.send_message(msg.chat.id, help_message).await?;
            bot.send_message(msg.chat.id, format!("{income_info}\n{income_example}"))
                .await?;
            bot.send_message(msg.chat.id, format!("{expense_info}\n{expense_example}"))
                .await?;
        }
        EntryCommands::Entrata {
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
        EntryCommands::Uscita {
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
        EntryCommands::Sommario => {
            if let Some(flow) = get_main_cash_flow(&bot, &msg, &cfg).await? {
                let user_response = format!("Ultime 10 voci:\n\n{}", format_entries(&flow, 10));
                bot.send_message(msg.chat.id, user_response).await?;
            };
        }
        EntryCommands::Elimina => {
            tracing::info!("error in receiving delete command");
        }
    };

    Ok(())
}

/// Show a list of entries of possible entries can be deleted.
///
/// NOTE: This is the first step of the deletion command
async fn handle_delete_list(
    bot: Bot,
    cfg: ConfigParameters,
    msg: Message,
    dialogue: GlobalDialogue,
) -> ResponseResult<()> {
    if let Some(flow) = get_main_cash_flow(&bot, &msg, &cfg).await? {
        let entries_id = flow
            .entries
            .iter()
            .take(10)
            .map(|entry| (entry.id.clone(), flow.name.clone()))
            .collect();

        let user_response = format!(
            "Quale voce vuoi eliminare?:\n\n{}",
            format_entries(&flow, 10)
        );
        bot.send_message(msg.chat.id, user_response).await?;
        dialogue
            .update(GlobalState::InDelete(entries_id))
            .await
            .unwrap();
    };

    Ok(())
}

/// Delete the entry selected by the user.
///
/// NOTE: This is the second step of the deletion command
async fn handle_delete_entry(
    bot: Bot,
    cfg: ConfigParameters,
    msg: Message,
    dialogue: GlobalDialogue,
    entries: Vec<(String, String)>,
) -> ResponseResult<()> {
    let user_id = msg.from().map(|user| user.id.to_string()).unwrap();
    let entry = &entries[msg.text().unwrap().parse::<usize>().unwrap() - 1];

    let (user_response, response) = get_check!(
        cfg.client,
        format!("{}/vault", cfg.server),
        user_id.clone(),
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

    let (user_response, _) = delete_check!(
        cfg.client,
        format!("{}/entry", cfg.server),
        user_id,
        &server::types::entry::EntryDelete {
            vault_id: vault.id.unwrap(),
            entry_id: entry.0.clone(),
            cash_flow: Some(entry.1.clone()),
            wallet: None
        },
        "Voce eliminata",
        "Problemi di connessione con il server. Riprova più tardi!"
    );

    bot.send_message(msg.chat.id, user_response).await?;
    dialogue.exit().await.unwrap();
    Ok(())
}

/// Fetch "Main" Cash flow
async fn get_main_cash_flow(
    bot: &Bot,
    msg: &Message,
    cfg: &ConfigParameters,
) -> ResponseResult<Option<CashFlow>> {
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
            return Ok(None);
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
            return Ok(None);
        }
        Some(response) => response.json::<CashFlow>().await?,
    };

    Ok(Some(flow))
}

/// Format the CashFlow entries
fn format_entries(flow: &CashFlow, num_entries: usize) -> String {
    let mut user_response = String::new();
    flow.entries
        .iter()
        .take(num_entries)
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
    user_response
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
