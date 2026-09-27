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
    ///
    /// Works the same on a live vault and on one a `DeleteVault` already took
    /// out of the projection ([`Core::deleted_vaults`]); an unknown vault is
    /// `Ok(0)`. Afterwards [`Core::sync_state`] reports zeros for it.
    pub fn forget_vault(&mut self, vault_id: Uuid) -> Result<u32> {
        let tx = self.conn.transaction()?;
        let outbox: i64 = tx.query_row(
            "SELECT COUNT(*) FROM commands
             WHERE vault_id = ?1 AND status = 'applied' AND server_seq IS NULL",
            params![vault_id],
            |r| r.get(0),
        )?;
        // Same two steps as a rebase starts with: the vault row cascades to
        // every projection table, the log has no foreign key.
        tx.execute("DELETE FROM vaults WHERE id = ?1", params![vault_id])?;
        tx.execute(
            "DELETE FROM commands WHERE vault_id = ?1",
            params![vault_id],
        )?;
        tx.commit()?;
        Ok(u32::try_from(outbox).unwrap_or(u32::MAX))
    }
}
