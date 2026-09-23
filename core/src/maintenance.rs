//! Whole-database chores: a consistent copy of the file and dropping a vault
//! this device no longer follows.

use std::path::Path;

use uuid::Uuid;

use crate::{Core, DomainError, Result};

impl Core {
    /// Writes a consistent copy of the whole database to `path` (`VACUUM
    /// INTO`). An existing file at `path` is `AlreadyExists`, never
    /// overwritten.
    pub fn backup_to(&self, path: &Path) -> Result<()> {
        let _ = path;
        Err(DomainError::InvalidCommand(
            "backup_to: not implemented".to_string(),
        ))
    }

    /// Drops a vault from this device: projection, log, outbox and rejected
    /// rows. Nothing is sent to the server; a later pull from 0 would join the
    /// vault again. Returns how many outbox commands were thrown away.
    pub fn forget_vault(&mut self, vault_id: Uuid) -> Result<u32> {
        let _ = vault_id;
        Err(DomainError::InvalidCommand(
            "forget_vault: not implemented".to_string(),
        ))
    }
}
