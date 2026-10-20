//! Acceptance tests for wallet, flow and category management.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use sparagne_core::{Command, DomainError, FlowMode, MergeConflictKind, TransactionFilter};
use uuid::Uuid;

fn category_names(fx: &Fx) -> Vec<String> {
    let categories = fx.core.categories(fx.vault, true).unwrap();
    categories.into_iter().map(|c| c.name).collect()
}

fn alias_names(fx: &Fx) -> Vec<String> {
    let aliases = fx.core.aliases(fx.vault).unwrap();
    aliases.into_iter().map(|a| a.alias).collect()
}

fn create_category(fx: &mut Fx, name: &str) -> Uuid {
    fx.id(Command::CreateCategory {
        name: name.to_string(),
    })
}

fn add_alias(category_id: Uuid, alias: &str) -> Command {
    Command::AddAlias {
        category_id,
        alias: alias.to_string(),
    }
}

fn rename_category(category_id: Uuid, name: &str) -> Command {
    Command::RenameCategory {
        category_id,
        name: name.to_string(),
    }
}

fn update_flow(
    flow_id: Uuid,
    name: Option<&str>,
    mode: Option<FlowMode>,
    allow_negative: Option<bool>,
) -> Command {
    Command::UpdateFlow {
        flow_id,
        name: name.map(str::to_string),
        mode,
        allow_negative,
    }
}

fn flow_view(fx: &Fx, id: Uuid) -> sparagne_core::FlowView {
    let flows = fx.core.snapshot(fx.vault).unwrap().flows;
    flows.into_iter().find(|f| f.id == id).unwrap()
}

fn wallet_view(fx: &Fx, id: Uuid) -> sparagne_core::WalletView {
    let wallets = fx.core.snapshot(fx.vault).unwrap().wallets;
    wallets.into_iter().find(|w| w.id == id).unwrap()
}

#[test]
fn alias_resolves_to_category() {
    let mut fx = setup();
    let spese = create_category(&mut fx, "Spese");
    let alias = fx.run(add_alias(spese, " SPESA "));
    assert_eq!(alias.result_id, Some(alias.command_id));

    fx.run(Command::Income(entry(50, None, None, Some("spesa"), T0)));
    let txs = list(&fx.core, fx.vault, &TransactionFilter::default());
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].category, "Spese");
    assert_eq!(txs[0].category_id, spese);
    assert_eq!(alias_names(&fx), vec!["SPESA".to_string()]);

    fx.run(Command::RemoveAlias {
        category_id: spese,
        alias: "spesa!".to_string(),
    });
    assert!(alias_names(&fx).is_empty());
    let err = fx
        .try_run(Command::RemoveAlias {
            category_id: spese,
            alias: "spesa".to_string(),
        })
        .unwrap_err();
    assert_eq!(err, DomainError::NotFound("alias".to_string()));
}

#[test]
fn alias_must_be_unique_across_names_and_aliases() {
    let mut fx = setup();
    let spese = create_category(&mut fx, "Spese");
    let food = create_category(&mut fx, "Food");
    fx.run(add_alias(spese, "spesa"));

    // Another category's name.
    let err = fx.try_run(add_alias(spese, "Food")).unwrap_err();
    assert_eq!(err, DomainError::AlreadyExists("Food".to_string()));
    // An existing alias, on any category.
    let err = fx.try_run(add_alias(food, "Spesa")).unwrap_err();
    assert_eq!(err, DomainError::AlreadyExists("Spesa".to_string()));
    // Its own name.
    let err = fx.try_run(add_alias(spese, "spese")).unwrap_err();
    assert!(matches!(err, DomainError::AlreadyExists(_)));
    // System categories take no aliases; blank aliases are refused.
    let uncategorized = category_id(&fx, "Uncategorized");
    let err = fx.try_run(add_alias(uncategorized, "misc")).unwrap_err();
    assert_eq!(err, DomainError::InvalidName("system category".to_string()));
    let err = fx.try_run(add_alias(food, "   ")).unwrap_err();
    assert!(matches!(err, DomainError::InvalidName(_)));
    assert_eq!(alias_names(&fx), vec!["spesa".to_string()]);
}

#[test]
fn alias_refused_on_archived_category() {
    let mut fx = setup();
    let travel = create_category(&mut fx, "Travel");
    fx.run(Command::ArchiveCategory {
        category_id: travel,
    });
    let err = fx.try_run(add_alias(travel, "trips")).unwrap_err();
    assert_eq!(
        err,
        DomainError::InvalidName("category is archived".to_string())
    );
}

#[test]
fn rename_category_updates_transactions() {
    let mut fx = setup();
    let food = create_category(&mut fx, "Food");
    fx.run(Command::Income(entry(1000, None, None, None, T0)));
    fx.run(Command::Expense(entry(
        40,
        None,
        None,
        Some("Food"),
        T0 + 1,
    )));

    fx.run(rename_category(food, "  Groceries  "));
    let txs = list(&fx.core, fx.vault, &TransactionFilter::default());
    let expense = txs.iter().find(|t| t.amount == 40).unwrap();
    assert_eq!(expense.category, "Groceries");
    assert_eq!(expense.category_id, food);
    // The old name is free again and creates a distinct category.
    let again = create_category(&mut fx, "Food");
    assert_ne!(again, food);
}

#[test]
fn rename_category_rejects_system_duplicates_and_reserved_names() {
    let mut fx = setup();
    let food = create_category(&mut fx, "Food");
    create_category(&mut fx, "Spese");
    let opening = category_id(&fx, "Opening");

    let err = fx.try_run(rename_category(opening, "Start")).unwrap_err();
    assert_eq!(err, DomainError::InvalidName("system category".to_string()));

    let err = fx.try_run(rename_category(food, " spese ")).unwrap_err();
    assert_eq!(err, DomainError::AlreadyExists("spese".to_string()));

    let err = fx
        .try_run(rename_category(food, "Uncategorized"))
        .unwrap_err();
    assert!(matches!(err, DomainError::InvalidName(_)));

    let err = fx
        .try_run(Command::RenameCategory {
            category_id: Uuid::now_v7(),
            name: "Whatever".to_string(),
        })
        .unwrap_err();
    assert_eq!(err, DomainError::NotFound("category".to_string()));

    // Renaming to its own current name is a no-op, not a conflict.
    fx.run(rename_category(food, "food"));
    assert!(category_names(&fx).contains(&"food".to_string()));
}

#[test]
fn rename_category_to_its_own_alias_drops_the_alias() {
    let mut fx = setup();
    let spese = create_category(&mut fx, "Spese");
    fx.run(add_alias(spese, "Spesa"));
    fx.run(rename_category(spese, "Spesa"));
    assert!(alias_names(&fx).is_empty());
    assert!(category_names(&fx).contains(&"Spesa".to_string()));
    // The name still resolves, now through the category itself.
    fx.run(Command::Income(entry(10, None, None, Some("spesa"), T0)));
    let txs = list(&fx.core, fx.vault, &TransactionFilter::default());
    assert_eq!(txs[0].category_id, spese);
}

#[test]
fn archived_category_rejected_on_create() {
    let mut fx = setup();
    let travel = create_category(&mut fx, "Travel");
    fx.run(Command::ArchiveCategory {
        category_id: travel,
    });
    let err = fx
        .try_run(Command::Income(entry(10, None, None, Some("Travel"), T0)))
        .unwrap_err();
    match err {
        DomainError::InvalidName(message) => assert!(message.contains("archived")),
        other => panic!("unexpected error: {other:?}"),
    }

    // Archiving twice, and restoring an active category, are refused.
    let err = fx
        .try_run(Command::ArchiveCategory {
            category_id: travel,
        })
        .unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));
    fx.run(Command::RestoreCategory {
        category_id: travel,
    });
    let err = fx
        .try_run(Command::RestoreCategory {
            category_id: travel,
        })
        .unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));
    fx.run(Command::Income(entry(10, None, None, Some("Travel"), T0)));

    let uncategorized = category_id(&fx, "Uncategorized");
    let err = fx
        .try_run(Command::ArchiveCategory {
            category_id: uncategorized,
        })
        .unwrap_err();
    assert_eq!(err, DomainError::InvalidName("system category".to_string()));
}

#[test]
fn merge_category_moves_transactions_and_aliases() {
    let mut fx = setup();
    let food = create_category(&mut fx, "Food");
    let spese = create_category(&mut fx, "Spese");
    fx.run(add_alias(spese, "spesa"));
    fx.run(Command::Income(entry(1000, None, None, None, T0)));
    let expense = fx.id(Command::Expense(entry(
        12,
        None,
        None,
        Some("Food"),
        T0 + 1,
    )));

    assert!(fx.core.preview_merge(fx.vault, food, spese).unwrap().ok);
    fx.run(Command::MergeCategory {
        source_id: food,
        target_id: spese,
    });

    let txs = list(&fx.core, fx.vault, &TransactionFilter::default());
    let moved = txs.iter().find(|t| t.id == expense).unwrap();
    assert_eq!(moved.category_id, spese);
    assert_eq!(moved.category, "Spese");

    // Source gone, its aliases and its name now belong to the target.
    assert!(!category_names(&fx).contains(&"Food".to_string()));
    let aliases = fx.core.aliases(fx.vault).unwrap();
    assert_eq!(
        aliases.iter().map(|a| a.alias.as_str()).collect::<Vec<_>>(),
        vec!["Food", "spesa"]
    );
    assert!(aliases.iter().all(|a| a.category_id == spese));

    // Later use of the old name resolves to the target.
    fx.run(Command::Expense(entry(5, None, None, Some("food"), T0 + 2)));
    let txs = list(&fx.core, fx.vault, &TransactionFilter::default());
    let later = txs.iter().find(|t| t.amount == 5).unwrap();
    assert_eq!(later.category_id, spese);
}

#[test]
fn merge_into_a_system_category_adds_no_alias() {
    let mut fx = setup();
    let food = create_category(&mut fx, "Food");
    let uncategorized = category_id(&fx, "Uncategorized");
    fx.run(Command::Income(entry(30, None, None, Some("Food"), T0)));
    fx.run(Command::MergeCategory {
        source_id: food,
        target_id: uncategorized,
    });
    let txs = list(&fx.core, fx.vault, &TransactionFilter::default());
    assert_eq!(txs[0].category_id, uncategorized);
    assert!(alias_names(&fx).is_empty());
    assert!(!category_names(&fx).contains(&"Food".to_string()));
}

#[test]
fn preview_merge_reports_conflicts() {
    let mut fx = setup();
    let food = create_category(&mut fx, "Food");
    let travel = create_category(&mut fx, "Travel");
    let opening = category_id(&fx, "Opening");
    fx.run(Command::ArchiveCategory {
        category_id: travel,
    });

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
    let err = fx
        .try_run(Command::MergeCategory {
            source_id: food,
            target_id: travel,
        })
        .unwrap_err();
    match err {
        DomainError::InvalidCommand(message) => assert!(message.contains("target_archived")),
        other => panic!("unexpected error: {other:?}"),
    }
    let err = fx
        .try_run(Command::MergeCategory {
            source_id: opening,
            target_id: food,
        })
        .unwrap_err();
    match err {
        DomainError::InvalidCommand(message) => assert!(message.contains("source_system")),
        other => panic!("unexpected error: {other:?}"),
    }
    // Nothing was touched.
    assert!(category_names(&fx).contains(&"Food".to_string()));
    assert!(category_names(&fx).contains(&"Opening".to_string()));
}

#[test]
fn similar_names_are_suggested_and_creation_is_never_blocked() {
    let mut fx = setup();
    create_category(&mut fx, "Spesa");

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
    let spese = create_category(&mut fx, "Spese");
    fx.run(Command::Income(entry(50, None, None, Some("spese"), T0)));
    let txs = list(&fx.core, fx.vault, &TransactionFilter::default());
    assert_eq!(txs[0].category_id, spese);

    // Archived and system categories are never suggested.
    fx.run(Command::ArchiveCategory { category_id: spese });
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

#[test]
fn rename_wallet_trims_and_keeps_names_unique() {
    let mut fx = setup();
    let spare = fx.id(wallet_cmd("Spare", 0));

    fx.run(Command::RenameWallet {
        wallet_id: spare,
        name: "  Savings  ".to_string(),
    });
    assert_eq!(wallet_view(&fx, spare).name, "Savings");

    let err = fx
        .try_run(Command::RenameWallet {
            wallet_id: spare,
            name: "cash".to_string(),
        })
        .unwrap_err();
    assert_eq!(err, DomainError::AlreadyExists("cash".to_string()));

    let err = fx
        .try_run(Command::RenameWallet {
            wallet_id: spare,
            name: "   ".to_string(),
        })
        .unwrap_err();
    assert!(matches!(err, DomainError::InvalidName(_)));

    let err = fx
        .try_run(Command::RenameWallet {
            wallet_id: Uuid::now_v7(),
            name: "Ghost".to_string(),
        })
        .unwrap_err();
    assert_eq!(err, DomainError::NotFound("wallet".to_string()));

    // Renaming to its own name, in another case, is allowed.
    fx.run(Command::RenameWallet {
        wallet_id: spare,
        name: "SAVINGS".to_string(),
    });
    assert_eq!(wallet_view(&fx, spare).name, "SAVINGS");
}

#[test]
fn wallet_archive_requires_a_zero_balance_and_refuses_new_legs() {
    let mut fx = setup();
    let w = Some(fx.wallet);
    let spare = fx.id(wallet_cmd("Spare", 0));
    fx.run(Command::Income(entry(1000, w, None, None, T0)));

    let err = fx
        .try_run(Command::ArchiveWallet {
            wallet_id: fx.wallet,
        })
        .unwrap_err();
    assert_eq!(
        err,
        DomainError::InvalidCommand("wallet has a non-zero balance".to_string())
    );

    fx.run(Command::ArchiveWallet { wallet_id: spare });
    assert!(wallet_view(&fx, spare).archived);

    let err = fx
        .try_run(Command::Income(entry(10, Some(spare), None, None, T0)))
        .unwrap_err();
    assert_eq!(
        err,
        DomainError::InvalidCommand("wallet is archived".to_string())
    );

    let err = fx
        .try_run(Command::ArchiveWallet { wallet_id: spare })
        .unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));

    fx.run(Command::RestoreWallet { wallet_id: spare });
    assert!(!wallet_view(&fx, spare).archived);
    let err = fx
        .try_run(Command::RestoreWallet { wallet_id: spare })
        .unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));
    fx.run(Command::Income(entry(10, Some(spare), None, None, T0)));
    assert_eq!(wallet_balance(&fx.core, fx.vault, spare), 10);
}

/// A vault with 1000 in the wallet and `Trip` holding `allocation`.
fn with_flow(allocation: i64, mode: FlowMode, allow_negative: bool) -> (Fx, Uuid) {
    let mut fx = setup();
    fx.run(Command::Income(entry(1000, None, None, None, T0)));
    let flow = fx.id(flow_cmd("Trip", mode, allow_negative, allocation));
    (fx, flow)
}

#[test]
fn update_flow_renames_and_rejects_reserved_or_taken_names() {
    let (mut fx, trip) = with_flow(0, FlowMode::Unlimited, false);
    fx.run(flow_cmd("Rent", FlowMode::Unlimited, false, 0));

    fx.run(update_flow(trip, Some("  Holiday  "), None, None));
    assert_eq!(flow_view(&fx, trip).name, "Holiday");

    let err = fx
        .try_run(update_flow(trip, Some("rent"), None, None))
        .unwrap_err();
    assert_eq!(err, DomainError::AlreadyExists("rent".to_string()));

    let err = fx
        .try_run(update_flow(trip, Some("Unallocated"), None, None))
        .unwrap_err();
    assert_eq!(
        err,
        DomainError::InvalidFlow("flow name is reserved".to_string())
    );
}

#[test]
fn update_flow_requires_a_field_and_refuses_unallocated() {
    let (mut fx, trip) = with_flow(0, FlowMode::Unlimited, false);
    let err = fx.try_run(update_flow(trip, None, None, None)).unwrap_err();
    assert_eq!(
        err,
        DomainError::InvalidCommand("nothing to update".to_string())
    );

    let err = fx
        .try_run(update_flow(fx.unallocated, Some("Pot"), None, None))
        .unwrap_err();
    assert!(matches!(err, DomainError::InvalidFlow(_)));

    let err = fx
        .try_run(update_flow(Uuid::now_v7(), Some("Pot"), None, None))
        .unwrap_err();
    assert_eq!(err, DomainError::NotFound("flow".to_string()));
}

#[test]
fn update_flow_cap_must_hold_for_the_current_balance() {
    let (mut fx, trip) = with_flow(500, FlowMode::Unlimited, false);

    let err = fx
        .try_run(update_flow(
            trip,
            None,
            Some(FlowMode::NetCapped { cap: 400 }),
            None,
        ))
        .unwrap_err();
    assert_eq!(err, DomainError::MaxBalanceReached("Trip".to_string()));
    assert_eq!(flow_view(&fx, trip).mode, FlowMode::Unlimited);

    let err = fx
        .try_run(update_flow(
            trip,
            None,
            Some(FlowMode::NetCapped { cap: 0 }),
            None,
        ))
        .unwrap_err();
    assert!(matches!(err, DomainError::InvalidFlow(_)));

    fx.run(update_flow(
        trip,
        None,
        Some(FlowMode::NetCapped { cap: 600 }),
        None,
    ));
    let view = flow_view(&fx, trip);
    assert_eq!(view.mode, FlowMode::NetCapped { cap: 600 });
    assert_eq!(view.income_total, None);
    // The cap now bites.
    let err = fx
        .try_run(Command::TransferFlow {
            amount: 200,
            from_flow_id: fx.unallocated,
            to_flow_id: trip,
            note: None,
            occurred_at: at(T0 + 1),
        })
        .unwrap_err();
    assert_eq!(err, DomainError::MaxBalanceReached("Trip".to_string()));

    // Back to unlimited clears cap and income total.
    fx.run(update_flow(trip, None, Some(FlowMode::Unlimited), None));
    let view = flow_view(&fx, trip);
    assert_eq!(view.mode, FlowMode::Unlimited);
    assert_eq!(view.income_total, None);
}

#[test]
fn update_flow_income_cap_recomputes_ignoring_voided_transactions() {
    let (mut fx, trip) = with_flow(0, FlowMode::Unlimited, false);
    fx.run(Command::Income(entry(500, None, Some(trip), None, T0 + 1)));
    let voided = fx.id(Command::Income(entry(300, None, Some(trip), None, T0 + 2)));
    fx.run(Command::VoidTransaction {
        transaction_id: voided,
    });
    fx.run(Command::Expense(entry(100, None, Some(trip), None, T0 + 3)));
    assert_eq!(flow_balance(&fx.core, fx.vault, trip), 400);

    // 500 counts, the voided 300 does not; spending does not free room.
    let err = fx
        .try_run(update_flow(
            trip,
            None,
            Some(FlowMode::IncomeCapped { cap: 450 }),
            None,
        ))
        .unwrap_err();
    assert_eq!(err, DomainError::MaxBalanceReached("Trip".to_string()));

    fx.run(update_flow(
        trip,
        None,
        Some(FlowMode::IncomeCapped { cap: 600 }),
        None,
    ));
    let view = flow_view(&fx, trip);
    assert_eq!(view.mode, FlowMode::IncomeCapped { cap: 600 });
    assert_eq!(view.income_total, Some(500));

    fx.run(Command::Income(entry(100, None, Some(trip), None, T0 + 4)));
    assert_eq!(flow_view(&fx, trip).income_total, Some(600));
    let err = fx
        .try_run(Command::Income(entry(1, None, Some(trip), None, T0 + 5)))
        .unwrap_err();
    assert_eq!(err, DomainError::MaxBalanceReached("Trip".to_string()));
}

#[test]
fn update_flow_allow_negative_cannot_be_turned_off_while_negative() {
    let (mut fx, trip) = with_flow(0, FlowMode::Unlimited, true);
    fx.run(Command::Expense(entry(250, None, Some(trip), None, T0 + 1)));
    assert_eq!(flow_balance(&fx.core, fx.vault, trip), -250);

    let err = fx
        .try_run(update_flow(trip, Some("Holiday"), None, Some(false)))
        .unwrap_err();
    assert_eq!(err, DomainError::InsufficientFunds("Trip".to_string()));
    // Atomic: the rename in the same command did not land either.
    assert_eq!(flow_view(&fx, trip).name, "Trip");

    fx.run(Command::Refund(entry(250, None, Some(trip), None, T0 + 2)));
    fx.run(update_flow(trip, None, None, Some(false)));
    let view = flow_view(&fx, trip);
    assert!(!view.allow_negative);
    let err = fx
        .try_run(Command::Expense(entry(1, None, Some(trip), None, T0 + 3)))
        .unwrap_err();
    assert_eq!(err, DomainError::InsufficientFunds("Trip".to_string()));
}

#[test]
fn archive_flow_requires_a_zero_balance_and_refuses_unallocated() {
    let (mut fx, trip) = with_flow(500, FlowMode::Unlimited, false);

    let err = fx
        .try_run(Command::ArchiveFlow { flow_id: trip })
        .unwrap_err();
    assert_eq!(
        err,
        DomainError::InvalidFlow("flow has a non-zero balance".to_string())
    );

    let err = fx
        .try_run(Command::ArchiveFlow {
            flow_id: fx.unallocated,
        })
        .unwrap_err();
    assert!(matches!(err, DomainError::InvalidFlow(_)));

    fx.run(Command::TransferFlow {
        amount: 500,
        from_flow_id: trip,
        to_flow_id: fx.unallocated,
        note: None,
        occurred_at: at(T0 + 1),
    });
    fx.run(Command::ArchiveFlow { flow_id: trip });
    assert!(flow_view(&fx, trip).archived);

    let err = fx
        .try_run(Command::Income(entry(10, None, Some(trip), None, T0 + 2)))
        .unwrap_err();
    assert_eq!(
        err,
        DomainError::InvalidCommand("flow is archived".to_string())
    );
    let err = fx
        .try_run(Command::ArchiveFlow { flow_id: trip })
        .unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));

    fx.run(Command::RestoreFlow { flow_id: trip });
    assert!(!flow_view(&fx, trip).archived);
    let err = fx
        .try_run(Command::RestoreFlow { flow_id: trip })
        .unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));
    fx.run(Command::Income(entry(10, None, Some(trip), None, T0 + 3)));
}

#[test]
fn replay_reproduces_merges_and_flow_updates() {
    let mut fx = setup();
    fx.run(Command::Income(entry(2000, None, None, Some("Salary"), T0)));
    let food = create_category(&mut fx, "Food");
    let spese = create_category(&mut fx, "Spese");
    fx.run(add_alias(spese, "spesa"));
    fx.run(Command::Expense(entry(
        120,
        None,
        None,
        Some("Food"),
        T0 + 1,
    )));
    fx.run(Command::MergeCategory {
        source_id: food,
        target_id: spese,
    });
    fx.run(rename_category(spese, "Spesa"));

    let trip = fx.id(flow_cmd("Trip", FlowMode::Unlimited, false, 400));
    fx.run(update_flow(
        trip,
        Some("Holiday"),
        Some(FlowMode::IncomeCapped { cap: 900 }),
        Some(true),
    ));
    let spare = fx.id(wallet_cmd("Spare", 0));
    fx.run(Command::RenameWallet {
        wallet_id: spare,
        name: "Savings".to_string(),
    });
    fx.run(Command::ArchiveWallet { wallet_id: spare });

    let fresh = replayed(&fx.core, fx.vault);
    assert_eq!(
        fresh.aliases(fx.vault).unwrap(),
        fx.core.aliases(fx.vault).unwrap()
    );
}
