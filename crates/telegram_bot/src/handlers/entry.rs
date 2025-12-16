//! Handler for managing user entries

use chrono::Utc;
use engine::{CashFlow, Currency, Money};
use reqwest::{Client, StatusCode};
use teloxide::{
    RequestError,
    dispatching::{UpdateFilterExt, UpdateHandler, dialogue::InMemStorage},
    prelude::*,
    utils::command::BotCommands,
};

use crate::{
    ConfigParameters,
    commands::{EntryCommands, UserStatisticsCommands, split_entry},
    get_check, post_check,
};

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
            send_help_message(bot, msg).await?;
        }
        EntryCommands::Entrata {
            amount,
            category,
            note,
        } => {
            send_entry(
                &cfg.client,
                &cfg.server,
                &amount,
                false,
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
                &amount,
                true,
                &category,
                &note,
                &msg,
                &bot,
            )
            .await?;
        }
        EntryCommands::Sommario => {
            if let Some(flow) = get_main_cash_flow(&bot, &msg, &cfg).await? {
                let transactions = get_flow_transactions(&cfg, &bot, &msg, &flow.id, 10).await?;
                let user_response = format!(
                    "Ultime 10 voci:\n\n{}",
                    format_transactions(&transactions, 10, vault_currency(&cfg, &bot, &msg).await?)
                );
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
        let transactions = get_flow_transactions(&cfg, &bot, &msg, &flow.id, 10).await?;
        let entries_id: Vec<uuid::Uuid> = transactions.iter().map(|t| t.id).collect();

        let user_response = format!(
            "Quale voce vuoi eliminare?:\n\n{}",
            format_transactions(&transactions, 10, vault_currency(&cfg, &bot, &msg).await?)
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
    entries: Vec<uuid::Uuid>,
) -> ResponseResult<()> {
    let user_id = msg.from.as_ref().map(|user| user.id.to_string()).unwrap();
    let entry_id = entries[msg.text().unwrap().parse::<usize>().unwrap() - 1];

    let (user_response, response) = get_check!(
        cfg.client,
        format!("{}/vault", cfg.server),
        user_id.clone(),
        &api_types::vault::Vault {
            id: None,
            name: Some("Main".to_string()),
            currency: None,
        },
        "",
        "Problemi di connessione con il server. Riprova più tardi!"
    );

    let vault = match response {
        None => {
            bot.send_message(msg.chat.id, user_response).await?;
            return Ok(());
        }
        Some(response) => response.json::<api_types::vault::Vault>().await?,
    };

    let (user_response, _) = post_check!(
        cfg.client,
        format!("{}/transactions/{}/void", cfg.server, entry_id),
        user_id,
        &api_types::transaction::TransactionVoid {
            vault_id: vault.id.unwrap(),
            voided_at: None,
        },
        StatusCode::OK,
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
    let user_id = msg.from.as_ref().map(|user| user.id.to_string()).unwrap();

    let (user_response, response) = get_check!(
        cfg.client,
        format!("{}/vault", cfg.server),
        user_id.clone(),
        &api_types::vault::Vault {
            id: None,
            name: Some("Main".to_string()),
            currency: None,
        },
        "",
        "Problemi di connessione con il server. Riprova più tardi!"
    );

    let vault = match response {
        None => {
            bot.send_message(msg.chat.id, user_response).await?;
            return Ok(None);
        }
        Some(response) => response.json::<api_types::vault::Vault>().await?,
    };

    let (user_response, response) = get_check!(
        cfg.client,
        format!("{}/cashFlow", cfg.server),
        user_id,
        &api_types::cash_flow::CashFlowGet {
            vault_id: vault.id.unwrap(),
            id: None,
            name: Some("Main".to_string()),
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
fn format_transactions(
    txs: &[api_types::transaction::TransactionView],
    num_entries: usize,
    currency: Currency,
) -> String {
    let mut user_response = String::new();
    txs.iter()
        .take(num_entries)
        .enumerate()
        .for_each(|(index, tx)| {
            let idx = index + 1;
            let money = Money::new(tx.amount_minor).format(currency);
            let category = tx.category.as_deref().unwrap_or("-");
            let note = tx.note.as_deref().unwrap_or("");

            let row = if tx.amount_minor >= 0 {
                format!("{idx}. 🟢 {money} {category} {note}\n")
            } else {
                format!("{idx}. 🔴 {money} {category} {note}\n")
            };

            user_response.push_str(&row);
        });
    user_response
}

async fn vault_currency(
    cfg: &ConfigParameters,
    bot: &Bot,
    msg: &Message,
) -> ResponseResult<Currency> {
    let user_id = msg.from.as_ref().map(|user| user.id.to_string()).unwrap();
    let (user_response, response) = get_check!(
        cfg.client,
        format!("{}/vault", cfg.server),
        user_id,
        &api_types::vault::Vault {
            id: None,
            name: Some("Main".to_string()),
            currency: None,
        },
        "",
        "Problemi di connessione con il server. Riprova più tardi!"
    );
    let Some(response) = response else {
        bot.send_message(msg.chat.id, user_response).await?;
        return Ok(Currency::Eur);
    };
    let vault = response.json::<api_types::vault::Vault>().await?;
    Ok(match vault.currency.unwrap_or(api_types::Currency::Eur) {
        api_types::Currency::Eur => Currency::Eur,
    })
}

async fn get_flow_transactions(
    cfg: &ConfigParameters,
    bot: &Bot,
    msg: &Message,
    flow_id: &uuid::Uuid,
    limit: u64,
) -> ResponseResult<Vec<api_types::transaction::TransactionView>> {
    let user_id = msg.from.as_ref().map(|user| user.id.to_string()).unwrap();

    let (user_response, response) = get_check!(
        cfg.client,
        format!("{}/vault", cfg.server),
        user_id.clone(),
        &api_types::vault::Vault {
            id: None,
            name: Some("Main".to_string()),
            currency: None,
        },
        "",
        "Problemi di connessione con il server. Riprova più tardi!"
    );
    let vault = match response {
        None => {
            bot.send_message(msg.chat.id, user_response).await?;
            return Ok(Vec::new());
        }
        Some(response) => response.json::<api_types::vault::Vault>().await?,
    };

    let (user_response, response) = get_check!(
        cfg.client,
        format!("{}/transactions", cfg.server),
        user_id,
        &api_types::transaction::TransactionList {
            vault_id: vault.id.unwrap(),
            flow_id: Some(*flow_id),
            wallet_id: None,
            limit: Some(limit),
            include_voided: Some(false),
            include_transfers: Some(false),
        },
        "",
        "Problemi di connessione con il server. Riprova più tardi!"
    );

    let Some(response) = response else {
        bot.send_message(msg.chat.id, user_response).await?;
        return Ok(Vec::new());
    };
    let list = response
        .json::<api_types::transaction::TransactionListResponse>()
        .await?;
    Ok(list.transactions)
}

async fn send_entry(
    client: &Client,
    url: &str,
    amount_str: &str,
    is_expense: bool,
    category: &str,
    note: &str,
    msg: &Message,
    bot: &Bot,
) -> ResponseResult<()> {
    let user_id = msg.from.as_ref().map(|user| user.id.to_string()).unwrap();

    let (user_response, response) = get_check!(
        client,
        format!("{}/vault", url),
        user_id.clone(),
        &api_types::vault::Vault {
            id: None,
            name: Some("Main".to_string()),
            currency: None,
        },
        "",
        "Problemi di connessione con il server. Riprova più tardi!"
    );

    let vault = match response {
        None => {
            bot.send_message(msg.chat.id, user_response).await?;
            return Ok(());
        }
        Some(response) => response.json::<api_types::vault::Vault>().await?,
    };

    let (user_response, response) = get_check!(
        client,
        format!("{}/cashFlow", url),
        user_id.clone(),
        &api_types::cash_flow::CashFlowGet {
            vault_id: vault.id.clone().unwrap(),
            id: None,
            name: Some("Main".to_string()),
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

    let currency = match vault.currency.unwrap_or(api_types::Currency::Eur) {
        api_types::Currency::Eur => Currency::Eur,
    };

    let amount = match Money::parse_major(amount_str, currency) {
        Ok(v) => v,
        Err(_) => {
            bot.send_message(msg.chat.id, "Importo non valido (es: 10 o 10.50)")
                .await?;
            return Ok(());
        }
    };

    let amount_minor = amount.minor().abs();

    let endpoint = if is_expense { "expense" } else { "income" };
    let (user_response, _) = if is_expense {
        post_check!(
            client,
            format!("{}/{}", url, endpoint),
            user_id,
            &api_types::transaction::ExpenseNew {
                vault_id: vault.id.unwrap(),
                amount_minor,
                flow_id: Some(flow.id),
                wallet_id: None,
                category: Some(category.to_string()),
                note: Some(note.to_string()),
                occurred_at: Utc::now().into(),
            },
            StatusCode::CREATED,
            "Uscita inserita",
            "Problemi di connessione con il server. Riprova più tardi!"
        )
    } else {
        post_check!(
            client,
            format!("{}/{}", url, endpoint),
            user_id,
            &api_types::transaction::IncomeNew {
                vault_id: vault.id.unwrap(),
                amount_minor,
                flow_id: Some(flow.id),
                wallet_id: None,
                category: Some(category.to_string()),
                note: Some(note.to_string()),
                occurred_at: Utc::now().into(),
            },
            StatusCode::CREATED,
            "Entrata inserita",
            "Problemi di connessione con il server. Riprova più tardi!"
        )
    };

    bot.send_message(msg.chat.id, user_response).await?;

    Ok(())
}

pub async fn send_help_message(bot: Bot, msg: Message) -> ResponseResult<()> {
    let help_message = format!(
        "Sparagne! Per monitorare il tuo budget!\n{}\n{}",
        EntryCommands::descriptions(),
        UserStatisticsCommands::descriptions(),
    );

    let income_info = "– Per registrare una nuova entrata utilizza il comando \\entrata importo nome categoria note";
    let income_example = "Per esempio\n\\entrata 1000 stipendio Stipendio di gennaio";

    let expense_info = "– Per registrare una nuova uscita è possibile utilizzare il comando \\uscita o inserirla direttamente";
    let expense_example = "Per esempio\n1.1 Bar Caffè al bar o \\uscita 1.1 Bar Caffè al bar";

    bot.send_message(msg.chat.id, help_message).await?;
    bot.send_message(msg.chat.id, format!("{income_info}\n{income_example}"))
        .await?;
    bot.send_message(msg.chat.id, format!("{expense_info}\n{expense_example}"))
        .await?;

    Ok(())
}
