//! Acceptance tests for the person of a transaction and the owner of a
//! recurring template: who a row is for, apart from who recorded it.
//!
//! The default is the author and is worked out when a command is applied, so
//! most of these tests are about replay: a log written before the fields
//! existed, a database migrated from version 3 and a fresh replay must all
//! agree.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use common::{T0, all, at, entry, list};
use rusqlite::Connection;
use sparagne_core::{
    Command, CommandEnvelope, Core, Currency, DomainError, FlowMode, Frequency, RecurringPatch,
    Schedule, TransactionKind, TransactionPatch, replay,
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// matteo's vault with a wallet `Conto` holding 1000.00 and a `Vacanze`
/// envelope funded with 100.00; elisa shares it.
struct Fx {
    core: Core,
    vault: Uuid,
    conto: Uuid,
    vacanze: Uuid,
}

fn setup() -> Fx {
    setup_on(Core::open_in_memory().unwrap())
}

/// [`setup`] on a given database.
fn setup_on(mut core: Core) -> Fx {
    let vault = core
        .execute(CommandEnvelope::create_vault(
            "matteo",
            "Casa",
            Currency::Eur,
        ))
        .unwrap()
        .command_id;
    let mut fx = Fx {
        core,
        vault,
        conto: Uuid::nil(),
        vacanze: Uuid::nil(),
    };
    fx.conto = fx.run("matteo", common::wallet_cmd("Conto", 100_000));
    fx.vacanze = fx.run(
        "matteo",
        common::flow_cmd("Vacanze", FlowMode::Unlimited, false, 10_000),
    );
    fx
}

impl Fx {
    /// Runs `command` as `author` and returns the id it created, if any.
    fn run(&mut self, author: &str, command: Command) -> Uuid {
        self.try_run(author, command)
            .unwrap_or_else(|e| panic!("{e}"))
    }

    fn try_run(&mut self, author: &str, command: Command) -> Result<Uuid, DomainError> {
        self.core
            .execute(CommandEnvelope::new(self.vault, author, command))
            .map(|receipt| receipt.result_id.unwrap_or(receipt.command_id))
    }

    /// `(person, created_by)` of one transaction.
    fn who(&self, id: Uuid) -> (String, String) {
        let row = self.core.transaction(self.vault, id).unwrap();
        (row.person, row.created_by)
    }

    fn owner(&self, recurring_id: Uuid) -> String {
        self.core
            .list_recurring(self.vault, true)
            .unwrap()
            .into_iter()
            .find(|t| t.id == recurring_id)
            .unwrap()
            .owner
    }
}

fn pair(person: &str, created_by: &str) -> (String, String) {
    (person.to_string(), created_by.to_string())
}

/// An expense of `amount` on `Conto`, for `person`.
fn spend(fx: &Fx, amount: i64, person: Option<&str>) -> Command {
    let mut e = entry(amount, Some(fx.conto), None, Some("Spesa"), T0 + 60);
    e.person = person.map(str::to_string);
    Command::Expense(e)
}

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// A monthly 780.00 expense on the 1st from November 2023 out of `wallet`,
/// owned by `owner`.
fn mutuo(wallet: Uuid, owner: Option<&str>) -> Command {
    Command::CreateRecurring {
        transaction_kind: TransactionKind::Expense,
        amount: 78_000,
        wallet_id: Some(wallet),
        flow_id: None,
        category: Some("Casa".to_string()),
        note: Some("mutuo".to_string()),
        schedule: Schedule {
            frequency: Frequency::Monthly { day: 1 },
            interval: 1,
            start_date: date(2023, 11, 1),
            end_date: None,
        },
        owner: owner.map(str::to_string),
    }
}

fn execute(recurring_id: Uuid, month: u32, person: Option<&str>) -> Command {
    Command::ExecuteRecurring {
        recurring_id,
        period_date: date(2023, month, 1),
        occurred_at: at(T0 + i64::from(month)),
        person: person.map(str::to_string),
    }
}

fn set_person(transaction_id: Uuid, person: &str) -> Command {
    Command::UpdateTransaction {
        transaction_id,
        patch: TransactionPatch {
            person: Some(person.to_string()),
            ..TransactionPatch::default()
        },
    }
}

fn set_owner(recurring_id: Uuid, owner: &str) -> Command {
    Command::UpdateRecurring {
        recurring_id,
        patch: RecurringPatch {
            owner: Some(owner.to_string()),
            ..RecurringPatch::default()
        },
    }
}

// ---------------------------------------------------------------------------
// Entries
// ---------------------------------------------------------------------------

#[test]
fn an_entry_is_its_authors_unless_it_names_someone_else() {
    let mut fx = setup();
    let own = spend(&fx, 1_000, None);
    let own = fx.run("matteo", own);
    let for_elisa = spend(&fx, 2_000, Some("elisa"));
    let for_elisa = fx.run("matteo", for_elisa);
    let blank = spend(&fx, 3_000, Some("   "));
    let blank = fx.run("elisa", blank);
    let padded = spend(&fx, 4_000, Some(" elisa "));
    let padded = fx.run("matteo", padded);

    assert_eq!(fx.who(own), pair("matteo", "matteo"));
    assert_eq!(fx.who(for_elisa), pair("elisa", "matteo"));
    // A blank person is no person: the author's.
    assert_eq!(fx.who(blank), pair("elisa", "elisa"));
    assert_eq!(fx.who(padded), pair("elisa", "matteo"));
}

#[test]
fn transfers_and_opening_balances_are_always_their_authors() {
    let fx = setup();
    let rows = list(&fx.core, fx.vault, &all());
    assert_eq!(rows.len(), 2, "the wallet's opening and the envelope's");
    for row in rows {
        assert_eq!(row.person, "matteo", "{:?}", row.kind);
    }
}

#[test]
fn a_patch_may_change_only_the_person_and_a_blank_one_gives_it_back() {
    let mut fx = setup();
    let id = spend(&fx, 1_000, None);
    let id = fx.run("matteo", id);
    let before = fx.core.transaction(fx.vault, id).unwrap();

    // A patch that carries nothing but the person is not empty.
    fx.run("elisa", set_person(id, "elisa"));
    let after = fx.core.transaction(fx.vault, id).unwrap();
    assert_eq!(after.person, "elisa");
    assert_eq!(after.created_by, "matteo");
    assert_eq!(
        (after.amount, after.note, after.legs),
        (before.amount, before.note, before.legs)
    );

    // Blank gives the row back to whoever recorded it, not to whoever
    // patched it.
    fx.run("elisa", set_person(id, " "));
    assert_eq!(fx.who(id), pair("matteo", "matteo"));

    // A patch without the person leaves it alone.
    fx.run("elisa", set_person(id, "elisa"));
    fx.run(
        "matteo",
        Command::UpdateTransaction {
            transaction_id: id,
            patch: TransactionPatch {
                note: Some("coop".to_string()),
                ..TransactionPatch::default()
            },
        },
    );
    assert_eq!(fx.who(id), pair("elisa", "matteo"));
}

#[test]
fn a_transfer_refuses_a_person_even_a_blank_one() {
    let mut fx = setup();
    let unallocated = fx.core.snapshot(fx.vault).unwrap().unallocated_flow_id;
    let transfer = fx.run(
        "matteo",
        Command::TransferFlow {
            amount: 1_000,
            from_flow_id: fx.vacanze,
            to_flow_id: unallocated,
            note: None,
            occurred_at: at(T0 + 60),
        },
    );
    for person in ["elisa", ""] {
        assert_eq!(
            fx.try_run("elisa", set_person(transfer, person)),
            Err(DomainError::InvalidCommand(
                "person is only valid on entries".to_string()
            ))
        );
    }
    assert_eq!(fx.who(transfer), pair("matteo", "matteo"));
}

// ---------------------------------------------------------------------------
// Recurring templates
// ---------------------------------------------------------------------------

#[test]
fn a_template_is_its_creators_unless_it_names_an_owner() {
    let mut fx = setup();
    let own = fx.run("matteo", mutuo(fx.conto, None));
    let elisas = fx.run("matteo", mutuo(fx.conto, Some("elisa")));
    let blank = fx.run("elisa", mutuo(fx.conto, Some("")));
    assert_eq!(fx.owner(own), "matteo");
    assert_eq!(fx.owner(elisas), "elisa");
    assert_eq!(fx.owner(blank), "elisa");

    // An owner-only patch is not empty; a blank owner goes back to the
    // creator, whoever sends it.
    fx.run("elisa", set_owner(own, "elisa"));
    assert_eq!(fx.owner(own), "elisa");
    fx.run("elisa", set_owner(own, "  "));
    assert_eq!(fx.owner(own), "matteo");
    // Other fields leave the owner alone.
    fx.run(
        "matteo",
        Command::UpdateRecurring {
            recurring_id: elisas,
            patch: RecurringPatch {
                amount: Some(80_000),
                ..RecurringPatch::default()
            },
        },
    );
    assert_eq!(fx.owner(elisas), "elisa");
}

#[test]
fn an_execution_is_for_the_person_it_names_and_recorded_by_its_author() {
    let mut fx = setup();
    let template = fx.run("matteo", mutuo(fx.conto, Some("elisa")));

    // What the app sends: the owner as the person.
    let paid = fx.run("matteo", execute(template, 11, Some("elisa")));
    assert_eq!(fx.who(paid), pair("elisa", "matteo"));

    // Without a person the row is the executor's: the owner is never read
    // when the command is applied, or changing it would rewrite this row on
    // the next replay.
    let unnamed = fx.run("matteo", execute(template, 12, None));
    assert_eq!(fx.who(unnamed), pair("matteo", "matteo"));

    fx.run("matteo", set_owner(template, "matteo"));
    let log = fx.core.commands_since(fx.vault, 0).unwrap();
    let mut fresh = Core::open_in_memory().unwrap();
    replay(&log, &mut fresh).unwrap();
    assert_eq!(
        fresh.transaction(fx.vault, paid).unwrap().person,
        "elisa",
        "a later owner does not move a past execution"
    );
    assert_eq!(
        list(&fresh, fx.vault, &all()),
        list(&fx.core, fx.vault, &all())
    );
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

#[test]
fn people_and_the_person_filter_read_the_person_not_the_author() {
    let mut fx = setup();
    let for_elisa = spend(&fx, 2_000, Some("elisa"));
    let for_elisa = fx.run("matteo", for_elisa);
    assert_eq!(fx.core.people(fx.vault).unwrap(), ["elisa", "matteo"]);

    let filter = sparagne_core::TransactionFilter {
        person: Some("elisa".to_string()),
        ..Default::default()
    };
    let rows = fx
        .core
        .list_transactions(fx.vault, &filter, 50, None)
        .unwrap();
    assert_eq!(
        rows.items.iter().map(|t| t.id).collect::<Vec<_>>(),
        [for_elisa]
    );

    fx.run(
        "matteo",
        Command::VoidTransaction {
            transaction_id: for_elisa,
        },
    );
    assert_eq!(fx.core.people(fx.vault).unwrap(), ["matteo"]);
}

// ---------------------------------------------------------------------------
// The JSON of the log
// ---------------------------------------------------------------------------

/// Commands exactly as a version 3 client wrote them into the log: they
/// parse, apply with every row on the author, and serialize back byte for
/// byte, since a person or owner that is `None` is left out.
#[test]
fn a_version_3_payload_replays_on_its_author_and_serializes_unchanged() {
    let mut fx = setup();
    let conto = fx.conto.hyphenated();
    let expense = format!(
        r#"{{"kind":"expense","amount":1250,"wallet_id":"{conto}","flow_id":null,"category":"Spesa","note":"coop","occurred_at":"2023-11-14T23:13:20+01:00"}}"#
    );
    let create = r#"{"kind":"create_recurring","transaction_kind":"expense","amount":78000,"wallet_id":null,"flow_id":null,"category":"Casa","note":"mutuo","schedule":{"frequency":{"unit":"monthly","day":1},"interval":1,"start_date":"2023-11-01","end_date":null}}"#;

    let parsed: Command = serde_json::from_str(&expense).unwrap();
    assert_eq!(serde_json::to_string(&parsed).unwrap(), expense);
    let spent = fx.run("elisa", parsed);
    assert_eq!(fx.who(spent), pair("elisa", "elisa"));

    let parsed: Command = serde_json::from_str(create).unwrap();
    assert_eq!(serde_json::to_string(&parsed).unwrap(), create);
    let template = fx.run("matteo", parsed);
    assert_eq!(fx.owner(template), "matteo");

    let execute = format!(
        r#"{{"kind":"execute_recurring","recurring_id":"{}","period_date":"2023-11-01","occurred_at":"2023-11-14T23:13:20+01:00"}}"#,
        template.hyphenated()
    );
    let update_tx = format!(
        r#"{{"kind":"update_transaction","transaction_id":"{}","patch":{{"amount":null,"occurred_at":null,"category":null,"note":"esselunga","wallet_id":null,"flow_id":null,"from_id":null,"to_id":null}}}}"#,
        spent.hyphenated()
    );
    let update_rec = format!(
        r#"{{"kind":"update_recurring","recurring_id":"{}","patch":{{"amount":80000,"wallet_id":null,"flow_id":null,"category":null,"note":null,"schedule":null,"enabled":null}}}}"#,
        template.hyphenated()
    );
    for json in [&execute, &update_tx, &update_rec] {
        let parsed: Command = serde_json::from_str(json).unwrap();
        assert_eq!(&serde_json::to_string(&parsed).unwrap(), json);
    }
    let paid = fx.run("elisa", serde_json::from_str(&execute).unwrap());
    assert_eq!(fx.who(paid), pair("elisa", "elisa"));
    fx.run("matteo", serde_json::from_str(&update_tx).unwrap());
    assert_eq!(fx.who(spent), pair("elisa", "elisa"));
    fx.run("elisa", serde_json::from_str(&update_rec).unwrap());
    assert_eq!(fx.owner(template), "matteo");
}

#[test]
fn a_person_or_owner_is_in_the_json_only_when_named() {
    let fx = setup();
    let named = [
        spend(&fx, 1, Some("elisa")),
        mutuo(Uuid::nil(), Some("elisa")),
        execute(Uuid::nil(), 11, Some("elisa")),
        set_person(Uuid::nil(), "elisa"),
        set_owner(Uuid::nil(), "elisa"),
    ];
    for command in named {
        let json = serde_json::to_string(&command).unwrap();
        assert!(
            json.contains(r#""person":"elisa""#) || json.contains(r#""owner":"elisa""#),
            "{json}"
        );
        assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
    }
    for command in [
        spend(&fx, 1, None),
        mutuo(Uuid::nil(), None),
        execute(Uuid::nil(), 11, None),
    ] {
        let json = serde_json::to_string(&command).unwrap();
        assert!(
            !json.contains("person") && !json.contains("owner"),
            "{json}"
        );
    }
}

// ---------------------------------------------------------------------------
// Migration
// ---------------------------------------------------------------------------

fn temp_database() -> PathBuf {
    std::env::temp_dir().join(format!("sparagne-person-{}.sqlite", Uuid::now_v7()))
}

fn remove_database(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
}

/// A version 3 file is migrated to what a replay of its log gives: every
/// person is the author and every owner the creator, including the period
/// elisa executed on matteo's template.
#[test]
fn a_version_3_database_migrates_to_what_its_log_replays_to() {
    let path = temp_database();
    let (vault, log, paid, template) = {
        let mut fx = setup_on(Core::open(&path).unwrap());
        let vault = fx.vault;
        let contanti = fx.run("elisa", common::wallet_cmd("Contanti", 5_000));
        let matteos = spend(&fx, 1_000, None);
        let matteos = fx.run("matteo", matteos);
        let elisas = spend(&fx, 2_000, None);
        let elisas = fx.run("elisa", elisas);
        fx.run(
            "elisa",
            Command::TransferWallet {
                amount: 500,
                from_wallet_id: contanti,
                to_wallet_id: fx.conto,
                note: None,
                occurred_at: at(T0 + 90),
            },
        );
        let template = fx.run("matteo", mutuo(fx.conto, None));
        let paid = fx.run("elisa", execute(template, 11, None));
        fx.run(
            "elisa",
            Command::UpdateTransaction {
                transaction_id: matteos,
                patch: TransactionPatch {
                    note: Some("coop".to_string()),
                    ..TransactionPatch::default()
                },
            },
        );
        fx.run(
            "matteo",
            Command::VoidTransaction {
                transaction_id: elisas,
            },
        );
        let log = fx.core.commands_since(vault, 0).unwrap();
        (vault, log, paid, template)
    };

    // What version 3 left on disk: no person, no owner, and none of the
    // tables later versions added.
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "ALTER TABLE transactions DROP COLUMN person;
             ALTER TABLE recurring_templates DROP COLUMN owner;
             DROP TABLE allocation_runs;
             DROP TABLE allocation_plans;
             PRAGMA user_version = 3;",
        )
        .unwrap();
    }

    let migrated = Core::open(&path).unwrap();
    let mut replayed = Core::open_in_memory().unwrap();
    replay(&log, &mut replayed).unwrap();

    let rows = |core: &Core| list(core, vault, &all());
    assert_eq!(rows(&migrated), rows(&replayed));
    assert_eq!(
        migrated.list_recurring(vault, true).unwrap(),
        replayed.list_recurring(vault, true).unwrap()
    );
    assert_eq!(
        migrated.people(vault).unwrap(),
        replayed.people(vault).unwrap()
    );
    assert_eq!(
        migrated.snapshot(vault).unwrap(),
        replayed.snapshot(vault).unwrap()
    );

    let row = migrated.transaction(vault, paid).unwrap();
    assert_eq!(
        (row.person.as_str(), row.created_by.as_str()),
        ("elisa", "elisa")
    );
    let owner = &migrated.list_recurring(vault, true).unwrap()[0];
    assert_eq!((owner.id, owner.owner.as_str()), (template, "matteo"));
    assert!(rows(&migrated).iter().all(|t| t.person == t.created_by));

    drop(migrated);
    remove_database(&path);
}

// ---------------------------------------------------------------------------
// The names a command carries
// ---------------------------------------------------------------------------

/// One command of every kind, each naming "elisa" where it can.
fn every_command(name: Option<&str>) -> Vec<Command> {
    let id = Uuid::nil();
    let when = at(T0);
    let named = name.map(str::to_string);
    let mut e = entry(1, None, None, None, T0);
    e.person.clone_from(&named);
    let mut with_owner = mutuo(id, None);
    if let Command::CreateRecurring { owner, .. } = &mut with_owner {
        owner.clone_from(&named);
    }
    vec![
        Command::CreateVault {
            name: "Casa".to_string(),
            currency: Currency::Eur,
        },
        Command::RenameVault {
            name: "Casa".to_string(),
        },
        Command::DeleteVault,
        common::wallet_cmd("Conto", 0),
        Command::RenameWallet {
            wallet_id: id,
            name: "Conto".to_string(),
        },
        Command::ArchiveWallet { wallet_id: id },
        Command::RestoreWallet { wallet_id: id },
        common::flow_cmd("Vacanze", FlowMode::Unlimited, false, 0),
        Command::UpdateFlow {
            flow_id: id,
            name: None,
            mode: None,
            allow_negative: None,
        },
        Command::ArchiveFlow { flow_id: id },
        Command::RestoreFlow { flow_id: id },
        Command::CreateCategory {
            name: "Spesa".to_string(),
        },
        Command::RenameCategory {
            category_id: id,
            name: "Spesa".to_string(),
        },
        Command::ArchiveCategory { category_id: id },
        Command::RestoreCategory { category_id: id },
        Command::AddAlias {
            category_id: id,
            alias: "coop".to_string(),
        },
        Command::RemoveAlias {
            category_id: id,
            alias: "coop".to_string(),
        },
        Command::MergeCategory {
            source_id: id,
            target_id: id,
        },
        Command::Income(e.clone()),
        Command::Expense(e.clone()),
        Command::Refund(e),
        Command::TransferWallet {
            amount: 1,
            from_wallet_id: id,
            to_wallet_id: id,
            note: None,
            occurred_at: when,
        },
        Command::TransferFlow {
            amount: 1,
            from_flow_id: id,
            to_flow_id: id,
            note: None,
            occurred_at: when,
        },
        Command::UpdateTransaction {
            transaction_id: id,
            patch: TransactionPatch {
                person: named.clone(),
                ..TransactionPatch::default()
            },
        },
        Command::VoidTransaction { transaction_id: id },
        with_owner,
        Command::UpdateRecurring {
            recurring_id: id,
            patch: RecurringPatch {
                owner: named.clone(),
                ..RecurringPatch::default()
            },
        },
        Command::ArchiveRecurring { recurring_id: id },
        Command::RestoreRecurring { recurring_id: id },
        Command::ExecuteRecurring {
            recurring_id: id,
            period_date: date(2023, 11, 1),
            occurred_at: when,
            person: named,
        },
        Command::SkipRecurring {
            recurring_id: id,
            period_date: date(2023, 11, 1),
        },
    ]
}

/// The kinds that may name somebody.
const NAMING: [&str; 7] = [
    "income",
    "expense",
    "refund",
    "update_transaction",
    "create_recurring",
    "update_recurring",
    "execute_recurring",
];

#[test]
fn every_kind_of_command_says_whom_it_names() {
    let commands = every_command(Some(" elisa "));
    let mut kinds: Vec<&str> = commands.iter().map(Command::kind_name).collect();
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(kinds.len(), 31, "one command of every kind");

    for command in &commands {
        let expected: Vec<&str> = if NAMING.contains(&command.kind_name()) {
            vec!["elisa"]
        } else {
            Vec::new()
        };
        assert_eq!(command.named_people(), expected, "{}", command.kind_name());
    }
    // Nobody named, or a blank name, is the author: nobody to check.
    for name in [None, Some(""), Some("  ")] {
        for command in every_command(name) {
            assert!(command.named_people().is_empty(), "{}", command.kind_name());
        }
    }
}

#[test]
fn renaming_a_person_touches_only_that_name() {
    for mut command in every_command(Some("local")) {
        let names = !command.named_people().is_empty();
        assert!(!command.rename_person("elisa", "alice"));
        assert_eq!(command.rename_person("local", "alice"), names);
        let expected: Vec<&str> = if names { vec!["alice"] } else { Vec::new() };
        assert_eq!(command.named_people(), expected, "{}", command.kind_name());
    }
    // A blank person follows the author by itself and stays blank.
    for mut command in every_command(Some(" ")) {
        assert!(!command.rename_person("", "alice"));
        assert!(!command.rename_person(" ", "alice"));
        assert!(command.named_people().is_empty());
    }
}
