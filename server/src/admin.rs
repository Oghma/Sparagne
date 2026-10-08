//! `sparagne-server user …`: accounts from the command line, for a server
//! whose registration is closed (`docs/DEPLOY.md` §3).
//!
//! It opens only `server.sqlite` in the data directory; WAL and the busy
//! timeout make that safe while the server runs, and a revoked token stops
//! working on the server's next request. A password is the first line of
//! stdin, never an argument, so it stays out of the shell history and `ps`:
//!
//! ```sh
//! printf '%s\n' "$PW" | docker compose exec -T sparagne sparagne-server user add alice
//! ```

use std::{
    io::{BufRead, Write},
    path::PathBuf,
};

use argon2::Argon2;
use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::{
    auth::{hash_with, normalize_username, validate_password, validate_username},
    db::{ServerDb, User},
    error::ApiError,
    state::SERVER_DB,
};

/// What `sparagne-server help` prints.
pub const USAGE: &str = "\
usage:
  sparagne-server [serve]              run the server (configured by SPARAGNE_* variables)
  sparagne-server user add <name>      create an account; password on the first line of stdin
  sparagne-server user passwd <name>   set a new password (stdin) and log the account out everywhere
  sparagne-server user list            print every account and when it was created
  sparagne-server user revoke <name>   log the account out everywhere

options:
  --data-dir <dir>   the server's data directory (default: $SPARAGNE_DATA_DIR, else ./data)";

/// Why a command did nothing.
#[derive(Debug, Error)]
pub enum AdminError {
    /// The command line itself is wrong: the message plus [`USAGE`].
    #[error("{0}\n\n{USAGE}")]
    Usage(String),
    #[error("{0}")]
    Failed(String),
}

impl From<ApiError> for AdminError {
    fn from(err: ApiError) -> Self {
        Self::Failed(err.message)
    }
}

impl From<std::io::Error> for AdminError {
    fn from(err: std::io::Error) -> Self {
        Self::Failed(err.to_string())
    }
}

/// Runs one admin command. `args` are the arguments after the program name
/// (`["user", "add", "alice"]`), `input` is where the password is read from
/// and `output` gets the result.
pub fn run(
    args: &[String],
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<(), AdminError> {
    let (data_dir, words) = parse(args)?;
    match words.as_slice() {
        [] | ["help"] => {
            writeln!(output, "{USAGE}")?;
            Ok(())
        }
        ["user", "add", name] => add(&open(data_dir)?, name, input, output),
        ["user", "passwd", name] => passwd(&open(data_dir)?, name, input, output),
        ["user", "list"] => list(&open(data_dir)?, output),
        ["user", "revoke", name] => revoke(&open(data_dir)?, name, output),
        _ => Err(AdminError::Usage(format!(
            "unknown command: {}",
            words.join(" ")
        ))),
    }
}

/// Splits `--data-dir` (anywhere, as `--data-dir x` or `--data-dir=x`) from
/// the command words. `-h` and `--help` are the `help` command.
fn parse(args: &[String]) -> Result<(PathBuf, Vec<&str>), AdminError> {
    let mut data_dir = None;
    let mut words = Vec::new();
    let mut rest = args.iter().map(String::as_str);
    while let Some(arg) = rest.next() {
        if arg == "--data-dir" {
            let value = rest
                .next()
                .ok_or_else(|| AdminError::Usage("--data-dir needs a directory".into()))?;
            data_dir = Some(PathBuf::from(value));
        } else if let Some(value) = arg.strip_prefix("--data-dir=") {
            data_dir = Some(PathBuf::from(value));
        } else if arg == "-h" || arg == "--help" {
            words.push("help");
        } else if arg.starts_with('-') {
            return Err(AdminError::Usage(format!("unknown option: {arg}")));
        } else {
            words.push(arg);
        }
    }
    let data_dir = data_dir.unwrap_or_else(|| {
        std::env::var_os("SPARAGNE_DATA_DIR").map_or_else(|| PathBuf::from("./data"), PathBuf::from)
    });
    Ok((data_dir, words))
}

/// The server's database, which must already exist: a mistyped directory
/// must not quietly get an empty one.
fn open(data_dir: PathBuf) -> Result<ServerDb, AdminError> {
    let path = data_dir.join(SERVER_DB);
    if !path.is_file() {
        return Err(AdminError::Failed(format!(
            "{} not found: start the server once, or point --data-dir (or SPARAGNE_DATA_DIR) at its data directory",
            path.display()
        )));
    }
    Ok(ServerDb::open(path)?)
}

fn add(
    db: &ServerDb,
    name: &str,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<(), AdminError> {
    let username = normalize_username(name);
    validate_username(&username)?;
    let password = read_password(input)?;
    let hash = hash_with(&Argon2::default(), &password)?;
    db.insert_user(&username, &hash, Utc::now().timestamp())?;
    writeln!(output, "created user {username}")?;
    Ok(())
}

fn passwd(
    db: &ServerDb,
    name: &str,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<(), AdminError> {
    let user = existing(db, name)?;
    let password = read_password(input)?;
    let hash = hash_with(&Argon2::default(), &password)?;
    db.set_password_hash(user.id, &hash)?;
    let revoked = db.delete_tokens_of_user(user.id, None)?;
    writeln!(
        output,
        "changed the password of {}, revoked {}",
        user.username,
        tokens(revoked)
    )?;
    Ok(())
}

fn list(db: &ServerDb, output: &mut impl Write) -> Result<(), AdminError> {
    for (username, created_at) in db.users()? {
        let created = DateTime::<Utc>::from_timestamp(created_at, 0).map_or_else(
            || created_at.to_string(),
            |at| at.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        );
        writeln!(output, "{username}\t{created}")?;
    }
    Ok(())
}

fn revoke(db: &ServerDb, name: &str, output: &mut impl Write) -> Result<(), AdminError> {
    let user = existing(db, name)?;
    let revoked = db.delete_tokens_of_user(user.id, None)?;
    writeln!(output, "revoked {} of {}", tokens(revoked), user.username)?;
    Ok(())
}

fn existing(db: &ServerDb, name: &str) -> Result<User, AdminError> {
    let username = normalize_username(name);
    db.user_by_username(&username)?
        .ok_or_else(|| AdminError::Failed(format!("no user named {username}")))
}

/// The first line of `input`, without its line ending, validated like the
/// password of `POST /auth/register`.
fn read_password(input: &mut impl BufRead) -> Result<String, AdminError> {
    let mut line = String::new();
    input.read_line(&mut line)?;
    let password = line.strip_suffix('\n').unwrap_or(&line);
    let password = password.strip_suffix('\r').unwrap_or(password);
    if password.is_empty() {
        return Err(AdminError::Failed(
            "no password on stdin: pass it as the first line, e.g. printf '%s\\n' \"$PW\" | sparagne-server user add <name>".into(),
        ));
    }
    validate_password(password)?;
    Ok(password.to_string())
}

fn tokens(count: usize) -> String {
    if count == 1 {
        "1 token".to_string()
    } else {
        format!("{count} tokens")
    }
}
