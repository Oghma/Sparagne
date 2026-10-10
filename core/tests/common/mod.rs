//! Shared fixtures for the acceptance tests.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use sparagne_core::{
    AllocationLine, AllocationMove, AllocationPlanPatch, AllocationRule, Command, CommandEnvelope,
    Core, Currency, DomainError, Entry, FlowMode, Frequency, Receipt, Schedule, TransactionFilter,
    TransactionView,
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
        person: None,
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

// ---------------------------------------------------------------------------
// Allocation plan
// ---------------------------------------------------------------------------

pub fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// Noon UTC of `date`: when the fixtures say an execution happened.
pub fn noon(date: NaiveDate) -> DateTime<FixedOffset> {
    date.and_hms_opt(12, 0, 0).unwrap().and_utc().fixed_offset()
}

/// Every month on `day_of_month`, from `start`, open-ended.
pub fn monthly_from(day_of_month: u8, start: NaiveDate) -> Schedule {
    Schedule {
        frequency: Frequency::Monthly { day: day_of_month },
        interval: 1,
        start_date: start,
        end_date: None,
    }
}

pub fn fixed(flow_id: Uuid, amount: i64) -> AllocationLine {
    AllocationLine {
        flow_id,
        rule: AllocationRule::Fixed { amount },
    }
}

pub fn percent(flow_id: Uuid, basis_points: u32) -> AllocationLine {
    AllocationLine {
        flow_id,
        rule: AllocationRule::Percent { basis_points },
    }
}

pub fn fill(flow_id: Uuid) -> AllocationLine {
    AllocationLine {
        flow_id,
        rule: AllocationRule::FillToCap,
    }
}

pub fn plan_cmd(schedule: Schedule, lines: Vec<AllocationLine>) -> Command {
    Command::CreateAllocationPlan { schedule, lines }
}

pub fn update_plan_cmd(plan_id: Uuid, patch: AllocationPlanPatch) -> Command {
    Command::UpdateAllocationPlan { plan_id, patch }
}

/// `moves` as `(flow, amount)` pairs, executed at noon of `period`.
pub fn execute_cmd(plan_id: Uuid, period: NaiveDate, total: i64, moves: &[(Uuid, i64)]) -> Command {
    Command::ExecuteAllocation {
        plan_id,
        period_date: period,
        occurred_at: noon(period),
        total,
        moves: moves
            .iter()
            .map(|&(flow_id, amount)| AllocationMove { flow_id, amount })
            .collect(),
        note: None,
    }
}

pub fn skip_cmd(plan_id: Uuid, period: NaiveDate) -> Command {
    Command::SkipAllocation {
        plan_id,
        period_date: period,
    }
}

pub fn reopen_cmd(plan_id: Uuid, period: NaiveDate) -> Command {
    Command::ReopenAllocation {
        plan_id,
        period_date: period,
    }
}

/// An income of `amount` into Unallocated on the only wallet, at `secs`;
/// returns the transaction id.
pub fn income_in(fx: &mut Fx, amount: i64, secs: i64) -> Uuid {
    run(
        &mut fx.core,
        fx.vault,
        Command::Income(entry(amount, None, None, None, secs)),
    )
    .result_id
    .unwrap()
}

/// A new envelope with no opening allocation; returns its id.
pub fn envelope(fx: &mut Fx, name: &str, mode: FlowMode) -> Uuid {
    run(&mut fx.core, fx.vault, flow_cmd(name, mode, false, 0))
        .result_id
        .unwrap()
}
