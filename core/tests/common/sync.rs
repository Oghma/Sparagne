//! Helpers shared by the vault, sync, batch, maintenance and allocation
//! tests. A second in-memory core plays the server through `serve_push` and
//! `serve_pull`.

use chrono::NaiveDate;
use sparagne_core::{
    AliasView, AllocationBase, AllocationRunView, CategoryView, Command, CommandEnvelope, Core,
    FlowMode, PendingAllocation, RecurringView, SyncReport, TransactionView, VaultSnapshot,
    sync::PullResponse,
};
use uuid::Uuid;

use super::{Fx, all, create_vault, entry, flow_cmd, noon, run, wallet_cmd};

/// A push or pull limit that is no limit.
pub const ALL: usize = usize::MAX;

/// A fresh in-memory core holding one vault.
pub fn vault_of(author: &str, name: &str) -> (Core, Uuid) {
    let mut core = Core::open_in_memory().unwrap();
    let vault = create_vault(&mut core, author, name);
    (core, vault)
}

/// A second vault of alice's in the same database, with a wallet and an
/// envelope; returns the vault and the envelope.
pub fn other_vault(core: &mut Core) -> (Uuid, Uuid) {
    let vault = create_vault(core, "alice", "Other");
    run(core, vault, wallet_cmd("Bank", 0));
    let flow = run(
        core,
        vault,
        flow_cmd("Elsewhere", FlowMode::Unlimited, false, 0),
    )
    .result_id
    .unwrap();
    (vault, flow)
}

/// Executes `command` as `author`; returns what it created, or the vault
/// when it created nothing.
pub fn exec(core: &mut Core, vault: Uuid, author: &str, command: Command) -> Uuid {
    core.execute(CommandEnvelope::new(vault, author, command))
        .unwrap()
        .result_id
        .unwrap_or(vault)
}

pub fn rename(name: &str) -> Command {
    Command::RenameVault {
        name: name.to_string(),
    }
}

pub fn category(name: &str) -> Command {
    Command::CreateCategory {
        name: name.to_string(),
    }
}

pub fn spend(amount: i64, wallet: Uuid, flow: Uuid, category: &str, secs: i64) -> Command {
    Command::Expense(entry(
        amount,
        Some(wallet),
        Some(flow),
        Some(category),
        secs,
    ))
}

pub fn pull(server: &Core, vault: Uuid, since: i64) -> PullResponse {
    server.serve_pull(vault, since, ALL).unwrap()
}

/// One round of what the app does: push the whole outbox, fold the answer
/// back, pull from the resulting watermark, fold that back.
pub fn sync(client: &mut Core, server: &mut Core, vault: Uuid) -> (SyncReport, SyncReport) {
    let request = client.push_request(vault, ALL).unwrap();
    let response = server.serve_push(vault, &request).unwrap();
    let pushed = client.apply_push_response(vault, &response).unwrap();
    let since = client.sync_state(vault).unwrap().last_server_seq;
    let pulled = client
        .integrate_pull(vault, &pull(server, vault, since))
        .unwrap();
    (pushed, pulled)
}

/// Everything a user can see of a vault, in one comparable value.
pub type Projection = (
    VaultSnapshot,
    Vec<TransactionView>,
    Vec<CategoryView>,
    Vec<AliasView>,
    Vec<RecurringView>,
);

pub fn projection(core: &Core, vault: Uuid) -> Projection {
    (
        core.snapshot(vault).unwrap(),
        core.list_transactions(vault, &all(), 200, None)
            .unwrap()
            .items,
        core.categories(vault, true).unwrap(),
        core.aliases(vault).unwrap(),
        core.list_recurring(vault, true).unwrap(),
    )
}

/// Noon UTC of `date`, in seconds.
pub fn secs(date: NaiveDate) -> i64 {
    noon(date).timestamp()
}

pub fn income_ids(base: &AllocationBase) -> Vec<Uuid> {
    base.incomes.iter().map(|t| t.id).collect()
}

pub fn base_of(fx: &Fx) -> AllocationBase {
    fx.core.allocation_base(fx.vault).unwrap()
}

pub fn runs_of(fx: &Fx) -> Vec<AllocationRunView> {
    fx.core.allocation_runs(fx.vault, 100).unwrap()
}

pub fn pending_on(fx: &Fx, today: NaiveDate) -> Option<PendingAllocation> {
    fx.core.pending_allocation(fx.vault, today).unwrap()
}
