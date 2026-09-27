//! Development fixture: a year of a two-person household, so the ledger has
//! something to draw while it is being designed (`docs/v2/UI.md`).
//!
//! ```text
//! cargo run -p sparagne_core --example seed -- <db path> [--replace]
//! ```
//!
//! It writes real commands through the real core, so the log and the
//! projection are exactly what the app would have produced. `--replace` is
//! required to overwrite an existing database, and it deletes it outright.

// A fixture may panic on anything: there is no caller to report to.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::{FixedOffset, TimeZone};
use sparagne_core::{Command, CommandEnvelope, Core, Currency, Entry, FlowMode};
use uuid::Uuid;

fn at(y: i32, m: u32, d: u32, h: u32) -> chrono::DateTime<FixedOffset> {
    FixedOffset::east_opt(7200)
        .unwrap()
        .with_ymd_and_hms(y, m, d, h, 0, 0)
        .unwrap()
}

fn run(core: &mut Core, vault: Uuid, who: &str, cmd: Command) -> Uuid {
    core.execute(CommandEnvelope::new(vault, who, cmd))
        .unwrap_or_else(|e| panic!("{e}"))
        .result_id
        .unwrap_or_default()
}

fn entry(
    amount: i64,
    wallet: Uuid,
    flow: Uuid,
    cat: &str,
    note: &str,
    when: chrono::DateTime<FixedOffset>,
) -> Entry {
    Entry {
        amount,
        wallet_id: Some(wallet),
        flow_id: Some(flow),
        category: Some(cat.to_string()),
        note: Some(note.to_string()),
        occurred_at: when,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let replace = args.iter().any(|a| a == "--replace");
    let path = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .expect("usage: seed <db path> [--replace]")
        .clone();

    if std::fs::exists(&path).unwrap_or(false) {
        assert!(
            replace,
            "{path} already exists; pass --replace to delete it and start over"
        );
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{path}{suffix}"));
        }
    }
    let mut core = Core::open(&path).unwrap();

    let vault = core
        .execute(CommandEnvelope::create_vault(
            "matteo",
            "Casa",
            Currency::Eur,
        ))
        .unwrap()
        .result_id
        .unwrap();

    let wallet = run(
        &mut core,
        vault,
        "matteo",
        Command::CreateWallet {
            name: "Conto".into(),
            opening_balance: 500_000,
            occurred_at: at(2025, 12, 31, 8),
        },
    );

    let flow = |core: &mut Core, name: &str| {
        run(
            core,
            vault,
            "matteo",
            Command::CreateFlow {
                name: name.into(),
                mode: FlowMode::Unlimited,
                allow_negative: true,
                opening_allocation: 0,
                occurred_at: at(2025, 12, 31, 9),
            },
        )
    };
    let cash = flow(&mut core, "Cash");
    let casa = flow(&mut core, "Casa");
    let varie = flow(&mut core, "Varie");
    let investimenti = flow(&mut core, "Investimenti");

    // Nine months of salaries, so the twelve-month strip has a shape.
    let months: [(u32, i64, i64); 9] = [
        (1, 720_000, 380_000),
        (2, 731_000, 392_000),
        (3, 745_000, 300_000),
        (4, 738_000, 415_000),
        (5, 752_000, 288_000),
        (6, 741_000, 460_000),
        (7, 746_229, 402_000),
        (8, 746_229, 271_708),
        (9, 746_229, 120_000),
    ];
    for (month, matteo_income, matteo_spend) in months {
        run(
            &mut core,
            vault,
            "matteo",
            Command::Income(entry(
                matteo_income,
                wallet,
                cash,
                "Stipendio",
                "stipendio",
                at(2026, month, 1, 9),
            )),
        );
        run(
            &mut core,
            vault,
            "elisa",
            Command::Income(entry(
                394_500,
                wallet,
                cash,
                "Stipendio",
                "stipendio",
                at(2026, month, 1, 9),
            )),
        );
        if month != 8 {
            run(
                &mut core,
                vault,
                "matteo",
                Command::Expense(entry(
                    matteo_spend,
                    wallet,
                    cash,
                    "Casa",
                    "spese del mese",
                    at(2026, month, 15, 12),
                )),
            );
            run(
                &mut core,
                vault,
                "elisa",
                Command::Expense(entry(
                    35_400,
                    wallet,
                    cash,
                    "Spesa",
                    "spesa",
                    at(2026, month, 16, 12),
                )),
            );
        }
    }

    // August in detail: the rows of the mockup.
    let august: [(u32, Uuid, &str, &str, &str, i64); 22] = [
        (1, cash, "Casa", "mutuo", "matteo", 95_000),
        (2, cash, "Computer", "Claude", "matteo", 7_433),
        (2, cash, "Svago", "Abbonamento Ilsole24ore", "matteo", 4_900),
        (3, cash, "Casa", "LeroyMerlin", "matteo", 1_497),
        (3, cash, "Spesa", "panetteria", "elisa", 720),
        (4, varie, "Svago", "iPhone Elisa", "matteo", 46_553),
        (4, cash, "Auto", "benzina", "matteo", 6_890),
        (5, cash, "Casa", "Bolletta acqua", "matteo", 9_409),
        (5, cash, "Svago", "cinema", "elisa", 1_800),
        (6, cash, "Spesa", "Ali", "matteo", 2_673),
        (6, cash, "Salute", "farmacia", "elisa", 2_345),
        (7, cash, "Spesa", "Verdura", "elisa", 1_250),
        (7, cash, "Spesa", "Coop", "matteo", 4_110),
        (8, cash, "Spesa", "Formaggio", "elisa", 880),
        (9, casa, "Casa", "Cabina Armadio", "matteo", 185_000),
        (9, cash, "Casa", "detersivi", "elisa", 1_430),
        (10, casa, "Casa", "Asciugatrice", "matteo", 40_632),
        (11, cash, "Casa", "Bollo Revolut", "matteo", 780),
        (11, cash, "Svago", "ristorante", "matteo", 8_600),
        (12, cash, "Casa", "Bolletta luce", "elisa", 9_000),
        (12, cash, "Auto", "telepass", "matteo", 1_240),
        (14, cash, "Abiti", "scarpe", "elisa", 9_459),
    ];
    for (day, envelope, category, note, who, amount) in august {
        run(
            &mut core,
            vault,
            who,
            Command::Expense(entry(
                amount,
                wallet,
                envelope,
                category,
                note,
                at(2026, 8, day, 10),
            )),
        );
    }
    run(
        &mut core,
        vault,
        "matteo",
        Command::Expense(entry(
            73_993,
            wallet,
            varie,
            "Auto",
            "assicurazione",
            at(2026, 8, 18, 10),
        )),
    );
    run(
        &mut core,
        vault,
        "matteo",
        Command::Expense(entry(
            24_140,
            wallet,
            varie,
            "Auto",
            "bollo",
            at(2026, 8, 20, 10),
        )),
    );
    run(
        &mut core,
        vault,
        "matteo",
        Command::Refund(entry(
            3_000,
            wallet,
            cash,
            "Spesa",
            "reso Coop",
            at(2026, 8, 22, 10),
        )),
    );
    run(
        &mut core,
        vault,
        "matteo",
        Command::Expense(entry(
            100_000,
            wallet,
            investimenti,
            "Investimenti",
            "PAC ETF",
            at(2026, 8, 25, 10),
        )),
    );

    // September, so the app opens on a month with rows in it.
    for (day, category, note, who, amount) in [
        (1u32, "Casa", "mutuo", "matteo", 95_000i64),
        (2, "Spesa", "Coop", "elisa", 6_240),
        (4, "Auto", "benzina", "matteo", 7_120),
        (6, "Svago", "concerto", "elisa", 4_400),
        (8, "Salute", "dentista", "matteo", 12_000),
    ] {
        run(
            &mut core,
            vault,
            who,
            Command::Expense(entry(
                amount,
                wallet,
                cash,
                category,
                note,
                at(2026, 9, day, 10),
            )),
        );
    }

    println!("seeded {path}");
}
