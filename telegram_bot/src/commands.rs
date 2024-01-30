//! Command structs

use teloxide::utils::command::BotCommands;

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
