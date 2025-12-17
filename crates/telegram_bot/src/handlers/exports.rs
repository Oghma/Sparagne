//! Handler for commands that export data

use csv::Writer;
use engine::CashFlow;
use reqwest::StatusCode;
use serde::Serialize;
use teloxide::{RequestError, dispatching::UpdateHandler, prelude::*, types::InputFile};

use crate::{ConfigParameters, commands::UserExportCommands, post_check};

/// Build the schema for Export commands
pub fn schema() -> UpdateHandler<RequestError> {
    Update::filter_message()
        .filter_command::<UserExportCommands>()
        .endpoint(handle_exports)
}

async fn handle_exports(bot: Bot, cfg: ConfigParameters, msg: Message) -> ResponseResult<()> {
    let user_id = match msg.from.as_ref() {
        Some(user) => user.id.to_string(),
        None => {
            bot.send_message(msg.chat.id, "Impossibile identificare l'utente.")
                .await?;
            return Ok(());
        }
    };

    let (user_response, response) = post_check!(
        cfg.client,
        format!("{}/vault/get", cfg.server),
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

    let (user_response, response) = post_check!(
        cfg.client,
        format!("{}/cashFlow/get", cfg.server),
        user_id.clone(),
        &api_types::cash_flow::CashFlowGet {
            vault_id: match vault.id.clone() {
                Some(id) => id,
                None => {
                    bot.send_message(msg.chat.id, "Vault non valido.").await?;
                    return Ok(());
                }
            },
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

    let vault_id = match vault.id {
        Some(id) => id,
        None => {
            bot.send_message(msg.chat.id, "Vault non valido.").await?;
            return Ok(());
        }
    };

    let (user_response, response) = post_check!(
        cfg.client,
        format!("{}/transactions", cfg.server),
        user_id,
        &api_types::transaction::TransactionList {
            vault_id: vault_id.clone(),
            flow_id: Some(flow.id),
            wallet_id: None,
            limit: Some(10_000),
            cursor: None,
            from: None,
            to: None,
            kinds: None,
            include_voided: Some(false),
            include_transfers: Some(false),
        },
        "",
        "Problemi di connessione con il server. Riprova più tardi!"
    );

    let Some(response) = response else {
        bot.send_message(msg.chat.id, user_response).await?;
        return Ok(());
    };
    let list = response
        .json::<api_types::transaction::TransactionListResponse>()
        .await?;

    #[derive(Serialize)]
    struct ExportRow {
        occurred_at: String,
        amount_minor: i64,
        category: Option<String>,
        note: Option<String>,
        id: String,
    }

    let mut writer = Writer::from_writer(vec![]);
    for tx in list.transactions {
        if let Err(err) = writer.serialize(ExportRow {
                occurred_at: tx.occurred_at.to_rfc3339(),
                amount_minor: tx.amount_minor,
                category: tx.category,
                note: tx.note,
                id: tx.id.to_string(),
            }) {
            tracing::error!("failed to serialize export row: {err}");
            bot.send_message(msg.chat.id, "Errore durante l'esportazione.")
                .await?;
            return Ok(());
        }
    }

    let data = match writer.into_inner() {
        Ok(data) => data,
        Err(err) => {
            tracing::error!("failed to finalize export: {err}");
            bot.send_message(msg.chat.id, "Errore durante l'esportazione.")
                .await?;
            return Ok(());
        }
    };

    bot.send_document(
        msg.chat.id,
        InputFile::memory(data).file_name("entries.csv"),
    )
    .await?;

    Ok(())
}
