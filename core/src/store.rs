//! SQLite connection and schema management.

use std::path::Path;

use rusqlite::Connection;

use crate::{DomainError, Result};

/// Full schema, as created for a new database: already at [`SCHEMA_VERSION`].
const SCHEMA: &str = include_str!("schema.sql");
/// Version 1 databases (before sync) gain the server sequence column.
const MIGRATION_V2: &str = "
ALTER TABLE commands ADD COLUMN server_seq INTEGER;
CREATE UNIQUE INDEX ux_commands_vault_server_seq ON commands(vault_id, server_seq);
";
/// Vault names become labels: an owner may keep two vaults with the same
/// name, so replaying a vault's log never trips over a name another vault
/// took in the meantime (`docs/v2/SYNC.md` §3).
const MIGRATION_V3: &str = "DROP INDEX IF EXISTS ux_vaults_owner_name;";
/// Every step, keyed by the version it leads to: a database at version `v`
/// runs, in order, each step whose key is above `v`.
const MIGRATIONS: &[(i64, &str)] = &[(2, MIGRATION_V2), (3, MIGRATION_V3)];
const SCHEMA_VERSION: i64 = 3;

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

    /// A new database gets [`SCHEMA`], an older one every migration above its
    /// version; either way in one transaction with the new `user_version`, so
    /// a step that fails leaves the file as it was. The pragmas stay outside:
    /// SQLite does not switch the journal mode inside a transaction.
    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let _journal: String =
            conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version == SCHEMA_VERSION {
            return Ok(Self { conn });
        }
        if !(0..SCHEMA_VERSION).contains(&version) {
            return Err(DomainError::Storage(format!(
                "database schema version {version} is newer than this core ({SCHEMA_VERSION})"
            )));
        }
        let tx = conn.transaction()?;
        if version == 0 {
            tx.execute_batch(SCHEMA)?;
        } else {
            for (_, step) in MIGRATIONS.iter().filter(|(target, _)| *target > version) {
                tx.execute_batch(step)?;
            }
        }
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tx.commit()?;
        Ok(Self { conn })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::path::PathBuf;

    use uuid::Uuid;

    use super::*;
    use crate::{CommandEnvelope, Currency};

    fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .map(|name| name.unwrap())
            .any(|name| name == column)
    }

    fn has_index(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1)",
            [name],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn user_version(conn: &Connection) -> i64 {
        conn.pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap()
    }

    fn temp_database() -> PathBuf {
        std::env::temp_dir().join(format!("sparagne-migrate-{}.sqlite", Uuid::now_v7()))
    }

    fn remove_database(path: &Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    fn create_vault(core: &mut Core, name: &str) -> Uuid {
        core.execute(CommandEnvelope::create_vault("alice", name, Currency::Eur))
            .unwrap()
            .command_id
    }

    /// A file as `version` left it, holding alice's vault "Casa": the index on
    /// the owner and the vault name is back, and before version 2 the server
    /// seq column is gone too.
    fn old_database(version: i64) -> (PathBuf, Uuid) {
        let path = temp_database();
        let casa = create_vault(&mut Core::open(&path).unwrap(), "Casa");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE UNIQUE INDEX ux_vaults_owner_name ON vaults(owner_user_id, lower(name));",
        )
        .unwrap();
        if version < 2 {
            conn.execute_batch(
                "DROP INDEX ux_commands_vault_server_seq;
                 ALTER TABLE commands DROP COLUMN server_seq;",
            )
            .unwrap();
        }
        conn.pragma_update(None, "user_version", version).unwrap();
        (path, casa)
    }

    /// Opens an [`old_database`]: the schema reaches the current version, the
    /// vault survives, and its owner may now reuse its name.
    fn migrates_from(version: i64) {
        let (path, casa) = old_database(version);
        let mut core = Core::open(&path).unwrap();
        assert_eq!(user_version(&core.conn), SCHEMA_VERSION);
        assert!(has_column(&core.conn, "commands", "server_seq"));
        assert!(has_index(&core.conn, "ux_commands_vault_server_seq"));
        assert!(!has_index(&core.conn, "ux_vaults_owner_name"));

        let vaults = core.vaults().unwrap();
        assert_eq!(vaults.len(), 1);
        assert_eq!(vaults[0].id, casa);
        assert_eq!(vaults[0].name, "Casa");
        assert_eq!(core.last_seq(casa).unwrap(), 1);

        let twin = create_vault(&mut core, "casa");
        assert_ne!(twin, casa);
        assert_eq!(core.vaults().unwrap().len(), 2);
        drop(core);
        remove_database(&path);
    }

    #[test]
    fn a_new_database_starts_at_the_current_version() {
        let core = Core::open_in_memory().unwrap();
        assert_eq!(user_version(&core.conn), SCHEMA_VERSION);
        assert!(has_column(&core.conn, "commands", "server_seq"));
        assert!(!has_index(&core.conn, "ux_vaults_owner_name"));
    }

    #[test]
    fn a_version_1_database_gains_the_server_seq_column() {
        let path = temp_database();
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
        assert_eq!(user_version(&core.conn), SCHEMA_VERSION);
        assert!(has_index(&core.conn, "ux_commands_vault_server_seq"));
        drop(core);
        remove_database(&path);
    }

    #[test]
    fn a_version_1_database_migrates_to_3() {
        migrates_from(1);
    }

    #[test]
    fn a_version_2_database_migrates_to_3() {
        migrates_from(2);
    }

    #[test]
    fn a_migration_that_fails_halfway_leaves_the_file_as_it_was() {
        let (path, _) = old_database(1);
        {
            // The index version 2 creates already has a namesake, so that step
            // fails after its `ALTER TABLE` went through.
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("CREATE INDEX ux_commands_vault_server_seq ON vaults(name);")
                .unwrap();
        }
        assert!(matches!(Core::open(&path), Err(DomainError::Storage(_))));

        let conn = Connection::open(&path).unwrap();
        assert_eq!(user_version(&conn), 1);
        assert!(!has_column(&conn, "commands", "server_seq"));
        assert!(has_index(&conn, "ux_vaults_owner_name"));
        drop(conn);
        remove_database(&path);
    }

    #[test]
    fn a_newer_database_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        assert!(matches!(Core::init(conn), Err(DomainError::Storage(_))));
    }
}
