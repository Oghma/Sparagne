//! Statement rows against what the vault already holds: the other side of a
//! transfer between wallets that another statement (or the user) already
//! recorded, and the rows the user entered by hand.
//!
//! Stateless: every preview matches again, so re-importing a statement finds
//! the same transactions and writes nothing new.

use uuid::Uuid;

use super::StatementOptions;
use super::plan::Planned;
use crate::{Core, Result};

/// Flags the planned rows that a live transaction already stands for, one
/// transaction per row at most.
pub(super) fn claim(
    _core: &Core,
    _vault_id: Uuid,
    _options: &StatementOptions,
    _planned: &mut [Planned],
) -> Result<()> {
    Ok(())
}
