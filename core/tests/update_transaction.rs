//! Acceptance tests for `Command::UpdateTransaction`, ported from the v1
//! engine suite plus the cases the v2 semantics add.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use sparagne_core::{
    Command, Core, DomainError, Entry, FlowMode, LegTarget, LegView, TransactionFilter,
    TransactionKind, TransactionPatch, TransactionView, replay,
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Builder around [`TransactionPatch`]; every field defaults to "leave as is".
#[derive(Clone)]
struct Patch {
    transaction_id: Uuid,
    patch: TransactionPatch,
}

fn patch(transaction_id: Uuid) -> Patch {
    Patch {
        transaction_id,
        patch: TransactionPatch::default(),
    }
}

impl Patch {
    fn amount(mut self, value: i64) -> Self {
        self.patch.amount = Some(value);
        self
    }

    fn occurred_at(mut self, secs: i64) -> Self {
        self.patch.occurred_at = Some(at(secs));
        self
    }

    fn category(mut self, value: &str) -> Self {
        self.patch.category = Some(value.to_string());
        self
    }

    fn note(mut self, value: &str) -> Self {
        self.patch.note = Some(value.to_string());
        self
    }

    fn wallet_id(mut self, value: Uuid) -> Self {
        self.patch.wallet_id = Some(value);
        self
    }

    fn flow_id(mut self, value: Uuid) -> Self {
        self.patch.flow_id = Some(value);
        self
    }

    fn source(mut self, value: Uuid) -> Self {
        self.patch.from_id = Some(value);
        self
    }

    fn destination(mut self, value: Uuid) -> Self {
        self.patch.to_id = Some(value);
        self
    }

    fn cmd(self) -> Command {
        Command::UpdateTransaction {
            transaction_id: self.transaction_id,
            patch: self.patch,
        }
    }
}

fn with_note(entry: Entry, note: &str) -> Entry {
    Entry {
        note: Some(note.to_string()),
        ..entry
    }
}

fn fail(core: &mut Core, vault: Uuid, cmd: Command) -> DomainError {
    try_run(core, vault, cmd).unwrap_err()
}

fn view(core: &Core, vault: Uuid, id: Uuid) -> TransactionView {
    list(core, vault, &all())
        .into_iter()
        .find(|t| t.id == id)
        .expect("transaction is listed")
}

fn wallet_leg(id: Uuid, amount: i64) -> LegView {
    LegView {
        target: LegTarget::Wallet { wallet_id: id },
        amount,
    }
}

fn flow_leg(id: Uuid, amount: i64) -> LegView {
    LegView {
        target: LegTarget::Flow { flow_id: id },
        amount,
    }
}

fn income_total(core: &Core, vault: Uuid, flow: Uuid) -> Option<i64> {
    core.snapshot(vault)
        .unwrap()
        .flows
        .into_iter()
        .find(|f| f.id == flow)
        .unwrap()
        .income_total
}

/// Every transaction moves the same money on both sides of the ledger.
fn assert_balanced(core: &Core, vault: Uuid) {
    let (wallets, flows) = balances(core, vault);
    let sum_wallets: i64 = wallets.iter().map(|(_, b)| b).sum();
    let sum_flows: i64 = flows.iter().map(|(_, b)| b).sum();
    assert_eq!(
        sum_wallets, sum_flows,
        "sum of wallet balances must equal sum of flow balances"
    );
}

fn flow(core: &mut Core, vault: Uuid, name: &str) -> Uuid {
    run(core, vault, flow_cmd(name, FlowMode::Unlimited, false, 0))
        .result_id
        .unwrap()
}

fn wallet(core: &mut Core, vault: Uuid, name: &str) -> Uuid {
    run(core, vault, wallet_cmd(name, 0)).result_id.unwrap()
}

// ---------------------------------------------------------------------------
// Ported from v1
// ---------------------------------------------------------------------------

#[test]
fn update_transaction_updates_balances() {
    let mut fx = setup();
    let vacanze = flow(&mut fx.core, fx.vault, "Vacanze");
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(1000, Some(fx.wallet), Some(vacanze), None, T0 + 1)),
    );
    let expense = run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(
            100,
            Some(fx.wallet),
            Some(vacanze),
            Some("food"),
            T0 + 2,
        )),
    )
    .result_id
    .unwrap();

    run(
        &mut fx.core,
        fx.vault,
        patch(expense)
            .amount(150)
            .category("food")
            .note("bigger lunch")
            .cmd(),
    );

    assert_eq!(flow_balance(&fx.core, fx.vault, vacanze), 850);
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 850);
    let updated = view(&fx.core, fx.vault, expense);
    assert_eq!(updated.amount, 150);
    assert_eq!(updated.note.as_deref(), Some("bigger lunch"));
    assert_eq!(
        updated.legs,
        vec![wallet_leg(fx.wallet, -150), flow_leg(vacanze, -150)]
    );
    assert_balanced(&fx.core, fx.vault);
}

#[test]
fn update_income_can_retarget_wallet_and_flow_and_keeps_metadata_when_omitted() {
    let mut fx = setup();
    let f1 = flow(&mut fx.core, fx.vault, "F1");
    let f2 = flow(&mut fx.core, fx.vault, "F2");
    let bank = wallet(&mut fx.core, fx.vault, "Bank");
    let tx = run(
        &mut fx.core,
        fx.vault,
        Command::Income(with_note(
            entry(100, Some(fx.wallet), Some(f1), Some("salary"), T0 + 1),
            "  hi  ",
        )),
    )
    .result_id
    .unwrap();

    run(
        &mut fx.core,
        fx.vault,
        patch(tx).wallet_id(bank).flow_id(f2).cmd(),
    );

    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 0);
    assert_eq!(wallet_balance(&fx.core, fx.vault, bank), 100);
    assert_eq!(flow_balance(&fx.core, fx.vault, f1), 0);
    assert_eq!(flow_balance(&fx.core, fx.vault, f2), 100);

    let updated = view(&fx.core, fx.vault, tx);
    assert_eq!(updated.category, "salary");
    assert_eq!(updated.note.as_deref(), Some("hi"));
    assert_eq!(updated.occurred_at, at(T0 + 1));
    assert_eq!(updated.legs, vec![wallet_leg(bank, 100), flow_leg(f2, 100)]);
    assert_balanced(&fx.core, fx.vault);
}

#[test]
fn update_expense_retarget_flow_fails_if_insufficient_and_is_atomic() {
    let mut fx = setup();
    let f1 = flow(&mut fx.core, fx.vault, "F1");
    let f2 = flow(&mut fx.core, fx.vault, "F2");
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(100, Some(fx.wallet), Some(f1), None, T0 + 1)),
    );
    let expense = run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(80, Some(fx.wallet), Some(f1), None, T0 + 2)),
    )
    .result_id
    .unwrap();
    let log_before = fx.core.commands_since(fx.vault, 0).unwrap().len();

    let err = fail(&mut fx.core, fx.vault, patch(expense).flow_id(f2).cmd());
    assert_eq!(err, DomainError::InsufficientFunds("F2".to_string()));

    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 20);
    assert_eq!(flow_balance(&fx.core, fx.vault, f1), 20);
    assert_eq!(flow_balance(&fx.core, fx.vault, f2), 0);
    assert_eq!(
        view(&fx.core, fx.vault, expense).legs,
        vec![wallet_leg(fx.wallet, -80), flow_leg(f1, -80)]
    );
    assert_eq!(
        fx.core.commands_since(fx.vault, 0).unwrap().len(),
        log_before,
        "a rejected command is not logged"
    );
    assert_balanced(&fx.core, fx.vault);
}

#[test]
fn update_transfer_wallet_can_change_endpoints_and_amount() {
    let mut fx = setup();
    let bank = wallet(&mut fx.core, fx.vault, "Bank");
    let card = wallet(&mut fx.core, fx.vault, "Card");
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(100, Some(fx.wallet), None, None, T0 + 1)),
    );
    let tx = run(
        &mut fx.core,
        fx.vault,
        Command::TransferWallet {
            amount: 50,
            from_wallet_id: fx.wallet,
            to_wallet_id: bank,
            note: Some(" move ".to_string()),
            occurred_at: at(T0 + 2),
        },
    )
    .result_id
    .unwrap();

    run(
        &mut fx.core,
        fx.vault,
        patch(tx)
            .amount(30)
            .source(bank)
            .destination(card)
            .note("   ")
            .cmd(),
    );

    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 100);
    assert_eq!(wallet_balance(&fx.core, fx.vault, bank), -30);
    assert_eq!(wallet_balance(&fx.core, fx.vault, card), 30);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 100);

    let updated = view(&fx.core, fx.vault, tx);
    assert_eq!(updated.note, None);
    assert_eq!(updated.kind, TransactionKind::TransferWallet);
    assert_eq!(updated.amount, 30);
    assert_eq!(
        updated.legs,
        vec![wallet_leg(bank, -30), wallet_leg(card, 30)]
    );
    assert_balanced(&fx.core, fx.vault);
}

#[test]
fn update_transfer_flow_can_change_endpoints_and_amount() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(100, Some(fx.wallet), None, None, T0 + 1)),
    );
    let f1 = flow(&mut fx.core, fx.vault, "F1");
    let f2 = flow(&mut fx.core, fx.vault, "F2");
    let f3 = flow(&mut fx.core, fx.vault, "F3");
    run(
        &mut fx.core,
        fx.vault,
        Command::TransferFlow {
            amount: 60,
            from_flow_id: fx.unallocated,
            to_flow_id: f1,
            note: Some("seed".to_string()),
            occurred_at: at(T0 + 2),
        },
    );
    let tx = run(
        &mut fx.core,
        fx.vault,
        Command::TransferFlow {
            amount: 40,
            from_flow_id: f1,
            to_flow_id: f2,
            note: Some("move".to_string()),
            occurred_at: at(T0 + 3),
        },
    )
    .result_id
    .unwrap();

    run(
        &mut fx.core,
        fx.vault,
        patch(tx).amount(10).source(f1).destination(f3).cmd(),
    );

    assert_eq!(flow_balance(&fx.core, fx.vault, f1), 50);
    assert_eq!(flow_balance(&fx.core, fx.vault, f2), 0);
    assert_eq!(flow_balance(&fx.core, fx.vault, f3), 10);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 40);
    assert_eq!(
        view(&fx.core, fx.vault, tx).legs,
        vec![flow_leg(f1, -10), flow_leg(f3, 10)]
    );
    assert_balanced(&fx.core, fx.vault);
}

// ---------------------------------------------------------------------------
// v2 semantics
// ---------------------------------------------------------------------------

#[test]
fn voided_transactions_cannot_be_updated() {
    let mut fx = setup();
    let tx = run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(100, Some(fx.wallet), None, None, T0 + 1)),
    )
    .result_id
    .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::VoidTransaction { transaction_id: tx },
    );

    let err = fail(&mut fx.core, fx.vault, patch(tx).amount(200).cmd());
    assert_eq!(
        err,
        DomainError::InvalidCommand("transaction is voided".to_string())
    );
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 0);
    assert_balanced(&fx.core, fx.vault);
}

#[test]
fn update_rejects_invalid_patches() {
    let mut fx = setup();
    let bank = wallet(&mut fx.core, fx.vault, "Bank");
    let tx = run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(100, Some(fx.wallet), None, None, T0 + 1)),
    )
    .result_id
    .unwrap();
    let transfer = run(
        &mut fx.core,
        fx.vault,
        Command::TransferWallet {
            amount: 40,
            from_wallet_id: fx.wallet,
            to_wallet_id: bank,
            note: None,
            occurred_at: at(T0 + 2),
        },
    )
    .result_id
    .unwrap();

    assert_eq!(
        fail(
            &mut fx.core,
            fx.vault,
            patch(Uuid::now_v7()).amount(10).cmd()
        ),
        DomainError::NotFound("transaction".to_string())
    );
    assert_eq!(
        fail(&mut fx.core, fx.vault, patch(tx).cmd()),
        DomainError::InvalidCommand("nothing to update".to_string())
    );
    assert_eq!(
        fail(&mut fx.core, fx.vault, patch(tx).amount(0).cmd()),
        DomainError::InvalidAmount("amount must be > 0".to_string())
    );
    assert_eq!(
        fail(&mut fx.core, fx.vault, patch(tx).amount(-5).cmd()),
        DomainError::InvalidAmount("amount must be > 0".to_string())
    );
    assert_eq!(
        fail(
            &mut fx.core,
            fx.vault,
            patch(transfer).destination(fx.wallet).cmd()
        ),
        DomainError::InvalidCommand("from and to must differ".to_string())
    );
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 60);
    assert_eq!(wallet_balance(&fx.core, fx.vault, bank), 40);
    assert_balanced(&fx.core, fx.vault);
}

#[test]
fn fields_that_do_not_belong_to_the_kind_are_refused() {
    let mut fx = setup();
    let bank = wallet(&mut fx.core, fx.vault, "Bank");
    let f1 = flow(&mut fx.core, fx.vault, "F1");
    let entry_tx = run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(100, Some(fx.wallet), Some(f1), None, T0 + 1)),
    )
    .result_id
    .unwrap();
    let transfer = run(
        &mut fx.core,
        fx.vault,
        Command::TransferWallet {
            amount: 40,
            from_wallet_id: fx.wallet,
            to_wallet_id: bank,
            note: None,
            occurred_at: at(T0 + 2),
        },
    )
    .result_id
    .unwrap();

    for cmd in [
        patch(entry_tx).source(fx.wallet).cmd(),
        patch(entry_tx).destination(bank).cmd(),
    ] {
        assert_eq!(
            fail(&mut fx.core, fx.vault, cmd),
            DomainError::InvalidCommand("from and to are only valid on transfers".to_string())
        );
    }
    for cmd in [
        patch(transfer).category("food").cmd(),
        patch(transfer).wallet_id(bank).cmd(),
        patch(transfer).flow_id(f1).cmd(),
    ] {
        assert_eq!(
            fail(&mut fx.core, fx.vault, cmd),
            DomainError::InvalidCommand(
                "category, wallet and flow are only valid on entries".to_string()
            )
        );
    }
    assert_balanced(&fx.core, fx.vault);
}

#[test]
fn archived_targets_are_refused() {
    let mut fx = setup();
    let bank = wallet(&mut fx.core, fx.vault, "Bank");
    let spare = flow(&mut fx.core, fx.vault, "Spare");
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveWallet { wallet_id: bank },
    );
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveFlow { flow_id: spare },
    );
    let tx = run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(100, Some(fx.wallet), None, None, T0 + 1)),
    )
    .result_id
    .unwrap();

    assert_eq!(
        fail(&mut fx.core, fx.vault, patch(tx).wallet_id(bank).cmd()),
        DomainError::InvalidCommand("wallet is archived".to_string())
    );
    assert_eq!(
        fail(&mut fx.core, fx.vault, patch(tx).flow_id(spare).cmd()),
        DomainError::InvalidCommand("flow is archived".to_string())
    );
    assert_eq!(
        fail(
            &mut fx.core,
            fx.vault,
            patch(tx).flow_id(Uuid::now_v7()).cmd()
        ),
        DomainError::NotFound("flow".to_string())
    );
    assert_eq!(
        fail(
            &mut fx.core,
            fx.vault,
            patch(tx).wallet_id(Uuid::now_v7()).cmd()
        ),
        DomainError::NotFound("wallet".to_string())
    );
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 100);
    assert_balanced(&fx.core, fx.vault);
}

#[test]
fn income_capped_flow_caps_the_new_amount_and_frees_room_when_lowered() {
    let mut fx = setup();
    let capped = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Capped", FlowMode::IncomeCapped { cap: 1000 }, false, 0),
    )
    .result_id
    .unwrap();
    let tx = run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(400, Some(fx.wallet), Some(capped), None, T0 + 1)),
    )
    .result_id
    .unwrap();
    assert_eq!(income_total(&fx.core, fx.vault, capped), Some(400));

    assert_eq!(
        fail(&mut fx.core, fx.vault, patch(tx).amount(1200).cmd()),
        DomainError::MaxBalanceReached("Capped".to_string())
    );
    assert_eq!(income_total(&fx.core, fx.vault, capped), Some(400));
    assert_eq!(flow_balance(&fx.core, fx.vault, capped), 400);

    run(&mut fx.core, fx.vault, patch(tx).amount(200).cmd());
    assert_eq!(income_total(&fx.core, fx.vault, capped), Some(200));
    assert_eq!(flow_balance(&fx.core, fx.vault, capped), 200);

    // The room freed by lowering the amount is usable again.
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(800, Some(fx.wallet), Some(capped), None, T0 + 2)),
    );
    assert_eq!(income_total(&fx.core, fx.vault, capped), Some(1000));
    assert_balanced(&fx.core, fx.vault);
}

#[test]
fn net_capped_flow_rejects_a_retarget_over_its_cap() {
    let mut fx = setup();
    let wide = flow(&mut fx.core, fx.vault, "Wide");
    let small = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Small", FlowMode::NetCapped { cap: 100 }, false, 0),
    )
    .result_id
    .unwrap();
    let tx = run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(500, Some(fx.wallet), Some(wide), None, T0 + 1)),
    )
    .result_id
    .unwrap();

    assert_eq!(
        fail(&mut fx.core, fx.vault, patch(tx).flow_id(small).cmd()),
        DomainError::MaxBalanceReached("Small".to_string())
    );
    assert_eq!(flow_balance(&fx.core, fx.vault, wide), 500);
    assert_eq!(flow_balance(&fx.core, fx.vault, small), 0);

    run(
        &mut fx.core,
        fx.vault,
        patch(tx).flow_id(small).amount(100).cmd(),
    );
    assert_eq!(flow_balance(&fx.core, fx.vault, wide), 0);
    assert_eq!(flow_balance(&fx.core, fx.vault, small), 100);
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 100);
    assert_balanced(&fx.core, fx.vault);
}

#[test]
fn transfer_flow_endpoints_can_be_swapped() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(300, Some(fx.wallet), None, None, T0 + 1)),
    );
    let f1 = flow(&mut fx.core, fx.vault, "F1");
    let f2 = flow(&mut fx.core, fx.vault, "F2");
    for (amount, to) in [(150, f1), (80, f2)] {
        run(
            &mut fx.core,
            fx.vault,
            Command::TransferFlow {
                amount,
                from_flow_id: fx.unallocated,
                to_flow_id: to,
                note: None,
                occurred_at: at(T0 + 2),
            },
        );
    }
    let tx = run(
        &mut fx.core,
        fx.vault,
        Command::TransferFlow {
            amount: 50,
            from_flow_id: f1,
            to_flow_id: f2,
            note: None,
            occurred_at: at(T0 + 3),
        },
    )
    .result_id
    .unwrap();
    assert_eq!(flow_balance(&fx.core, fx.vault, f1), 100);
    assert_eq!(flow_balance(&fx.core, fx.vault, f2), 130);

    run(
        &mut fx.core,
        fx.vault,
        patch(tx).source(f2).destination(f1).cmd(),
    );

    assert_eq!(flow_balance(&fx.core, fx.vault, f1), 200);
    assert_eq!(flow_balance(&fx.core, fx.vault, f2), 30);
    assert_eq!(flow_balance(&fx.core, fx.vault, fx.unallocated), 70);
    assert_eq!(
        view(&fx.core, fx.vault, tx).legs,
        vec![flow_leg(f2, -50), flow_leg(f1, 50)]
    );
    assert_balanced(&fx.core, fx.vault);
}

#[test]
fn blank_category_falls_back_to_uncategorized() {
    let mut fx = setup();
    let tx = run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(100, Some(fx.wallet), None, Some("Salary"), T0 + 1)),
    )
    .result_id
    .unwrap();
    assert_eq!(view(&fx.core, fx.vault, tx).category, "Salary");

    run(&mut fx.core, fx.vault, patch(tx).category("   ").cmd());
    assert_eq!(view(&fx.core, fx.vault, tx).category, "Uncategorized");
    assert_balanced(&fx.core, fx.vault);
}

#[test]
fn list_transactions_shows_the_updated_row() {
    let mut fx = setup();
    let bank = wallet(&mut fx.core, fx.vault, "Bank");
    let f1 = flow(&mut fx.core, fx.vault, "F1");
    let tx = run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(100, Some(fx.wallet), None, Some("salary"), T0 + 1)),
    )
    .result_id
    .unwrap();

    run(
        &mut fx.core,
        fx.vault,
        patch(tx)
            .amount(250)
            .occurred_at(T0 + 500)
            .category("gift")
            .note("  present  ")
            .wallet_id(bank)
            .flow_id(f1)
            .cmd(),
    );

    let updated = view(&fx.core, fx.vault, tx);
    assert_eq!(updated.kind, TransactionKind::Income);
    assert_eq!(updated.amount, 250);
    assert_eq!(updated.occurred_at, at(T0 + 500));
    assert_eq!(updated.category, "gift");
    assert_eq!(updated.note.as_deref(), Some("present"));
    assert_eq!(updated.legs, vec![wallet_leg(bank, 250), flow_leg(f1, 250)]);
    assert!(!updated.voided);

    // The updated row is the newest one, since it moved forward in time.
    let newest = list(&fx.core, fx.vault, &TransactionFilter::default());
    assert_eq!(newest.first().map(|t| t.id), Some(tx));
    assert_balanced(&fx.core, fx.vault);
}

#[test]
fn updates_replay_into_an_identical_projection() {
    let mut fx = setup();
    let bank = wallet(&mut fx.core, fx.vault, "Bank");
    let f1 = flow(&mut fx.core, fx.vault, "F1");
    let f2 = flow(&mut fx.core, fx.vault, "F2");
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(
            1000,
            Some(fx.wallet),
            Some(f1),
            Some("salary"),
            T0 + 1,
        )),
    );
    let expense = run(
        &mut fx.core,
        fx.vault,
        Command::Expense(entry(300, Some(fx.wallet), Some(f1), Some("food"), T0 + 2)),
    )
    .result_id
    .unwrap();
    let wallet_transfer = run(
        &mut fx.core,
        fx.vault,
        Command::TransferWallet {
            amount: 200,
            from_wallet_id: fx.wallet,
            to_wallet_id: bank,
            note: Some("cash out".to_string()),
            occurred_at: at(T0 + 3),
        },
    )
    .result_id
    .unwrap();
    let flow_transfer = run(
        &mut fx.core,
        fx.vault,
        Command::TransferFlow {
            amount: 400,
            from_flow_id: f1,
            to_flow_id: f2,
            note: None,
            occurred_at: at(T0 + 4),
        },
    )
    .result_id
    .unwrap();
    let voided = run(
        &mut fx.core,
        fx.vault,
        Command::Refund(entry(50, Some(bank), Some(f2), Some("food"), T0 + 5)),
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

    // Amount and metadata, a new auto-created category, a retarget, and both
    // kinds of transfer endpoint change.
    run(
        &mut fx.core,
        fx.vault,
        patch(expense)
            .amount(250)
            .category("groceries")
            .note(" weekly ")
            .cmd(),
    );
    run(
        &mut fx.core,
        fx.vault,
        patch(expense).wallet_id(bank).flow_id(f2).cmd(),
    );
    run(
        &mut fx.core,
        fx.vault,
        patch(wallet_transfer)
            .amount(120)
            .source(bank)
            .destination(fx.wallet)
            .note("")
            .cmd(),
    );
    run(
        &mut fx.core,
        fx.vault,
        patch(flow_transfer).amount(500).occurred_at(T0 + 9).cmd(),
    );
    assert_balanced(&fx.core, fx.vault);

    let log = fx.core.commands_since(fx.vault, 0).unwrap();
    let mut fresh = Core::open_in_memory().unwrap();
    replay(&log, &mut fresh).unwrap();

    assert_eq!(
        fresh.snapshot(fx.vault).unwrap(),
        fx.core.snapshot(fx.vault).unwrap()
    );
    assert_eq!(
        list(&fresh, fx.vault, &all()),
        list(&fx.core, fx.vault, &all())
    );
    assert_eq!(
        fresh.categories(fx.vault, true).unwrap(),
        fx.core.categories(fx.vault, true).unwrap()
    );
    assert_balanced(&fresh, fx.vault);
}
