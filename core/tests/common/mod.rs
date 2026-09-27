//! Shared fixtures for the acceptance tests.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use sparagne_core::{
    Command, CommandEnvelope, Core, Currency, DomainError, Entry, FlowMode, Receipt,
    TransactionFilter, TransactionView,
};
use uuid::Uuid;

pub const T0: i64 = 1_700_000_000;

/// `(id, balance)` pairs.
pub type Balances = Vec<(Uuid, i64)>;

pub fn at(secs: i64) -> DateTime<FixedOffset> {
    Utc.timestamp_opt(secs, 0).unwrap().fixed_offset()
}

pub struct Fx {
    pub core: Core,
    pub vault: Uuid,
    pub wallet: Uuid,
    pub unallocated: Uuid,
}

pub fn setup() -> Fx {
    let mut core = Core::open_in_memory().unwrap();
    let vault = core
        .execute(CommandEnvelope::create_vault(
            "alice",
            "Main",
            Currency::Eur,
        ))
        .unwrap()
        .result_id
        .unwrap();
    let wallet = run(&mut core, vault, wallet_cmd("Cash", 0))
        .result_id
        .unwrap();
    let unallocated = core.snapshot(vault).unwrap().unallocated_flow_id;
    Fx {
        core,
        vault,
        wallet,
        unallocated,
    }
}

pub fn run(core: &mut Core, vault: Uuid, cmd: Command) -> Receipt {
    core.execute(CommandEnvelope::new(vault, "alice", cmd))
        .unwrap()
}

pub fn try_run(core: &mut Core, vault: Uuid, cmd: Command) -> Result<Receipt, DomainError> {
    core.execute(CommandEnvelope::new(vault, "alice", cmd))
}

pub fn wallet_cmd(name: &str, opening: i64) -> Command {
    Command::CreateWallet {
        name: name.to_string(),
        opening_balance: opening,
        occurred_at: at(T0),
    }
}

pub fn flow_cmd(name: &str, mode: FlowMode, allow_negative: bool, opening: i64) -> Command {
    Command::CreateFlow {
        name: name.to_string(),
        mode,
        allow_negative,
        opening_allocation: opening,
        occurred_at: at(T0),
    }
}

pub fn entry(
    amount: i64,
    wallet: Option<Uuid>,
    flow: Option<Uuid>,
    category: Option<&str>,
    secs: i64,
) -> Entry {
    Entry {
        amount,
        wallet_id: wallet,
        flow_id: flow,
        category: category.map(str::to_string),
        note: None,
        occurred_at: at(secs),
    }
}

pub fn balances(core: &Core, vault: Uuid) -> (Balances, Balances) {
    let s = core.snapshot(vault).unwrap();
    (
        s.wallets.iter().map(|w| (w.id, w.balance)).collect(),
        s.flows.iter().map(|f| (f.id, f.balance)).collect(),
    )
}

pub fn wallet_balance(core: &Core, vault: Uuid, id: Uuid) -> i64 {
    balances(core, vault)
        .0
        .into_iter()
        .find(|(w, _)| *w == id)
        .unwrap()
        .1
}

pub fn flow_balance(core: &Core, vault: Uuid, id: Uuid) -> i64 {
    balances(core, vault)
        .1
        .into_iter()
        .find(|(f, _)| *f == id)
        .unwrap()
        .1
}

pub fn list(core: &Core, vault: Uuid, filter: &TransactionFilter) -> Vec<TransactionView> {
    core.list_transactions(vault, filter, 100, None)
        .unwrap()
        .items
}

pub fn all() -> TransactionFilter {
    TransactionFilter {
        include_voided: true,
        include_transfers: true,
        ..Default::default()
    }
}
