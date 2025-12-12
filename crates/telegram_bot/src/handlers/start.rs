//! Handler for the /start command

use teloxide::{
    RequestError,
    dispatching::{HandlerExt, UpdateHandler},
    prelude::*,
};

use crate::commands::UserStartCommands;

/// Build the schema for `UserStartCommands` commands
pub fn schema() -> UpdateHandler<RequestError> {
    Update::filter_message()
        .filter_command::<UserStartCommands>()
        .endpoint(handle_start_command)
}

async fn handle_start_command(
    bot: Bot,
    msg: Message,
    cmd: UserStartCommands,
) -> ResponseResult<()> {
    match cmd {
        UserStartCommands::Start => {
            let info_msg = "Per iniziare associa il tuo account telegram utilizzando il comando /pair. Digita\n/pair codice fornito";
            bot.send_message(msg.chat.id, info_msg).await?;
        }
    }

    Ok(())
}
