//! Handler for commands that export data

use csv::Writer;
use engine::CashFlow;
use reqwest::StatusCode;
use teloxide::{RequestError, dispatching::UpdateHandler, prelude::*, types::InputFile};

use crate::{ConfigParameters, commands::UserExportCommands, get_check};

/// Build the schema for Export commands
pub fn schema() -> UpdateHandler<RequestError> {
    Update::filter_message()
        .filter_command::<UserExportCommands>()
        .endpoint(handle_exports)
}

async fn handle_exports(bot: Bot, cfg: ConfigParameters, msg: Message) -> ResponseResult<()> {
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
            return Ok(());
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
            return Ok(());
        }
        Some(response) => response.json::<CashFlow>().await?,
    };

    let mut writer = Writer::from_writer(vec![]);
    writer.serialize(flow.entries).unwrap();

    bot.send_document(
        msg.chat.id,
        InputFile::memory(writer.into_inner().unwrap()).file_name("entries.csv"),
    )
    .await?;

    Ok(())
}
