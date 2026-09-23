//! Whole-database chores: a consistent copy of the file and dropping a vault
//! this device no longer follows.

use std::path::Path;

use rusqlite::params;
use uuid::Uuid;

use crate::{Core, DomainError, Result};

impl Core {
    /// Writes a consistent copy of the whole database to `path` (`VACUUM
    /// INTO`). An existing file at `path` is `AlreadyExists`, never
    /// overwritten.
    ///
    /// The copy is a plain database with the same schema version, so
    /// [`Core::open`] opens it as it is. Anything that stops SQLite from
    /// writing it (a missing directory, no permission, a full disk) is a
    /// `Storage` error.
    pub fn backup_to(&self, path: &Path) -> Result<()> {
        // `symlink_metadata` so that even a dangling link counts as taken.
        if std::fs::symlink_metadata(path).is_ok() {
            return Err(DomainError::AlreadyExists(path.display().to_string()));
        }
        let target = path.to_str().ok_or_else(|| {
            DomainError::Storage(format!("backup path {} is not UTF-8", path.display()))
        })?;
        self.conn.execute("VACUUM INTO ?1", params![target])?;
        Ok(())
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
