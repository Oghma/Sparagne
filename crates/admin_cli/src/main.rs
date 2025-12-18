use std::{error::Error, io::Write};

use clap::{Args, Parser, Subcommand};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::Print,
    terminal,
    terminal::ClearType,
};
use engine::{Currency, Engine};
use migration::MigratorTrait;
use sea_orm::{Database, DatabaseConnection, EntityTrait, Set};

mod users {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
    #[sea_orm(table_name = "users")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub username: String,
        pub password: String,
        pub telegram_id: Option<String>,
        pub pair_code: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

#[derive(Parser, Debug)]
#[command(name = "sparagne_admin")]
#[command(about = "Admin utilities for Sparagne (bootstrap users/vaults)")]
struct Cli {
    /// Database connection string (also read from `DATABASE_URL`).
    #[arg(
        long,
        env = "DATABASE_URL",
        default_value = "sqlite:./sparagne.db?mode=rwc"
    )]
    database_url: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    User(User),
    Vault(Vault),
}

#[derive(Args, Debug)]
struct User {
    #[command(subcommand)]
    command: UserCommand,
}

#[derive(Subcommand, Debug)]
enum UserCommand {
    Create(UserCreateArgs),
}

#[derive(Args, Debug)]
struct UserCreateArgs {
    #[arg(long)]
    username: String,
    #[arg(long)]
    telegram_id: Option<String>,
    #[arg(long)]
    pair_code: Option<String>,
}

#[derive(Args, Debug)]
struct Vault {
    #[command(subcommand)]
    command: VaultCommand,
}

#[derive(Subcommand, Debug)]
enum VaultCommand {
    Create(VaultCreateArgs),
}

#[derive(Args, Debug)]
struct VaultCreateArgs {
    #[arg(long)]
    owner: String,
    #[arg(long)]
    name: String,
    #[arg(long, default_value = "EUR")]
    currency: String,
}

fn parse_currency(raw: &str) -> Result<Currency, String> {
    match raw {
        "EUR" | "eur" => Ok(Currency::Eur),
        other => Err(format!("unsupported currency: {other}")),
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> Result<Self, Box<dyn Error + Send + Sync>> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

fn prompt_password(prompt: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    let _raw = RawModeGuard::enter()?;

    let mut out = std::io::stderr();
    execute!(
        out,
        cursor::MoveToColumn(0),
        terminal::Clear(ClearType::CurrentLine),
        Print(prompt)
    )?;
    out.flush()?;

    let mut buf = String::new();
    loop {
        let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event::read()?
        else {
            continue;
        };

        match code {
            KeyCode::Enter => {
                execute!(out, Print("\r\n"))?;
                out.flush()?;
                break;
            }
            KeyCode::Backspace => {
                if buf.pop().is_some() {
                    execute!(out, cursor::MoveLeft(1), Print(" "), cursor::MoveLeft(1))?;
                    out.flush()?;
                }
            }
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                execute!(out, Print("\r\n"))?;
                out.flush()?;
                return Err("interrupted".into());
            }
            KeyCode::Char(ch) if !modifiers.contains(KeyModifiers::CONTROL) => {
                buf.push(ch);
                execute!(out, Print("*"))?;
                out.flush()?;
            }
            _ => {}
        }
    }

    Ok(buf)
}

fn prompt_password_twice() -> Result<String, Box<dyn Error + Send + Sync>> {
    let mut out = std::io::stderr();
    for _ in 0..3 {
        let p1 = prompt_password("Password: ")?;
        if p1.is_empty() {
            execute!(
                out,
                cursor::MoveToColumn(0),
                terminal::Clear(ClearType::CurrentLine),
                Print("Password must not be empty.\r\n")
            )?;
            continue;
        }

        let p2 = prompt_password("Confirm password: ")?;
        if p1 == p2 {
            return Ok(p1);
        }

        execute!(
            out,
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::CurrentLine),
            Print("Passwords do not match. Try again.\r\n")
        )?;
    }

    Err("too many attempts".into())
}

async fn connect_db(
    database_url: &str,
) -> Result<DatabaseConnection, Box<dyn Error + Send + Sync>> {
    let db = Database::connect(database_url).await?;
    migration::Migrator::up(&db, None).await?;
    Ok(db)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let cli = Cli::parse();

    let db = connect_db(&cli.database_url).await?;

    match cli.command {
        Command::User(User {
            command: UserCommand::Create(args),
        }) => {
            let password = prompt_password_twice()?;

            if users::Entity::find_by_id(args.username.clone())
                .one(&db)
                .await?
                .is_some()
            {
                eprintln!("user already exists: {}", args.username);
                std::process::exit(1);
            }

            let user = users::ActiveModel {
                username: Set(args.username.clone()),
                password: Set(password),
                telegram_id: Set(args.telegram_id),
                pair_code: Set(args.pair_code),
            };
            users::Entity::insert(user).exec(&db).await?;

            println!("created user: {}", args.username);
        }
        Command::Vault(Vault {
            command: VaultCommand::Create(args),
        }) => {
            if users::Entity::find_by_id(args.owner.clone())
                .one(&db)
                .await?
                .is_none()
            {
                eprintln!("user not found: {}", args.owner);
                std::process::exit(1);
            }

            let currency = match parse_currency(&args.currency) {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("{err}");
                    std::process::exit(2);
                }
            };

            let engine = Engine::builder().database(db.clone()).build().await?;
            let vault_id = engine
                .new_vault(&args.name, &args.owner, Some(currency))
                .await?;
            println!("created vault: {} ({vault_id})", args.name);
        }
    }

    Ok(())
}
