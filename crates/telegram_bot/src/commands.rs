//! Command structs

use teloxide::utils::command::{BotCommands, ParseError};

pub fn split_entry(input: String) -> Result<(String, String, String), ParseError> {
    let args: Vec<&str> = input.split(' ').collect();

    if args.len() < 3 {
        Err(ParseError::Custom("Failed to parse the entry".into()))
    } else {
        Ok((
            args[0].to_string(),
            args[1].to_string(),
            args[2..].join(" "),
        ))
    }
}

// TODO: Avoid to hardcode italian strings and commands. Generalize
#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "Comandi per gestire le finanze:"
)]
pub enum EntryCommands {
    #[command(description = "Mostra il seguente messaggio.")]
    Help,
    #[command(
        description = "Inserisce una nuova entrata.",
        parse_with = split_entry
    )]
    Entrata {
        amount: String,
        category: String,
        note: String,
    },
    #[command(
        description = "Inserisce una nuova uscita.",
        parse_with = split_entry
    )]
    Uscita {
        amount: String,
        category: String,
        note: String,
    },
    #[command(description = "Una lista delle ultime entrate ed uscite")]
    Sommario,
    #[command(description = "Elimina una delle entrate.")]
    Elimina,
}

/// Commands to manage user accounts
#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "Comandi per gestire l'account"
)]
pub enum HandleUserAccount {
    #[command(description = "Collega il tuo account telegram.")]
    Pair { code: String },
    #[command(description = "Scollega il tuo account telegram.")]
    UnPair,
}

/// Commands for user statistics
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Comandi per le statistiche")]
pub enum UserStatisticsCommands {
    #[command(description = "Mostra le statistiche del vault")]
    Stats,
}

/// Commands for exporting user data
#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "Comandi per esportare i dati"
)]
pub enum UserExportCommands {
    #[command(description = "Esporta in un file csv le tue voci")]
    Export,
}

/// Start command. Needed when user send /start
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
pub enum UserStartCommands {
    Start,
}
