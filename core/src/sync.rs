//! Sync protocol: wire types shared with the server and the client-side
//! algorithm (outbox, push results, pull integration with rebase).
//!
//! See `docs/v2/SYNC.md`. The wire types are plain serde JSON; the client
//! never serializes commands itself, it asks the core for the push body and
//! hands the server's responses back to it.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{CommandEnvelope, CommandRecord, Core, Currency, DomainError, Result};

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// Body of `POST /vaults/{vault_id}/push`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PushRequest {
    pub commands: Vec<CommandEnvelope>,
}

/// What the server did with one pushed command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PushOutcome {
    Applied { seq: i64, result_id: Option<Uuid> },
    Rejected { code: String, message: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushResult {
    pub command_id: Uuid,
    #[serde(flatten)]
    pub outcome: PushOutcome,
}

/// Response of `POST /vaults/{vault_id}/push`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushResponse {
    pub results: Vec<PushResult>,
    /// Last seq of the vault after the push.
    pub last_seq: i64,
}

/// One confirmed command as the server hands it out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SyncRecord {
    pub envelope: CommandEnvelope,
    /// Server-assigned position in the vault log.
    pub seq: i64,
    pub result_id: Option<Uuid>,
    /// Unix seconds, when the server accepted it.
    pub created_at: i64,
}

/// Response of `GET /vaults/{vault_id}/pull`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PullResponse {
    pub commands: Vec<SyncRecord>,
    /// Last seq of the vault; more to pull while `commands` stops short of it.
    pub last_seq: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    Owner,
    Editor,
    Viewer,
}

impl MemberRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Editor => "editor",
            Self::Viewer => "viewer",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "owner" => Ok(Self::Owner),
            "editor" => Ok(Self::Editor),
            "viewer" => Ok(Self::Viewer),
            other => Err(DomainError::InvalidCommand(format!(
                "unknown role '{other}'"
            ))),
        }
    }

    #[must_use]
    pub const fn can_write(self) -> bool {
        matches!(self, Self::Owner | Self::Editor)
    }
}

/// One row of `GET /vaults`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultSummary {
    pub id: Uuid,
    pub name: String,
    pub currency: Currency,
    pub owner: String,
    pub role: MemberRole,
    pub last_seq: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// Response of register and login.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenResponse {
    pub token: String,
    /// Unix seconds.
    pub expires_at: i64,
    pub username: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberEntry {
    pub username: String,
    pub role: MemberRole,
}

/// Body of `PUT /vaults/{vault_id}/members`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetMemberRequest {
    pub username: String,
    pub role: MemberRole,
}

/// Request-level error body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Client side
// ---------------------------------------------------------------------------

/// Where a vault stands with respect to the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncState {
    /// Highest server seq confirmed locally; 0 when never synced.
    pub last_server_seq: i64,
    /// Applied local commands not yet confirmed.
    pub outbox: u32,
    /// Commands the server refused, kept for the UI until dismissed.
    pub rejected: u32,
}

/// A command the server refused (or that failed during a rebase).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RejectedCommand {
    pub command_id: Uuid,
    pub kind: String,
    pub code: String,
    pub message: String,
}

/// Outcome of a push response or a pull integration.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SyncReport {
    /// Local commands that got their server seq.
    pub confirmed: u32,
    /// Commands from other members applied locally.
    pub received: u32,
    /// Whether the projection was rebuilt.
    pub rebased: bool,
    pub rejected: Vec<RejectedCommand>,
}

fn unimplemented(what: &str) -> DomainError {
    DomainError::InvalidCommand(format!("{what}: not implemented"))
}

impl Core {
    /// Highest applied seq in the vault's local log; 0 for an unknown vault.
    pub fn last_seq(&self, vault_id: Uuid) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM commands WHERE vault_id = ?1 AND status = 'applied'",
            rusqlite::params![vault_id],
            |r| r.get(0),
        )?)
    }

    pub fn sync_state(&self, _vault_id: Uuid) -> Result<SyncState> {
        Err(unimplemented("sync_state"))
    }

    /// Applied local commands without a server seq, in local order.
    pub fn outbox(&self, _vault_id: Uuid) -> Result<Vec<CommandRecord>> {
        Err(unimplemented("outbox"))
    }

    pub fn push_request(&self, _vault_id: Uuid) -> Result<PushRequest> {
        Err(unimplemented("push_request"))
    }

    pub fn apply_push_response(
        &mut self,
        _vault_id: Uuid,
        _response: &PushResponse,
    ) -> Result<SyncReport> {
        Err(unimplemented("apply_push_response"))
    }

    pub fn integrate_pull(
        &mut self,
        _vault_id: Uuid,
        _response: &PullResponse,
    ) -> Result<SyncReport> {
        Err(unimplemented("integrate_pull"))
    }

    /// Rewrites the author of the outbox and rebuilds the projection.
    pub fn relabel_outbox(&mut self, _vault_id: Uuid, _author: &str) -> Result<()> {
        Err(unimplemented("relabel_outbox"))
    }

    pub fn rejected_commands(&self, _vault_id: Uuid) -> Result<Vec<RejectedCommand>> {
        Err(unimplemented("rejected_commands"))
    }

    pub fn dismiss_rejected(&mut self, _vault_id: Uuid, _command_id: Uuid) -> Result<()> {
        Err(unimplemented("dismiss_rejected"))
    }
}
