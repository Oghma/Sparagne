//! Command structs

use teloxide::utils::command::BotCommands;

/// Commands to manage user accounts
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Gestione degli utenti")]
pub enum HandleUserAccount {
    #[command(description = "Pair with an account.")]
    Pair {
        code: String,
    },
    UnPair,
}
