//! The vault's own life cycle: `RenameVault` and `DeleteVault`
//! (`docs/v2/ARCH.md` §4). Creation is covered in `core.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::NaiveDate;
use common::{T0, all, entry, flow_cmd, run, setup, try_run, wallet_cmd};
use sparagne_core::{
    Command, CommandEnvelope, Core, Currency, DomainError, FlowMode, Frequency, Schedule,
    TransactionKind, replay,
};
use uuid::Uuid;

fn rename(name: &str) -> Command {
    Command::RenameVault {
        name: name.to_string(),
    }
}

fn create_vault(core: &mut Core, author: &str, name: &str) -> Uuid {
    core.execute(CommandEnvelope::create_vault(author, name, Currency::Eur))
        .unwrap()
        .result_id
        .unwrap()
}

/// A monthly expense template, so the cascade has a `recurring_templates`
/// row (and a run) to take with it.
fn rent_cmd() -> Command {
    Command::CreateRecurring {
        transaction_kind: TransactionKind::Expense,
        amount: 2_500,
        wallet_id: None,
        flow_id: None,
        category: Some("Rent".to_string()),
        note: None,
        schedule: Schedule {
            frequency: Frequency::Monthly { day: 1 },
            interval: 1,
            start_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            end_date: None,
        },
    }
}

// ---------------------------------------------------------------------------
// Rename
// ---------------------------------------------------------------------------

#[test]
fn rename_vault_trims_and_shows_everywhere() {
    let mut fx = setup();
    run(&mut fx.core, fx.vault, rename("  Casa "));

    assert_eq!(fx.core.vault(fx.vault).unwrap().unwrap().name, "Casa");
    assert_eq!(fx.core.snapshot(fx.vault).unwrap().name, "Casa");
    assert_eq!(fx.core.vaults().unwrap()[0].name, "Casa");
    // The owner and the currency are untouched.
    let view = fx.core.vault(fx.vault).unwrap().unwrap();
    assert_eq!(view.owner, "alice");
    assert_eq!(view.currency, Currency::Eur);

    assert!(matches!(
        try_run(&mut fx.core, fx.vault, rename("   ")),
        Err(DomainError::InvalidName(_))
    ));
    assert_eq!(
        try_run(&mut fx.core, Uuid::now_v7(), rename("Other")).unwrap_err(),
        DomainError::NotFound("vault".to_string())
    );
}

#[test]
fn rename_vault_keeps_names_unique_per_owner() {
    let mut fx = setup();
    let lavoro = create_vault(&mut fx.core, "alice", "Lavoro");
    // Bob's "Casa" lives next to alice's vaults in the same database, as on
    // the server: it never gets in alice's way.
    create_vault(&mut fx.core, "bob", "Casa");

    assert_eq!(
        try_run(&mut fx.core, lavoro, rename("main")).unwrap_err(),
        DomainError::AlreadyExists("main".to_string())
    );
    run(&mut fx.core, fx.vault, rename("Casa"));
    // Renaming to one's own name (a different case) is fine.
    run(&mut fx.core, fx.vault, rename("CASA"));
    assert_eq!(fx.core.vault(fx.vault).unwrap().unwrap().name, "CASA");
}

#[test]
fn an_editor_may_rename_and_the_check_still_runs_against_the_owner() {
    let mut fx = setup();
    create_vault(&mut fx.core, "alice", "Lavoro");

    // bob has no vault called Lavoro, but alice does: the name belongs to
    // the owner's namespace, not the author's.
    let err = fx
        .core
        .execute(CommandEnvelope::new(fx.vault, "bob", rename("Lavoro")))
        .unwrap_err();
    assert_eq!(err, DomainError::AlreadyExists("Lavoro".to_string()));

    fx.core
        .execute(CommandEnvelope::new(fx.vault, "bob", rename("Casa")))
        .unwrap();
    let view = fx.core.vault(fx.vault).unwrap().unwrap();
    assert_eq!(view.name, "Casa");
    assert_eq!(view.owner, "alice", "renaming never changes the owner");
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

#[test]
fn delete_vault_drops_the_projection_and_keeps_the_log() {
    let mut fx = setup();
    let food = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Food", FlowMode::Unlimited, false, 0),
    )
    .result_id
    .unwrap();
    run(&mut fx.core, fx.vault, wallet_cmd("Bank", 10_000));
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(1_000, Some(fx.wallet), Some(food), Some("Tips"), T0)),
    );
    run(&mut fx.core, fx.vault, rent_cmd());
    let before = fx.core.last_seq(fx.vault).unwrap();
    assert_eq!(before, 6);

    let receipt = run(&mut fx.core, fx.vault, Command::DeleteVault);
    assert_eq!(receipt.seq, 7);
    assert_eq!(receipt.result_id, None);

    // Gone from every read of the projection.
    assert_eq!(fx.core.vault(fx.vault).unwrap(), None);
    assert!(fx.core.vaults().unwrap().is_empty());
    assert_eq!(
        fx.core.snapshot(fx.vault).unwrap_err(),
        DomainError::NotFound("vault".to_string())
    );
    assert!(fx.core.categories(fx.vault, true).unwrap().is_empty());
    assert!(fx.core.aliases(fx.vault).unwrap().is_empty());
    assert!(
        fx.core
            .list_transactions(fx.vault, &all(), 100, None)
            .unwrap()
            .items
            .is_empty()
    );
    assert!(fx.core.list_recurring(fx.vault, true).unwrap().is_empty());
    assert!(fx.core.authors(fx.vault).unwrap().is_empty());

    // The log is intact, the deletion is its last entry, and all of it is
    // still waiting for the server.
    let log = fx.core.commands_since(fx.vault, 0).unwrap();
    assert_eq!(log.len(), 7);
    assert_eq!(log[6].envelope.command, Command::DeleteVault);
    assert_eq!(fx.core.last_seq(fx.vault).unwrap(), 7);
    assert_eq!(fx.core.sync_state(fx.vault).unwrap().outbox, 7);
    assert_eq!(fx.core.deleted_vaults().unwrap(), vec![fx.vault]);
}

#[test]
fn nothing_applies_to_a_deleted_vault() {
    let mut fx = setup();
    run(&mut fx.core, fx.vault, Command::DeleteVault);
    let not_found = DomainError::NotFound("vault".to_string());

    assert_eq!(
        try_run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0)).unwrap_err(),
        not_found
    );
    assert_eq!(
        try_run(
            &mut fx.core,
            fx.vault,
            Command::Income(entry(500, Some(fx.wallet), None, None, T0)),
        )
        .unwrap_err(),
        not_found
    );
    assert_eq!(
        try_run(
            &mut fx.core,
            fx.vault,
            Command::RenameWallet {
                wallet_id: fx.wallet,
                name: "Bank".to_string(),
            },
        )
        .unwrap_err(),
        DomainError::NotFound("wallet".to_string())
    );
    assert_eq!(
        try_run(&mut fx.core, fx.vault, rename("Ghost")).unwrap_err(),
        not_found
    );
    assert_eq!(
        try_run(&mut fx.core, fx.vault, Command::DeleteVault).unwrap_err(),
        not_found
    );
    // The refusals never entered the log.
    assert_eq!(fx.core.last_seq(fx.vault).unwrap(), 3);
}

#[test]
fn only_the_owner_deletes() {
    let mut fx = setup();
    let err = fx
        .core
        .execute(CommandEnvelope::new(fx.vault, "bob", Command::DeleteVault))
        .unwrap_err();
    assert_eq!(err.code(), "forbidden");
    assert!(matches!(err, DomainError::Forbidden(message) if message.contains("alice")));
    assert!(fx.core.vault(fx.vault).unwrap().is_some());
    assert!(fx.core.deleted_vaults().unwrap().is_empty());

    run(&mut fx.core, fx.vault, Command::DeleteVault);
    assert!(fx.core.vault(fx.vault).unwrap().is_none());
}

#[test]
fn deleting_is_idempotent_by_command_id() {
    let mut fx = setup();
    let envelope = CommandEnvelope::new(fx.vault, "alice", Command::DeleteVault);
    let first = fx.core.execute(envelope.clone()).unwrap();
    let again = fx.core.execute(envelope).unwrap();
    assert!(again.deduplicated);
    assert_eq!(again.seq, first.seq);
    assert_eq!(fx.core.last_seq(fx.vault).unwrap(), first.seq);
}

#[test]
fn a_deleted_vaults_name_is_free_again() {
    let mut fx = setup();
    run(&mut fx.core, fx.vault, Command::DeleteVault);

    let reborn = create_vault(&mut fx.core, "alice", "Main");
    assert_ne!(reborn, fx.vault);
    let vaults = fx.core.vaults().unwrap();
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0].id, reborn);
    assert_eq!(vaults[0].name, "Main");
    // Only the dead one is reported as deleted.
    assert_eq!(fx.core.deleted_vaults().unwrap(), vec![fx.vault]);
}

#[test]
fn other_vaults_survive_a_deletion() {
    let mut fx = setup();
    let lavoro = create_vault(&mut fx.core, "alice", "Lavoro");
    let bank = run(&mut fx.core, lavoro, wallet_cmd("Bank", 4_200))
        .result_id
        .unwrap();

    run(&mut fx.core, fx.vault, Command::DeleteVault);

    let snapshot = fx.core.snapshot(lavoro).unwrap();
    assert_eq!(snapshot.name, "Lavoro");
    assert_eq!(snapshot.wallets.len(), 1);
    assert_eq!(snapshot.wallets[0].id, bank);
    assert_eq!(snapshot.wallets[0].balance, 4_200);
    assert_eq!(
        fx.core
            .list_transactions(lavoro, &all(), 100, None)
            .unwrap()
            .items
            .len(),
        1
    );
    assert_eq!(fx.core.vaults().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Replay
// ---------------------------------------------------------------------------

#[test]
fn replay_reproduces_a_rename_and_ends_on_a_deletion() {
    let mut fx = setup();
    run(&mut fx.core, fx.vault, rename("Casa"));
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(1_000, Some(fx.wallet), None, Some("Bar"), T0)),
    );
    let log = fx.core.commands_since(fx.vault, 0).unwrap();

    // Up to the rename: the same name and the same rows.
    let mut renamed = Core::open_in_memory().unwrap();
    replay(&log, &mut renamed).unwrap();
    assert_eq!(renamed.vault(fx.vault).unwrap().unwrap().name, "Casa");
    assert_eq!(
        renamed
            .list_transactions(fx.vault, &all(), 100, None)
            .unwrap()
            .items,
        fx.core
            .list_transactions(fx.vault, &all(), 100, None)
            .unwrap()
            .items
    );

    // With the deletion at the end: no vault, and the log agrees.
    run(&mut fx.core, fx.vault, Command::DeleteVault);
    let log = fx.core.commands_since(fx.vault, 0).unwrap();
    let mut deleted = Core::open_in_memory().unwrap();
    replay(&log, &mut deleted).unwrap();
    assert_eq!(deleted.vault(fx.vault).unwrap(), None);
    assert_eq!(deleted.last_seq(fx.vault).unwrap(), log.len() as i64);
    assert_eq!(deleted.deleted_vaults().unwrap(), vec![fx.vault]);
}
