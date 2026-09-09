//! Engine: wallet, flow and category management commands, plus the read
//! side that supports them (merge preview, alias list, similar names).

use rusqlite::Transaction;
use uuid::Uuid;

use crate::{CommandEnvelope, DomainError, FlowMode, Result};

fn unimplemented(what: &str) -> DomainError {
    DomainError::InvalidCommand(format!("{what}: not implemented"))
}

pub(super) fn rename_wallet(
    _tx: &Transaction<'_>,
    _env: &CommandEnvelope,
    _wallet_id: Uuid,
    _name: &str,
) -> Result<()> {
    Err(unimplemented("rename_wallet"))
}

pub(super) fn set_wallet_archived(
    _tx: &Transaction<'_>,
    _env: &CommandEnvelope,
    _wallet_id: Uuid,
    _archived: bool,
) -> Result<()> {
    Err(unimplemented("set_wallet_archived"))
}

pub(super) fn update_flow(
    _tx: &Transaction<'_>,
    _env: &CommandEnvelope,
    _flow_id: Uuid,
    _name: Option<&str>,
    _mode: Option<FlowMode>,
    _allow_negative: Option<bool>,
) -> Result<()> {
    Err(unimplemented("update_flow"))
}

pub(super) fn set_flow_archived(
    _tx: &Transaction<'_>,
    _env: &CommandEnvelope,
    _flow_id: Uuid,
    _archived: bool,
) -> Result<()> {
    Err(unimplemented("set_flow_archived"))
}

pub(super) fn rename_category(
    _tx: &Transaction<'_>,
    _env: &CommandEnvelope,
    _category_id: Uuid,
    _name: &str,
) -> Result<()> {
    Err(unimplemented("rename_category"))
}

pub(super) fn set_category_archived(
    _tx: &Transaction<'_>,
    _env: &CommandEnvelope,
    _category_id: Uuid,
    _archived: bool,
) -> Result<()> {
    Err(unimplemented("set_category_archived"))
}

/// Returns the alias id (derived from the command id).
pub(super) fn add_alias(
    _tx: &Transaction<'_>,
    _env: &CommandEnvelope,
    _category_id: Uuid,
    _alias: &str,
) -> Result<Uuid> {
    Err(unimplemented("add_alias"))
}

pub(super) fn remove_alias(
    _tx: &Transaction<'_>,
    _env: &CommandEnvelope,
    _category_id: Uuid,
    _alias: &str,
) -> Result<()> {
    Err(unimplemented("remove_alias"))
}

pub(super) fn merge_category(
    _tx: &Transaction<'_>,
    _env: &CommandEnvelope,
    _source_id: Uuid,
    _target_id: Uuid,
) -> Result<()> {
    Err(unimplemented("merge_category"))
}
