//! Import a Sparagne v1 database into a v2 one and print the report.
//!
//! ```text
//! cargo run -p sparagne_core --example import_v1 -- \
//!     <v1.sqlite> <v2.sqlite> --author <username> [--vault <name>] [--tz Europe/Rome]
//! ```
//!
//! The v1 file is opened read-only. The v2 file is created if it does not
//! exist and appended to if it does: the import is idempotent, so running it
//! twice on the same pair of files changes nothing the second time. Close the
//! app before importing into its database.

use std::path::PathBuf;
use std::process::ExitCode;

use sparagne_core::{
    Core,
    import_v1::{DEFAULT_TIMEZONE, ImportOptions, import_v1},
};

const USAGE: &str = "\
usage: import_v1 <v1.sqlite> <v2.sqlite> --author <username> [--vault <name>] [--tz <IANA>]

  <v1.sqlite>        the Sparagne v1 database, opened read-only
  <v2.sqlite>        the v2 database to write to, created if missing
  --author <name>    v2 author of every imported command (the login username)
  --vault <name>     import only this v1 vault; default: every vault of the
                     single v1 user, or every vault when there are several
  --tz <IANA>        timezone of the accounting day; default: Europe/Rome
";

struct Args {
    source: PathBuf,
    target: PathBuf,
    options: ImportOptions,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut positional: Vec<&String> = Vec::new();
    let mut author = None;
    let mut vault = None;
    let mut timezone = DEFAULT_TIMEZONE.to_string();

    let mut rest = argv.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--author" | "--vault" | "--tz" => {
                let value = rest
                    .next()
                    .ok_or_else(|| format!("{arg} needs a value"))?
                    .clone();
                match arg.as_str() {
                    "--author" => author = Some(value),
                    "--vault" => vault = Some(value),
                    _ => timezone = value,
                }
            }
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            _ => positional.push(arg),
        }
    }

    let [source, target] = positional.as_slice() else {
        return Err(format!("expected two file paths, got {}", positional.len()));
    };
    let author = author.ok_or_else(|| "--author is required".to_string())?;
    Ok(Args {
        source: PathBuf::from(source.as_str()),
        target: PathBuf::from(target.as_str()),
        options: ImportOptions {
            author,
            vault,
            timezone,
        },
    })
}

fn run(argv: &[String]) -> Result<(), String> {
    let args = parse_args(argv)?;
    if !args.source.is_file() {
        return Err(format!("{} does not exist", args.source.display()));
    }
    let mut core =
        Core::open(&args.target).map_err(|e| format!("opening {}: {e}", args.target.display()))?;
    let report = import_v1(&mut core, &args.source, &args.options).map_err(|e| e.to_string())?;
    print!("{report}");
    Ok(())
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.is_empty() || argv.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match run(&argv) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("import_v1: {message}");
            eprint!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}
