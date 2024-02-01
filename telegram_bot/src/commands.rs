//! Command structs

use teloxide::utils::command::{BotCommands, ParseError};

pub fn split_entry(input: String) -> Result<(f64, String, String), ParseError> {
    let args: Vec<&str> = input.split(' ').collect();

    if args.len() < 3 {
        Err(ParseError::Custom("Failed to parse the entry".into()))
    } else {
        let Ok(amount) = args[0].parse() else {
            return Err(ParseError::Custom("Failed to parse the entry".into()));
        };

        Ok((amount, args[1].to_string(), args[2..].join(" ")))
    }
}

// TODO: Avoid to hardcode italian strings and commands. Generalize
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Commandi supportati:")]
pub enum UserCommands {
    #[command(description = "Mostra questo messaggio.")]
    Help,
    #[command(
        description = "Inserisce una nuova entrata.",
        parse_with = split_entry
    )]
    Entrata {
        amount: f64,
        category: String,
        note: String,
    },
    #[command(
        description = "Inserisce una nuova entrata.",
        parse_with = split_entry
    )]
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
