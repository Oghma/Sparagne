//! Acceptance tests for wallet, flow and category management.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use sparagne_core::{
    Command, Core, DomainError, FlowMode, MergeConflictKind, TransactionFilter, replay,
};
use uuid::Uuid;

/// Id of the category with this display name, archived included.
fn category_id(core: &Core, vault: Uuid, name: &str) -> Uuid {
    core.categories(vault, true)
        .unwrap()
        .into_iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no category named '{name}'"))
        .id
}

fn category_names(core: &Core, vault: Uuid) -> Vec<String> {
    core.categories(vault, true)
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect()
}

fn alias_names(core: &Core, vault: Uuid) -> Vec<String> {
    core.aliases(vault)
        .unwrap()
        .into_iter()
        .map(|a| a.alias)
        .collect()
}

fn create_category(core: &mut Core, vault: Uuid, name: &str) -> Uuid {
    run(
        core,
        vault,
        Command::CreateCategory {
            name: name.to_string(),
        },
    )
    .result_id
    .unwrap()
}

fn flow_view(core: &Core, vault: Uuid, id: Uuid) -> sparagne_core::FlowView {
    core.snapshot(vault)
        .unwrap()
        .flows
        .into_iter()
        .find(|f| f.id == id)
        .unwrap()
}

fn wallet_view(core: &Core, vault: Uuid, id: Uuid) -> sparagne_core::WalletView {
    core.snapshot(vault)
        .unwrap()
        .wallets
        .into_iter()
        .find(|w| w.id == id)
        .unwrap()
}

// ---------------------------------------------------------------------------
// Categories: aliases, rename, archive
// ---------------------------------------------------------------------------

#[test]
fn alias_resolves_to_category() {
    let mut fx = setup();
    let spese = create_category(&mut fx.core, fx.vault, "Spese");
    let alias = run(
        &mut fx.core,
        fx.vault,
        Command::AddAlias {
            category_id: spese,
            alias: " SPESA ".to_string(),
        },
    );
    assert_eq!(alias.result_id, Some(alias.command_id));

    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(50, None, None, Some("spesa"), T0)),
    );
    let txs = list(&fx.core, fx.vault, &TransactionFilter::default());
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].category, "Spese");
    assert_eq!(txs[0].category_id, spese);
    assert_eq!(alias_names(&fx.core, fx.vault), vec!["SPESA".to_string()]);

    run(
        &mut fx.core,
        fx.vault,
        Command::RemoveAlias {
            category_id: spese,
            alias: "spesa!".to_string(),
        },
    );
    assert!(alias_names(&fx.core, fx.vault).is_empty());
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::RemoveAlias {
            category_id: spese,
            alias: "spesa".to_string(),
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::NotFound("alias".to_string()));
}

#[test]
fn alias_must_be_unique_across_names_and_aliases() {
    let mut fx = setup();
    let spese = create_category(&mut fx.core, fx.vault, "Spese");
    let food = create_category(&mut fx.core, fx.vault, "Food");
    run(
        &mut fx.core,
        fx.vault,
        Command::AddAlias {
            category_id: spese,
            alias: "spesa".to_string(),
        },
    );

    // Another category's name.
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::AddAlias {
            category_id: spese,
            alias: "Food".to_string(),
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::AlreadyExists("Food".to_string()));
    // An existing alias, on any category.
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::AddAlias {
            category_id: food,
            alias: "Spesa".to_string(),
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::AlreadyExists("Spesa".to_string()));
    // Its own name.
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::AddAlias {
            category_id: spese,
            alias: "spese".to_string(),
        },
    )
    .unwrap_err();
    assert!(matches!(err, DomainError::AlreadyExists(_)));
    // System categories take no aliases; blank aliases are refused.
    let uncategorized = category_id(&fx.core, fx.vault, "Uncategorized");
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::AddAlias {
            category_id: uncategorized,
            alias: "misc".to_string(),
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InvalidName("system category".to_string()));
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::AddAlias {
            category_id: food,
            alias: "   ".to_string(),
        },
    )
    .unwrap_err();
    assert!(matches!(err, DomainError::InvalidName(_)));
    assert_eq!(alias_names(&fx.core, fx.vault), vec!["spesa".to_string()]);
}

#[test]
fn alias_refused_on_archived_category() {
    let mut fx = setup();
    let travel = create_category(&mut fx.core, fx.vault, "Travel");
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveCategory {
            category_id: travel,
        },
    );
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::AddAlias {
            category_id: travel,
            alias: "trips".to_string(),
        },
    )
    .unwrap_err();
    assert_eq!(
        err,
        DomainError::InvalidName("category is archived".to_string())
    );
}

#[test]
fn rename_category_updates_transactions() {
    let mut fx = setup();
    let food = create_category(&mut fx.core, fx.vault, "Food");
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(1000, None, None, None, T0)),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(40, None, None, Some("Food"), T0 + 1)),
    );

    run(
        &mut fx.core,
        fx.vault,
        Command::RenameCategory {
            category_id: food,
            name: "  Groceries  ".to_string(),
        },
    );
    let txs = list(&fx.core, fx.vault, &TransactionFilter::default());
    let expense = txs.iter().find(|t| t.amount == 40).unwrap();
    assert_eq!(expense.category, "Groceries");
    assert_eq!(expense.category_id, food);
    // The old name is free again and creates a distinct category.
    let again = create_category(&mut fx.core, fx.vault, "Food");
    assert_ne!(again, food);
}

#[test]
fn rename_category_rejects_system_duplicates_and_reserved_names() {
    let mut fx = setup();
    let food = create_category(&mut fx.core, fx.vault, "Food");
    create_category(&mut fx.core, fx.vault, "Spese");
    let opening = category_id(&fx.core, fx.vault, "Opening");

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::RenameCategory {
            category_id: opening,
            name: "Start".to_string(),
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InvalidName("system category".to_string()));

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::RenameCategory {
            category_id: food,
            name: " spese ".to_string(),
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::AlreadyExists("spese".to_string()));

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::RenameCategory {
            category_id: food,
            name: "Uncategorized".to_string(),
        },
    )
    .unwrap_err();
    assert!(matches!(err, DomainError::InvalidName(_)));

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::RenameCategory {
            category_id: Uuid::now_v7(),
            name: "Whatever".to_string(),
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::NotFound("category".to_string()));

    // Renaming to its own current name is a no-op, not a conflict.
    run(
        &mut fx.core,
        fx.vault,
        Command::RenameCategory {
            category_id: food,
            name: "food".to_string(),
        },
    );
    assert!(category_names(&fx.core, fx.vault).contains(&"food".to_string()));
}

#[test]
fn rename_category_to_its_own_alias_drops_the_alias() {
    let mut fx = setup();
    let spese = create_category(&mut fx.core, fx.vault, "Spese");
    run(
        &mut fx.core,
        fx.vault,
        Command::AddAlias {
            category_id: spese,
            alias: "Spesa".to_string(),
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::RenameCategory {
            category_id: spese,
            name: "Spesa".to_string(),
        },
    );
    assert!(alias_names(&fx.core, fx.vault).is_empty());
    assert!(category_names(&fx.core, fx.vault).contains(&"Spesa".to_string()));
    // The name still resolves, now through the category itself.
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10, None, None, Some("spesa"), T0)),
    );
    let txs = list(&fx.core, fx.vault, &TransactionFilter::default());
    assert_eq!(txs[0].category_id, spese);
}

#[test]
fn archived_category_rejected_on_create() {
    let mut fx = setup();
    let travel = create_category(&mut fx.core, fx.vault, "Travel");
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveCategory {
            category_id: travel,
        },
    );
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10, None, None, Some("Travel"), T0)),
    )
    .unwrap_err();
    match err {
        DomainError::InvalidName(message) => assert!(message.contains("archived")),
        other => panic!("unexpected error: {other:?}"),
    }

    // Archiving twice, and restoring an active category, are refused.
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveCategory {
            category_id: travel,
        },
    )
    .unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));
    run(
        &mut fx.core,
        fx.vault,
        Command::RestoreCategory {
            category_id: travel,
        },
    );
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::RestoreCategory {
            category_id: travel,
        },
    )
    .unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10, None, None, Some("Travel"), T0)),
    );

    let uncategorized = category_id(&fx.core, fx.vault, "Uncategorized");
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveCategory {
            category_id: uncategorized,
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InvalidName("system category".to_string()));
}

// ---------------------------------------------------------------------------
// Categories: merge
// ---------------------------------------------------------------------------

#[test]
fn merge_category_moves_transactions_and_aliases() {
    let mut fx = setup();
    let food = create_category(&mut fx.core, fx.vault, "Food");
    let spese = create_category(&mut fx.core, fx.vault, "Spese");
    run(
        &mut fx.core,
        fx.vault,
        Command::AddAlias {
            category_id: spese,
            alias: "spesa".to_string(),
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(1000, None, None, None, T0)),
    );
    let expense = run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(12, None, None, Some("Food"), T0 + 1)),
    )
    .result_id
    .unwrap();

    assert!(fx.core.preview_merge(fx.vault, food, spese).unwrap().ok);
    run(
        &mut fx.core,
        fx.vault,
        Command::MergeCategory {
            source_id: food,
            target_id: spese,
        },
    );

    let txs = list(&fx.core, fx.vault, &TransactionFilter::default());
    let moved = txs.iter().find(|t| t.id == expense).unwrap();
    assert_eq!(moved.category_id, spese);
    assert_eq!(moved.category, "Spese");

    // Source gone, its aliases and its name now belong to the target.
    assert!(!category_names(&fx.core, fx.vault).contains(&"Food".to_string()));
    let aliases = fx.core.aliases(fx.vault).unwrap();
    assert_eq!(
        aliases.iter().map(|a| a.alias.as_str()).collect::<Vec<_>>(),
        vec!["Food", "spesa"]
    );
    assert!(aliases.iter().all(|a| a.category_id == spese));

    // Later use of the old name resolves to the target.
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(5, None, None, Some("food"), T0 + 2)),
    );
    let txs = list(&fx.core, fx.vault, &TransactionFilter::default());
    let later = txs.iter().find(|t| t.amount == 5).unwrap();
    assert_eq!(later.category_id, spese);
}

#[test]
fn merge_into_a_system_category_adds_no_alias() {
    let mut fx = setup();
    let food = create_category(&mut fx.core, fx.vault, "Food");
    let uncategorized = category_id(&fx.core, fx.vault, "Uncategorized");
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(30, None, None, Some("Food"), T0)),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::MergeCategory {
            source_id: food,
            target_id: uncategorized,
        },
    );
    let txs = list(&fx.core, fx.vault, &TransactionFilter::default());
    assert_eq!(txs[0].category_id, uncategorized);
    assert!(alias_names(&fx.core, fx.vault).is_empty());
    assert!(!category_names(&fx.core, fx.vault).contains(&"Food".to_string()));
}

#[test]
fn preview_merge_reports_conflicts() {
    let mut fx = setup();
    let food = create_category(&mut fx.core, fx.vault, "Food");
    let travel = create_category(&mut fx.core, fx.vault, "Travel");
    let opening = category_id(&fx.core, fx.vault, "Opening");
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveCategory {
            category_id: travel,
        },
    );

    let preview = fx.core.preview_merge(fx.vault, food, travel).unwrap();
    assert!(!preview.ok);
    assert_eq!(preview.conflicts.len(), 1);
    assert_eq!(preview.conflicts[0].kind, MergeConflictKind::TargetArchived);
    assert_eq!(preview.conflicts[0].value, "Travel");

    let preview = fx.core.preview_merge(fx.vault, food, food).unwrap();
    assert_eq!(preview.conflicts[0].kind, MergeConflictKind::SameCategory);
    let preview = fx.core.preview_merge(fx.vault, opening, food).unwrap();
    assert_eq!(preview.conflicts[0].kind, MergeConflictKind::SourceSystem);

    assert_eq!(
        fx.core
            .preview_merge(fx.vault, food, Uuid::now_v7())
            .unwrap_err(),
        DomainError::NotFound("category".to_string())
    );

    // The command refuses exactly what the preview reports.
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::MergeCategory {
            source_id: food,
            target_id: travel,
        },
    )
    .unwrap_err();
    match err {
        DomainError::InvalidCommand(message) => assert!(message.contains("target_archived")),
        other => panic!("unexpected error: {other:?}"),
    }
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::MergeCategory {
            source_id: opening,
            target_id: food,
        },
    )
    .unwrap_err();
    match err {
        DomainError::InvalidCommand(message) => assert!(message.contains("source_system")),
        other => panic!("unexpected error: {other:?}"),
    }
    // Nothing was touched.
    assert!(category_names(&fx.core, fx.vault).contains(&"Food".to_string()));
    assert!(category_names(&fx.core, fx.vault).contains(&"Opening".to_string()));
}

#[test]
fn similar_names_are_suggested_and_creation_is_never_blocked() {
    let mut fx = setup();
    create_category(&mut fx.core, fx.vault, "Spesa");

    let similar = fx.core.similar_categories(fx.vault, "spese").unwrap();
    assert_eq!(
        similar.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        vec!["Spesa"]
    );
    // An exact match is not a suggestion.
    assert!(
        fx.core
            .similar_categories(fx.vault, " SPESA ")
            .unwrap()
            .is_empty()
    );
    // Distance 2 is only allowed above six characters.
    assert!(fx.core.similar_categories(fx.vault, "spesw").unwrap().len() == 1);
    assert!(
        fx.core
            .similar_categories(fx.vault, "spwsw")
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        fx.core.similar_categories(fx.vault, "  ").unwrap_err(),
        DomainError::InvalidName(_)
    ));

    // v1 blocked this; v2 only suggests.
    let spese = create_category(&mut fx.core, fx.vault, "Spese");
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(50, None, None, Some("spese"), T0)),
    );
    let txs = list(&fx.core, fx.vault, &TransactionFilter::default());
    assert_eq!(txs[0].category_id, spese);

    // Archived and system categories are never suggested.
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveCategory { category_id: spese },
    );
    let similar = fx.core.similar_categories(fx.vault, "spesi").unwrap();
    assert_eq!(
        similar.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        vec!["Spesa"]
    );
    assert!(
        fx.core
            .similar_categories(fx.vault, "opening")
            .unwrap()
            .is_empty()
    );
}

// ---------------------------------------------------------------------------
// Wallets
// ---------------------------------------------------------------------------

#[test]
fn rename_wallet_trims_and_keeps_names_unique() {
    let mut fx = setup();
    let spare = run(&mut fx.core, fx.vault, wallet_cmd("Spare", 0))
        .result_id
        .unwrap();

    run(
        &mut fx.core,
        fx.vault,
        Command::RenameWallet {
            wallet_id: spare,
            name: "  Savings  ".to_string(),
        },
    );
    assert_eq!(wallet_view(&fx.core, fx.vault, spare).name, "Savings");

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::RenameWallet {
            wallet_id: spare,
            name: "cash".to_string(),
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::AlreadyExists("cash".to_string()));

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::RenameWallet {
            wallet_id: spare,
            name: "   ".to_string(),
        },
    )
    .unwrap_err();
    assert!(matches!(err, DomainError::InvalidName(_)));

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::RenameWallet {
            wallet_id: Uuid::now_v7(),
            name: "Ghost".to_string(),
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::NotFound("wallet".to_string()));

    // Renaming to its own name, in another case, is allowed.
    run(
        &mut fx.core,
        fx.vault,
        Command::RenameWallet {
            wallet_id: spare,
            name: "SAVINGS".to_string(),
        },
    );
    assert_eq!(wallet_view(&fx.core, fx.vault, spare).name, "SAVINGS");
}

#[test]
fn wallet_archive_requires_a_zero_balance_and_refuses_new_legs() {
    let mut fx = setup();
    let spare = run(&mut fx.core, fx.vault, wallet_cmd("Spare", 0))
        .result_id
        .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(1000, Some(fx.wallet), None, None, T0)),
    );

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveWallet {
            wallet_id: fx.wallet,
        },
    )
    .unwrap_err();
    assert_eq!(
        err,
        DomainError::InvalidCommand("wallet has a non-zero balance".to_string())
    );

    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveWallet { wallet_id: spare },
    );
    assert!(wallet_view(&fx.core, fx.vault, spare).archived);

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10, Some(spare), None, None, T0)),
    )
    .unwrap_err();
    assert_eq!(
        err,
        DomainError::InvalidCommand("wallet is archived".to_string())
    );

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveWallet { wallet_id: spare },
    )
    .unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));

    run(
        &mut fx.core,
        fx.vault,
        Command::RestoreWallet { wallet_id: spare },
    );
    assert!(!wallet_view(&fx.core, fx.vault, spare).archived);
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::RestoreWallet { wallet_id: spare },
    )
    .unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10, Some(spare), None, None, T0)),
    );
    assert_eq!(wallet_balance(&fx.core, fx.vault, spare), 10);
}

// ---------------------------------------------------------------------------
// Flows
// ---------------------------------------------------------------------------

/// A vault with 1000 in the wallet and `Trip` holding `allocation`.
fn with_flow(allocation: i64, mode: FlowMode, allow_negative: bool) -> (Fx, Uuid) {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(1000, None, None, None, T0)),
    );
    let flow = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Trip", mode, allow_negative, allocation),
    )
    .result_id
    .unwrap();
    (fx, flow)
}

#[test]
fn update_flow_renames_and_rejects_reserved_or_taken_names() {
    let (mut fx, trip) = with_flow(0, FlowMode::Unlimited, false);
    run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Rent", FlowMode::Unlimited, false, 0),
    );

    run(
        &mut fx.core,
        fx.vault,
        Command::UpdateFlow {
            flow_id: trip,
            name: Some("  Holiday  ".to_string()),
            mode: None,
            allow_negative: None,
        },
    );
    assert_eq!(flow_view(&fx.core, fx.vault, trip).name, "Holiday");

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::UpdateFlow {
            flow_id: trip,
            name: Some("rent".to_string()),
            mode: None,
            allow_negative: None,
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::AlreadyExists("rent".to_string()));

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::UpdateFlow {
            flow_id: trip,
            name: Some("Unallocated".to_string()),
            mode: None,
            allow_negative: None,
        },
    )
    .unwrap_err();
    assert_eq!(
        err,
        DomainError::InvalidFlow("flow name is reserved".to_string())
    );
}

#[test]
fn update_flow_requires_a_field_and_refuses_unallocated() {
    let (mut fx, trip) = with_flow(0, FlowMode::Unlimited, false);
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::UpdateFlow {
            flow_id: trip,
            name: None,
            mode: None,
            allow_negative: None,
        },
    )
    .unwrap_err();
    assert_eq!(
        err,
        DomainError::InvalidCommand("nothing to update".to_string())
    );

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::UpdateFlow {
            flow_id: fx.unallocated,
            name: Some("Pot".to_string()),
            mode: None,
            allow_negative: None,
        },
    )
    .unwrap_err();
    assert!(matches!(err, DomainError::InvalidFlow(_)));

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::UpdateFlow {
            flow_id: Uuid::now_v7(),
            name: Some("Pot".to_string()),
            mode: None,
            allow_negative: None,
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::NotFound("flow".to_string()));
}

#[test]
fn update_flow_cap_must_hold_for_the_current_balance() {
    let (mut fx, trip) = with_flow(500, FlowMode::Unlimited, false);

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::UpdateFlow {
            flow_id: trip,
            name: None,
            mode: Some(FlowMode::NetCapped { cap: 400 }),
            allow_negative: None,
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::MaxBalanceReached("Trip".to_string()));
    assert_eq!(
        flow_view(&fx.core, fx.vault, trip).mode,
        FlowMode::Unlimited
    );

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::UpdateFlow {
            flow_id: trip,
            name: None,
            mode: Some(FlowMode::NetCapped { cap: 0 }),
            allow_negative: None,
        },
    )
    .unwrap_err();
    assert!(matches!(err, DomainError::InvalidFlow(_)));

    run(
        &mut fx.core,
        fx.vault,
        Command::UpdateFlow {
            flow_id: trip,
            name: None,
            mode: Some(FlowMode::NetCapped { cap: 600 }),
            allow_negative: None,
        },
    );
    let view = flow_view(&fx.core, fx.vault, trip);
    assert_eq!(view.mode, FlowMode::NetCapped { cap: 600 });
    assert_eq!(view.income_total, None);
    // The cap now bites.
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::TransferFlow {
            amount: 200,
            from_flow_id: fx.unallocated,
            to_flow_id: trip,
            note: None,
            occurred_at: at(T0 + 1),
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::MaxBalanceReached("Trip".to_string()));

    // Back to unlimited clears cap and income total.
    run(
        &mut fx.core,
        fx.vault,
        Command::UpdateFlow {
            flow_id: trip,
            name: None,
            mode: Some(FlowMode::Unlimited),
            allow_negative: None,
        },
    );
    let view = flow_view(&fx.core, fx.vault, trip);
    assert_eq!(view.mode, FlowMode::Unlimited);
    assert_eq!(view.income_total, None);
}

#[test]
fn update_flow_income_cap_recomputes_ignoring_voided_transactions() {
    let (mut fx, trip) = with_flow(0, FlowMode::Unlimited, false);
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(500, None, Some(trip), None, T0 + 1)),
    );
    let voided = run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(300, None, Some(trip), None, T0 + 2)),
    )
    .result_id
    .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::VoidTransaction {
            transaction_id: voided,
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(100, None, Some(trip), None, T0 + 3)),
    );
    assert_eq!(flow_balance(&fx.core, fx.vault, trip), 400);

    // 500 counts, the voided 300 does not; spending does not free room.
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::UpdateFlow {
            flow_id: trip,
            name: None,
            mode: Some(FlowMode::IncomeCapped { cap: 450 }),
            allow_negative: None,
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::MaxBalanceReached("Trip".to_string()));

    run(
        &mut fx.core,
        fx.vault,
        Command::UpdateFlow {
            flow_id: trip,
            name: None,
            mode: Some(FlowMode::IncomeCapped { cap: 600 }),
            allow_negative: None,
        },
    );
    let view = flow_view(&fx.core, fx.vault, trip);
    assert_eq!(view.mode, FlowMode::IncomeCapped { cap: 600 });
    assert_eq!(view.income_total, Some(500));

    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(100, None, Some(trip), None, T0 + 4)),
    );
    assert_eq!(flow_view(&fx.core, fx.vault, trip).income_total, Some(600));
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(1, None, Some(trip), None, T0 + 5)),
    )
    .unwrap_err();
    assert_eq!(err, DomainError::MaxBalanceReached("Trip".to_string()));
}

#[test]
fn update_flow_allow_negative_cannot_be_turned_off_while_negative() {
    let (mut fx, trip) = with_flow(0, FlowMode::Unlimited, true);
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(250, None, Some(trip), None, T0 + 1)),
    );
    assert_eq!(flow_balance(&fx.core, fx.vault, trip), -250);

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::UpdateFlow {
            flow_id: trip,
            name: Some("Holiday".to_string()),
            mode: None,
            allow_negative: Some(false),
        },
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InsufficientFunds("Trip".to_string()));
    // Atomic: the rename in the same command did not land either.
    assert_eq!(flow_view(&fx.core, fx.vault, trip).name, "Trip");

    run(
        &mut fx.core,
        fx.vault,
        Command::Refund(entry(250, None, Some(trip), None, T0 + 2)),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::UpdateFlow {
            flow_id: trip,
            name: None,
            mode: None,
            allow_negative: Some(false),
        },
    );
    let view = flow_view(&fx.core, fx.vault, trip);
    assert!(!view.allow_negative);
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(1, None, Some(trip), None, T0 + 3)),
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InsufficientFunds("Trip".to_string()));
}

#[test]
fn archive_flow_requires_a_zero_balance_and_refuses_unallocated() {
    let (mut fx, trip) = with_flow(500, FlowMode::Unlimited, false);

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveFlow { flow_id: trip },
    )
    .unwrap_err();
    assert_eq!(
        err,
        DomainError::InvalidFlow("flow has a non-zero balance".to_string())
    );

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveFlow {
            flow_id: fx.unallocated,
        },
    )
    .unwrap_err();
    assert!(matches!(err, DomainError::InvalidFlow(_)));

    run(
        &mut fx.core,
        fx.vault,
        Command::TransferFlow {
            amount: 500,
            from_flow_id: trip,
            to_flow_id: fx.unallocated,
            note: None,
            occurred_at: at(T0 + 1),
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveFlow { flow_id: trip },
    );
    assert!(flow_view(&fx.core, fx.vault, trip).archived);

    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10, None, Some(trip), None, T0 + 2)),
    )
    .unwrap_err();
    assert_eq!(
        err,
        DomainError::InvalidCommand("flow is archived".to_string())
    );
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveFlow { flow_id: trip },
    )
    .unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));

    run(
        &mut fx.core,
        fx.vault,
        Command::RestoreFlow { flow_id: trip },
    );
    assert!(!flow_view(&fx.core, fx.vault, trip).archived);
    let err = try_run(
        &mut fx.core,
        fx.vault,
        Command::RestoreFlow { flow_id: trip },
    )
    .unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(10, None, Some(trip), None, T0 + 3)),
    );
}

// ---------------------------------------------------------------------------
// Replay
// ---------------------------------------------------------------------------

#[test]
fn replay_reproduces_merges_and_flow_updates() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(2000, None, None, Some("Salary"), T0)),
    );
    let food = create_category(&mut fx.core, fx.vault, "Food");
    let spese = create_category(&mut fx.core, fx.vault, "Spese");
    run(
        &mut fx.core,
        fx.vault,
        Command::AddAlias {
            category_id: spese,
            alias: "spesa".to_string(),
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(120, None, None, Some("Food"), T0 + 1)),
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::MergeCategory {
            source_id: food,
            target_id: spese,
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::RenameCategory {
            category_id: spese,
            name: "Spesa".to_string(),
        },
    );

    let trip = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Trip", FlowMode::Unlimited, false, 400),
    )
    .result_id
    .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::UpdateFlow {
            flow_id: trip,
            name: Some("Holiday".to_string()),
            mode: Some(FlowMode::IncomeCapped { cap: 900 }),
            allow_negative: Some(true),
        },
    );
    let spare = run(&mut fx.core, fx.vault, wallet_cmd("Spare", 0))
        .result_id
        .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::RenameWallet {
            wallet_id: spare,
            name: "Savings".to_string(),
        },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveWallet { wallet_id: spare },
    );

    let log = fx.core.commands_since(fx.vault, 0).unwrap();
    let mut fresh = Core::open_in_memory().unwrap();
    replay(&log, &mut fresh).unwrap();

    assert_eq!(
        fresh.snapshot(fx.vault).unwrap(),
        fx.core.snapshot(fx.vault).unwrap()
    );
    assert_eq!(
        fresh.categories(fx.vault, true).unwrap(),
        fx.core.categories(fx.vault, true).unwrap()
    );
    assert_eq!(
        fresh.aliases(fx.vault).unwrap(),
        fx.core.aliases(fx.vault).unwrap()
    );
    assert_eq!(
        list(&fresh, fx.vault, &all()),
        list(&fx.core, fx.vault, &all())
    );
}
