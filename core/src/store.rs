//! SQLite connection and schema management.

use std::path::Path;

use rusqlite::Connection;

use crate::{DomainError, Result};

/// Full schema, as created for a new database.
const SCHEMA: &str = include_str!("schema.sql");
/// Version 1 databases (before sync) gain the server sequence column.
const MIGRATION_V2: &str = "
ALTER TABLE commands ADD COLUMN server_seq INTEGER;
CREATE UNIQUE INDEX ux_commands_vault_server_seq ON commands(vault_id, server_seq);
";
const SCHEMA_VERSION: i64 = 2;

/// Handle to one local database (one file per account, many vaults).
pub struct Core {
    pub(crate) conn: Connection,
}

impl std::fmt::Debug for Core {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Core").finish_non_exhaustive()
    }
}

impl Core {
    /// Open (or create) a database file and migrate it.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    /// Fresh in-memory database, for tests and replay.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let _journal: String =
            conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        match version {
            0 => conn.execute_batch(SCHEMA)?,
            1 => conn.execute_batch(MIGRATION_V2)?,
            v if v == SCHEMA_VERSION => {}
            v => {
                return Err(DomainError::Storage(format!(
                    "database schema version {v} is newer than this core ({SCHEMA_VERSION})"
                )));
            }
        }
        if version < SCHEMA_VERSION {
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(Self { conn })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .map(|name| name.unwrap())
            .any(|name| name == column)
    }

    #[test]
    fn a_version_1_database_gains_the_server_seq_column() {
        let path =
            std::env::temp_dir().join(format!("sparagne-migrate-{}.sqlite", uuid::Uuid::now_v7()));
        drop(Core::open(&path).unwrap());
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "DROP INDEX ux_commands_vault_server_seq;
                 ALTER TABLE commands DROP COLUMN server_seq;
                 PRAGMA user_version = 1;",
            )
            .unwrap();
            assert!(!has_column(&conn, "commands", "server_seq"));
        }
        let core = Core::open(&path).unwrap();
        assert!(has_column(&core.conn, "commands", "server_seq"));
        let version: i64 = core
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        let index: bool = core
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'ux_commands_vault_server_seq')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(index);
        drop(core);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    #[test]
    fn a_newer_database_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        assert!(matches!(Core::init(conn), Err(DomainError::Storage(_))));
    }
}
