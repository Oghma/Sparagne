//! Development fixture: a year of a two-person household, so the app has
//! something to show while it is being designed or tried out.
//!
//! ```text
//! cargo run -p sparagne_core --example seed -- <db path> [--replace]
//! ```
//!
//! The year is the twelve months that end with the current one, up to now,
//! so the app opens on a month with rows in it whenever the file is written.
//! The vault starts the way the app makes one, with the Italian categories
//! and aliases of `apple/Sparagne/Sparagne/Model/DefaultCategories.swift`,
//! then holds two wallets, three envelopes (one of them a capped fund),
//! salaries, a household's expenses, transfers between wallets and between
//! envelopes, refunds, rows without a category, a voided row and three
//! recurring templates: the mortgage has this month's payment still due, and
//! the gym was cancelled half way.
//!
//! Who a row is for is not always who recorded it: matteo records a few of
//! elisa's expenses for her, the gym is elisa's although matteo set it up,
//! and the mortgage stays matteo's in the months elisa records it.
//!
//! It writes real commands through the real core, so the log and the
//! projection are exactly what the app would have produced. `--replace` is
//! required to overwrite an existing database, and it deletes it outright.
//!
//! The rows are signed by two people, matteo and elisa, which a server would
//! refuse from one account: open the file with the app's `-SparagneDatabase`
//! launch option, which never syncs (`docs/v2/UI.md`).

// A fixture may panic on anything: there is no caller to report to.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::{DateTime, Datelike, FixedOffset, Local, Months, NaiveDate, TimeZone};
use sparagne_core::{
    Command, CommandEnvelope, Core, Currency, Entry, FlowMode, Frequency, Schedule, TransactionKind,
};
use uuid::Uuid;

/// The list a vault made by the app in Italian starts with. A copy: the core
/// knows nothing of the app's list, and this file only has to look like it.
const CATEGORIES: [(&str, &[&str]); 16] = [
    ("Spesa", &["supermercato", "alimentari"]),
    ("Casa", &["affitto", "mutuo"]),
    ("Bollette", &["luce", "gas", "internet"]),
    ("Trasporti", &["benzina", "treno", "auto"]),
    ("Ristoranti", &["bar", "caffè", "pizza"]),
    ("Salute", &["farmacia", "medico"]),
    ("Abbigliamento", &["vestiti", "scarpe"]),
    ("Svago", &["cinema", "hobby"]),
    ("Viaggi", &["vacanze", "hotel"]),
    ("Abbonamenti", &["streaming", "palestra"]),
    ("Istruzione", &["libri", "corsi"]),
    ("Tasse", &["imposte", "multe"]),
    ("Assicurazioni", &[]),
    ("Regali", &["donazioni"]),
    ("Stipendio", &[]),
    ("Interessi", &[]),
];

/// xorshift64*: amounts that vary from row to row, the same on every run.
struct Rng(u64);

impl Rng {
    fn step(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// `low..=high`.
    fn between(&mut self, low: i64, high: i64) -> i64 {
        let span = u64::try_from(high - low + 1).unwrap();
        low + i64::try_from(self.step() % span).unwrap()
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        let index = usize::try_from(self.step() % items.len() as u64).unwrap();
        items[index]
    }
}

/// One entry to write: who, when, how much, where from and under what, and
/// for whom when that is not who writes it.
struct Row<'a> {
    who: &'a str,
    person: Option<&'a str>,
    day: NaiveDate,
    amount: i64,
    wallet: Uuid,
    flow: Uuid,
    category: Option<&'a str>,
    note: &'a str,
}

struct Seeder {
    core: Core,
    vault: Uuid,
    now: DateTime<FixedOffset>,
    rng: Rng,
}

/// `hour` o'clock on `day` in the system timezone, with the offset of that
/// day (summer time included).
fn at(day: NaiveDate, hour: u32) -> DateTime<FixedOffset> {
    Local
        .from_local_datetime(&day.and_hms_opt(hour, 0, 0).unwrap())
        .earliest()
        .unwrap()
        .fixed_offset()
}

impl Seeder {
    fn run(&mut self, who: &str, command: Command) -> Uuid {
        self.core
            .execute(CommandEnvelope::new(self.vault, who, command))
            .unwrap_or_else(|e| panic!("{e}"))
            .result_id
            .unwrap_or_default()
    }

    /// Writes `row` as `kind`, unless it lies in the future. Returns the
    /// transaction's id when written.
    fn entry(&mut self, kind: TransactionKind, row: &Row<'_>) -> Option<Uuid> {
        let occurred_at = at(row.day, 10);
        if occurred_at > self.now {
            return None;
        }
        let entry = Entry {
            amount: row.amount,
            wallet_id: Some(row.wallet),
            flow_id: Some(row.flow),
            category: row.category.map(str::to_string),
            note: Some(row.note.to_string()),
            occurred_at,
            person: row.person.map(str::to_string),
        };
        let command = match kind {
            TransactionKind::Income => Command::Income(entry),
            TransactionKind::Refund => Command::Refund(entry),
            _ => Command::Expense(entry),
        };
        Some(self.run(row.who, command))
    }

    fn expense(&mut self, row: &Row<'_>) -> Option<Uuid> {
        self.entry(TransactionKind::Expense, row)
    }

    fn transfer_wallet(
        &mut self,
        who: &str,
        day: NaiveDate,
        amount: i64,
        from: Uuid,
        to: Uuid,
        note: &str,
    ) {
        let occurred_at = at(day, 8);
        if occurred_at > self.now {
            return;
        }
        self.run(
            who,
            Command::TransferWallet {
                amount,
                from_wallet_id: from,
                to_wallet_id: to,
                note: Some(note.to_string()),
                occurred_at,
            },
        );
    }

    fn transfer_flow(&mut self, day: NaiveDate, amount: i64, from: Uuid, to: Uuid, note: &str) {
        let occurred_at = at(day, 8);
        if occurred_at > self.now {
            return;
        }
        self.run(
            "matteo",
            Command::TransferFlow {
                amount,
                from_flow_id: from,
                to_flow_id: to,
                note: Some(note.to_string()),
                occurred_at,
            },
        );
    }

    /// A monthly expense template from `start`, `row.person`'s when it names
    /// one, else its creator's.
    fn monthly(&mut self, who: &str, row: &Row<'_>, start: NaiveDate) -> Uuid {
        self.run(
            who,
            Command::CreateRecurring {
                transaction_kind: TransactionKind::Expense,
                amount: row.amount,
                wallet_id: Some(row.wallet),
                flow_id: Some(row.flow),
                category: row.category.map(str::to_string),
                note: Some(row.note.to_string()),
                schedule: Schedule {
                    frequency: Frequency::Monthly {
                        day: u8::try_from(row.day.day()).unwrap(),
                    },
                    interval: 1,
                    start_date: start,
                    end_date: None,
                },
                owner: row.person.map(str::to_string),
            },
        )
    }

    /// Confirms the period of `template` that falls on `day`, when it has
    /// come, for `owner`: the app always sends the template's owner, whoever
    /// presses the button.
    fn execute(&mut self, who: &str, template: Uuid, owner: &str, day: NaiveDate) {
        let occurred_at = at(day, 8);
        if occurred_at > self.now {
            return;
        }
        self.run(
            who,
            Command::ExecuteRecurring {
                recurring_id: template,
                period_date: day,
                occurred_at,
                person: Some(owner.to_string()),
            },
        );
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

    let now = Local::now().fixed_offset();
    let this_month = now.date_naive().with_day(1).unwrap();
    let first_month = this_month - Months::new(11);
    let mut seed = Seeder {
        core,
        vault,
        now,
        rng: Rng(0x5EED_CA5A_2026_0001),
    };

    for (name, aliases) in CATEGORIES {
        let category = seed.run("matteo", Command::CreateCategory { name: name.into() });
        for alias in aliases {
            seed.run(
                "matteo",
                Command::AddAlias {
                    category_id: category,
                    alias: (*alias).into(),
                },
            );
        }
    }

    let eve = at(first_month.pred_opt().unwrap(), 9);
    let wallet = |seed: &mut Seeder, name: &str, opening_balance: i64| {
        seed.run(
            "matteo",
            Command::CreateWallet {
                name: name.into(),
                opening_balance,
                occurred_at: eve,
            },
        )
    };
    let conto = wallet(&mut seed, "Conto", 400_000);
    let contanti = wallet(&mut seed, "Contanti", 12_000);

    let envelope = |seed: &mut Seeder, name: &str, mode: FlowMode, opening_allocation: i64| {
        seed.run(
            "matteo",
            Command::CreateFlow {
                name: name.into(),
                mode,
                allow_negative: true,
                opening_allocation,
                occurred_at: eve,
            },
        )
    };
    let cash = envelope(&mut seed, "Cash", FlowMode::Unlimited, 0);
    let casa = envelope(&mut seed, "Casa", FlowMode::Unlimited, 0);
    // Started with 600.00, so the summer holiday fits whichever month the
    // year begins with.
    let vacanze = envelope(
        &mut seed,
        "Vacanze",
        FlowMode::NetCapped { cap: 300_000 },
        60_000,
    );

    let row = |who, day, amount, wallet, flow, category, note| Row {
        who,
        person: None,
        day,
        amount,
        wallet,
        flow,
        category,
        note,
    };

    let mutuo = seed.monthly(
        "matteo",
        &row(
            "matteo",
            first_month,
            78_000,
            conto,
            casa,
            Some("Casa"),
            "mutuo",
        ),
        first_month,
    );
    let streaming = seed.monthly(
        "matteo",
        &row(
            "matteo",
            first_month.with_day(12).unwrap(),
            1_299,
            conto,
            cash,
            Some("Abbonamenti"),
            "Netflix",
        ),
        first_month,
    );
    // matteo set elisa's gym up for her: the template is hers.
    let palestra = seed.monthly(
        "matteo",
        &Row {
            person: Some("elisa"),
            ..row(
                "matteo",
                first_month.with_day(5).unwrap(),
                4_500,
                conto,
                cash,
                Some("Abbonamenti"),
                "palestra",
            )
        },
        first_month,
    );

    // The dentist's refund comes the month after the bill, when there was one.
    let mut dentist_paid = false;
    for index in 0..12 {
        let start = first_month + Months::new(index);
        let day = |n: u32| start.with_day(n).unwrap();
        let month = start.month();
        let current = start == this_month;

        // Salaries on the first, the thirteenth in December.
        let salary = 235_000 + seed.rng.between(-2_000, 4_000);
        seed.entry(
            TransactionKind::Income,
            &row(
                "matteo",
                day(1),
                salary,
                conto,
                cash,
                Some("Stipendio"),
                "stipendio",
            ),
        );
        seed.entry(
            TransactionKind::Income,
            &row(
                "elisa",
                day(1),
                172_000,
                conto,
                cash,
                Some("Stipendio"),
                "stipendio",
            ),
        );
        if month == 12 {
            seed.entry(
                TransactionKind::Income,
                &row(
                    "matteo",
                    day(15),
                    210_000,
                    conto,
                    cash,
                    Some("Stipendio"),
                    "tredicesima",
                ),
            );
            seed.entry(
                TransactionKind::Income,
                &row(
                    "elisa",
                    day(15),
                    155_000,
                    conto,
                    cash,
                    Some("Stipendio"),
                    "tredicesima",
                ),
            );
        }
        if month.is_multiple_of(3) {
            let interest = seed.rng.between(180, 420);
            seed.entry(
                TransactionKind::Income,
                &row(
                    "matteo",
                    day(28),
                    interest,
                    conto,
                    cash,
                    Some("Interessi"),
                    "interessi conto",
                ),
            );
        }

        // What the envelopes and the cash wallet get every month.
        seed.transfer_flow(day(2), 100_000, cash, casa, "quota casa");
        seed.transfer_flow(day(2), 15_000, cash, vacanze, "accantonamento");
        seed.transfer_wallet("matteo", day(7), 10_000, conto, contanti, "prelievo");

        // The templates. This month's mortgage is left for the banner; elisa
        // records it every third month, and it stays matteo's.
        if !current {
            let who = if index % 3 == 2 { "elisa" } else { "matteo" };
            seed.execute(who, mutuo, "matteo", day(1));
        }
        seed.execute("matteo", streaming, "matteo", day(12));
        if index < 6 {
            seed.execute("elisa", palestra, "elisa", day(5));
        }

        // Groceries every week, the market and the bakery in cash.
        for (n, who) in [(3, "elisa"), (10, "matteo"), (17, "elisa"), (24, "matteo")] {
            let note = seed.rng.pick(&["Esselunga", "Coop", "Lidl", "Conad"]);
            let amount = seed.rng.between(4_500, 11_000);
            seed.expense(&row(who, day(n), amount, conto, cash, Some("Spesa"), note));
        }
        let market = seed.rng.between(1_500, 3_000);
        seed.expense(&row(
            "elisa",
            day(8),
            market,
            contanti,
            cash,
            Some("Spesa"),
            "mercato",
        ));
        let bread = seed.rng.between(400, 900);
        seed.expense(&row(
            "matteo",
            day(20),
            bread,
            contanti,
            cash,
            Some("alimentari"),
            "panetteria",
        ));

        // Eating out; "bar" is an alias of Ristoranti.
        for (n, who) in [(6, "matteo"), (14, "elisa"), (22, "matteo")] {
            let note = seed
                .rng
                .pick(&["pizzeria", "sushi", "trattoria", "aperitivo"]);
            let amount = seed.rng.between(2_500, 6_500);
            seed.expense(&row(
                who,
                day(n),
                amount,
                conto,
                cash,
                Some("Ristoranti"),
                note,
            ));
        }
        let coffee = seed.rng.between(350, 900);
        seed.expense(&row(
            "elisa",
            day(11),
            coffee,
            contanti,
            cash,
            Some("bar"),
            "colazione",
        ));

        // Fuel twice a month.
        for n in [9, 23] {
            let amount = seed.rng.between(5_500, 7_000);
            seed.expense(&row(
                "matteo",
                day(n),
                amount,
                conto,
                cash,
                Some("benzina"),
                "benzina",
            ));
        }

        // Bills, out of the house envelope: gas follows the season.
        let winter = matches!(month, 11 | 12 | 1 | 2 | 3);
        let gas = if winter {
            seed.rng.between(9_000, 15_000)
        } else {
            seed.rng.between(2_500, 4_000)
        };
        seed.expense(&row(
            "elisa",
            day(18),
            gas,
            conto,
            casa,
            Some("gas"),
            "bolletta gas",
        ));
        if month.is_multiple_of(2) {
            let power = seed.rng.between(7_000, 11_000);
            seed.expense(&row(
                "elisa",
                day(18),
                power,
                conto,
                casa,
                Some("luce"),
                "bolletta luce",
            ));
        }
        seed.expense(&row(
            "matteo",
            day(20),
            2_790,
            conto,
            casa,
            Some("internet"),
            "fibra",
        ));

        // The rest of a month.
        if index % 2 == 1 {
            let amount = seed.rng.between(800, 3_500);
            seed.expense(&row(
                "elisa",
                day(13),
                amount,
                conto,
                cash,
                Some("farmacia"),
                "farmacia",
            ));
        }
        let cinema = 1_800;
        seed.expense(&row(
            "elisa",
            day(26),
            cinema,
            conto,
            cash,
            Some("cinema"),
            "cinema",
        ));
        if index % 3 == 2 {
            let amount = seed.rng.between(1_500, 3_500);
            seed.expense(&row(
                "matteo",
                day(16),
                amount,
                conto,
                cash,
                Some("libri"),
                "libri",
            ));
        }

        // What happens once a year, on its calendar month.
        match month {
            4 => {
                seed.expense(&row(
                    "matteo",
                    day(15),
                    24_140,
                    conto,
                    cash,
                    Some("Tasse"),
                    "bollo auto",
                ));
            }
            5 => {
                seed.expense(&row(
                    "matteo",
                    day(10),
                    73_993,
                    conto,
                    cash,
                    Some("Assicurazioni"),
                    "assicurazione auto",
                ));
                seed.expense(&row(
                    "matteo",
                    day(21),
                    4_200,
                    conto,
                    cash,
                    Some("multe"),
                    "multa divieto di sosta",
                ));
            }
            3 | 7 | 10 => {
                let amount = seed.rng.between(4_000, 12_000);
                seed.expense(&row(
                    "elisa",
                    day(19),
                    amount,
                    conto,
                    cash,
                    Some("Abbigliamento"),
                    "Zalando",
                ));
                if month == 7 {
                    seed.entry(
                        TransactionKind::Refund,
                        &row(
                            "elisa",
                            day(27),
                            3_990,
                            conto,
                            cash,
                            Some("Abbigliamento"),
                            "reso Zalando",
                        ),
                    );
                }
            }
            8 => {
                seed.expense(&row(
                    "matteo",
                    day(4),
                    28_000,
                    conto,
                    vacanze,
                    Some("Viaggi"),
                    "volo",
                ));
                seed.expense(&row(
                    "elisa",
                    day(5),
                    64_000,
                    conto,
                    vacanze,
                    Some("hotel"),
                    "hotel",
                ));
                seed.expense(&row(
                    "matteo",
                    day(9),
                    5_500,
                    conto,
                    cash,
                    Some("Svago"),
                    "concerto",
                ));
            }
            9 => {
                dentist_paid = seed
                    .expense(&row(
                        "matteo",
                        day(8),
                        12_000,
                        conto,
                        cash,
                        Some("medico"),
                        "dentista",
                    ))
                    .is_some();
            }
            12 => {
                for (n, who) in [(12, "elisa"), (18, "matteo"), (21, "elisa")] {
                    let amount = seed.rng.between(3_000, 9_000);
                    seed.expense(&row(
                        who,
                        day(n),
                        amount,
                        conto,
                        cash,
                        Some("Regali"),
                        "regali di Natale",
                    ));
                }
                seed.expense(&row(
                    "matteo",
                    day(22),
                    12_000,
                    conto,
                    vacanze,
                    Some("Viaggi"),
                    "treno per Natale",
                ));
            }
            _ => {}
        }
        if month == 10 && dentist_paid {
            seed.entry(
                TransactionKind::Refund,
                &row(
                    "matteo",
                    day(2),
                    6_000,
                    conto,
                    cash,
                    Some("Salute"),
                    "rimborso assicurazione dentista",
                ),
            );
        }

        // elisa's, recorded by matteo, who paid for them. Fixed amounts, so
        // the rest of the year keeps its numbers.
        if index % 3 == 1 {
            seed.expense(&Row {
                person: Some("elisa"),
                ..row(
                    "matteo",
                    day(25),
                    3_500,
                    conto,
                    cash,
                    Some("Svago"),
                    "parrucchiere",
                )
            });
        }
        if index % 4 == 2 {
            seed.expense(&Row {
                person: Some("elisa"),
                ..row(
                    "matteo",
                    day(27),
                    9_000,
                    conto,
                    cash,
                    Some("medico"),
                    "visita oculistica",
                )
            });
        }

        // Rows nobody filed, and a charge that came twice.
        if index % 4 == 1 {
            let amount = seed.rng.between(1_500, 4_500);
            seed.expense(&row("matteo", day(15), amount, conto, cash, None, "Amazon"));
        }
        if index == 9 {
            seed.expense(&row(
                "matteo",
                day(14),
                3_200,
                conto,
                cash,
                Some("Ristoranti"),
                "cena",
            ));
            if let Some(twice) = seed.expense(&row(
                "matteo",
                day(14),
                3_200,
                conto,
                cash,
                Some("Ristoranti"),
                "cena",
            )) {
                seed.run(
                    "matteo",
                    Command::VoidTransaction {
                        transaction_id: twice,
                    },
                );
            }
        }
    }

    // The gym was cancelled after six months.
    seed.run(
        "elisa",
        Command::ArchiveRecurring {
            recurring_id: palestra,
        },
    );

    println!("seeded {path}");
}
