//! `server.sqlite`: accounts, tokens and vault memberships.
//!
//! The vault log and its projection live in the other database, opened as a
//! [`sparagne_core::Core`]; this one only knows who may touch them.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use sparagne_core::sync::{MemberEntry, MemberRole};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};

const SCHEMA_V1: &str = "\
CREATE TABLE users (
    id            BLOB PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at    INTEGER NOT NULL
);

CREATE TABLE tokens (
    hash       TEXT PRIMARY KEY,
    user_id    BLOB NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX ix_tokens_user ON tokens(user_id);

CREATE TABLE vault_memberships (
    vault_id   BLOB NOT NULL,
    user_id    BLOB NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (vault_id, user_id)
);
CREATE INDEX ix_memberships_user ON vault_memberships(user_id);
";
const SCHEMA_VERSION: i64 = 1;

/// An account as the API sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub id: Uuid,
    pub username: String,
}

/// Handle to the server's own database.
pub struct ServerDb {
    conn: Connection,
}

impl std::fmt::Debug for ServerDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerDb").finish_non_exhaustive()
    }
}

impl ServerDb {
    /// Open (or create) the file and migrate it.
    pub fn open(path: impl AsRef<Path>) -> ApiResult<Self> {
        Self::init(Connection::open(path)?)
    }

    /// Fresh in-memory database, for tests.
    pub fn open_in_memory() -> ApiResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> ApiResult<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let _journal: String =
            conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version < SCHEMA_VERSION {
            conn.execute_batch(SCHEMA_V1)?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(Self { conn })
    }

    // -- Users --------------------------------------------------------------

    /// Inserts an account; the username is unique.
    pub fn insert_user(&self, username: &str, password_hash: &str, now: i64) -> ApiResult<User> {
        let id = Uuid::now_v7();
        let changed = self.conn.execute(
            "INSERT OR IGNORE INTO users (id, username, password_hash, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, username, password_hash, now],
        )?;
        if changed == 0 {
            return Err(ApiError::already_exists("username already taken"));
        }
        Ok(User {
            id,
            username: username.to_string(),
        })
    }

    /// The account and its password hash, by username.
    pub fn user_with_hash(&self, username: &str) -> ApiResult<Option<(User, String)>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, username, password_hash FROM users WHERE username = ?1",
                params![username],
                |r| {
                    Ok((
                        r.get::<_, Uuid>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        Ok(row.map(|(id, username, hash)| (User { id, username }, hash)))
    }

    /// The account id of a username, when it exists.
    pub fn user_by_username(&self, username: &str) -> ApiResult<Option<User>> {
        Ok(self.user_with_hash(username)?.map(|(user, _)| user))
    }

    /// Replaces an account's password hash.
    pub fn set_password_hash(&self, user_id: Uuid, password_hash: &str) -> ApiResult<()> {
        self.conn.execute(
            "UPDATE users SET password_hash = ?2 WHERE id = ?1",
            params![user_id, password_hash],
        )?;
        Ok(())
    }

    /// Every account with its creation time (unix seconds), by username.
    pub fn users(&self) -> ApiResult<Vec<(String, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT username, created_at FROM users ORDER BY username")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    // -- Tokens -------------------------------------------------------------

    /// Stores a token by its sha256 hex digest.
    pub fn insert_token(
        &self,
        token_hash: &str,
        user_id: Uuid,
        now: i64,
        expires_at: i64,
    ) -> ApiResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO tokens (hash, user_id, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![token_hash, user_id, now, expires_at],
        )?;
        Ok(())
    }

    /// The account a still valid token belongs to. Expired rows are dropped.
    pub fn user_for_token(&self, token_hash: &str, now: i64) -> ApiResult<Option<User>> {
        let row = self
            .conn
            .query_row(
                "SELECT u.id, u.username, t.expires_at
                 FROM tokens t JOIN users u ON u.id = t.user_id
                 WHERE t.hash = ?1",
                params![token_hash],
                |r| {
                    Ok((
                        r.get::<_, Uuid>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        match row {
            Some((id, username, expires_at)) if expires_at > now => Ok(Some(User { id, username })),
            Some(_) => {
                self.delete_token(token_hash)?;
                Ok(None)
            }
            None => Ok(None),
        }
    }

    pub fn delete_token(&self, token_hash: &str) -> ApiResult<()> {
        self.conn
            .execute("DELETE FROM tokens WHERE hash = ?1", params![token_hash])?;
        Ok(())
    }

    /// Revokes every token of an account but `keep` (the digest of the one
    /// in use, when there is one). Returns how many were revoked.
    pub fn delete_tokens_of_user(&self, user_id: Uuid, keep: Option<&str>) -> ApiResult<usize> {
        Ok(self.conn.execute(
            "DELETE FROM tokens WHERE user_id = ?1 AND hash IS NOT ?2",
            params![user_id, keep],
        )?)
    }

    /// Drops every token that is already past its expiry.
    pub fn purge_expired_tokens(&self, now: i64) -> ApiResult<()> {
        self.conn
            .execute("DELETE FROM tokens WHERE expires_at <= ?1", params![now])?;
        Ok(())
    }

    // -- Memberships --------------------------------------------------------

    /// The role of an account on a vault, `None` when it is not a member.
    pub fn membership(&self, vault_id: Uuid, user_id: Uuid) -> ApiResult<Option<MemberRole>> {
        let role: Option<String> = self
            .conn
            .query_row(
                "SELECT role FROM vault_memberships WHERE vault_id = ?1 AND user_id = ?2",
                params![vault_id, user_id],
                |r| r.get(0),
            )
            .optional()?;
        role.map(|value| MemberRole::parse(&value).map_err(ApiError::from))
            .transpose()
    }

    /// Adds a membership, leaving an existing one untouched.
    pub fn insert_membership(
        &self,
        vault_id: Uuid,
        user_id: Uuid,
        role: MemberRole,
        now: i64,
    ) -> ApiResult<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO vault_memberships (vault_id, user_id, role, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![vault_id, user_id, role.as_str(), now],
        )?;
        Ok(())
    }

    /// Adds or changes a membership.
    pub fn set_membership(
        &self,
        vault_id: Uuid,
        user_id: Uuid,
        role: MemberRole,
        now: i64,
    ) -> ApiResult<()> {
        self.conn.execute(
            "INSERT INTO vault_memberships (vault_id, user_id, role, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(vault_id, user_id) DO UPDATE SET role = excluded.role",
            params![vault_id, user_id, role.as_str(), now],
        )?;
        Ok(())
    }

    /// `true` when a membership was actually removed.
    pub fn remove_membership(&self, vault_id: Uuid, user_id: Uuid) -> ApiResult<bool> {
        let changed = self.conn.execute(
            "DELETE FROM vault_memberships WHERE vault_id = ?1 AND user_id = ?2",
            params![vault_id, user_id],
        )?;
        Ok(changed > 0)
    }

    /// Every vault an account is a member of, with its role.
    pub fn memberships_of_user(&self, user_id: Uuid) -> ApiResult<Vec<(Uuid, MemberRole)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT vault_id, role FROM vault_memberships WHERE user_id = ?1")?;
        let rows = stmt
            .query_map(params![user_id], |r| {
                Ok((r.get::<_, Uuid>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(vault_id, role)| Ok((vault_id, MemberRole::parse(&role)?)))
            .collect()
    }

    /// Members of a vault: the owner first, then editors, then viewers.
    pub fn members_of_vault(&self, vault_id: Uuid) -> ApiResult<Vec<MemberEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT u.username, m.role
             FROM vault_memberships m JOIN users u ON u.id = m.user_id
             WHERE m.vault_id = ?1
             ORDER BY CASE m.role WHEN 'owner' THEN 0 WHEN 'editor' THEN 1 ELSE 2 END,
                      lower(u.username)",
        )?;
        let rows = stmt
            .query_map(params![vault_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(username, role)| {
                Ok(MemberEntry {
                    username,
                    role: MemberRole::parse(&role)?,
                })
            })
            .collect()
    }
}
