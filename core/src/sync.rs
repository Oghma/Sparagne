//! Sync protocol: wire types shared with the server and the client-side
//! algorithm (outbox, push results, pull integration with rebase).
//!
//! See `docs/v2/SYNC.md`. The wire types are plain serde JSON; the client
//! never serializes commands itself, it asks the core for the push body and
//! hands the server's responses back to it.

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    CommandEnvelope, CommandRecord, Core, Currency, DomainError, Receipt, Result,
    engine::{LogRow, apply_envelope, log_row, try_apply_envelope},
    query::log_records,
};

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
    Applied {
        seq: i64,
        result_id: Option<Uuid>,
    },
    Rejected {
        code: String,
        message: String,
        /// What the refusal is about, for a client to show without parsing
        /// `message`, which is English prose: for `not_a_member`, the name
        /// the server refused. Absent for the other codes, and from a server
        /// older than the field; left out of the JSON when absent, so an
        /// older client reads the body as before.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
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
// Server side
// ---------------------------------------------------------------------------

impl Core {
    /// The server side of a push: apply in order, one outcome per command. A
    /// domain failure rejects that command and the batch goes on; a storage
    /// failure aborts. Idempotent: a known command id answers `applied` with
    /// its original seq.
    pub fn serve_push(&mut self, vault_id: Uuid, request: &PushRequest) -> Result<PushResponse> {
        if let Some(stray) = request
            .commands
            .iter()
            .find(|envelope| envelope.vault_id != vault_id)
        {
            return Err(DomainError::InvalidCommand(format!(
                "push: command {} belongs to vault {}",
                stray.id, stray.vault_id
            )));
        }
        let mut results = Vec::with_capacity(request.commands.len());
        for envelope in &request.commands {
            let outcome = match self.execute(envelope.clone()) {
                Ok(receipt) => PushOutcome::Applied {
                    seq: receipt.seq,
                    result_id: receipt.result_id,
                },
                Err(DomainError::Storage(detail)) => return Err(DomainError::Storage(detail)),
                Err(err) => PushOutcome::Rejected {
                    code: err.code().to_string(),
                    message: err.to_string(),
                    detail: None,
                },
            };
            results.push(PushResult {
                command_id: envelope.id,
                outcome,
            });
        }
        Ok(PushResponse {
            results,
            last_seq: self.last_seq(vault_id)?,
        })
    }

    /// The receipt [`Core::execute`] answers for a command already in the log,
    /// `None` for one the log does not hold. A server that refuses commands
    /// for reasons the log does not record (who is a member) asks first, so
    /// that pushing the same command twice keeps answering the same seq.
    pub fn receipt(&self, command_id: Uuid) -> Result<Option<Receipt>> {
        let row = self
            .conn
            .query_row(
                "SELECT seq, result_id FROM commands WHERE id = ?1",
                params![command_id],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<Uuid>>(1)?)),
            )
            .optional()?;
        Ok(row.map(|(seq, result_id)| Receipt {
            command_id,
            seq,
            result_id,
            deduplicated: true,
        }))
    }

    /// The server side of a pull: applied commands with `seq > since`, at
    /// most `limit`, plus the vault's last seq.
    pub fn serve_pull(&self, vault_id: Uuid, since: i64, limit: usize) -> Result<PullResponse> {
        let mut records = self.commands_since(vault_id, since)?;
        records.truncate(limit);
        Ok(PullResponse {
            commands: records
                .into_iter()
                .map(|record| SyncRecord {
                    envelope: record.envelope,
                    seq: record.seq,
                    result_id: record.result_id,
                    created_at: record.created_at,
                })
                .collect(),
            last_seq: self.last_seq(vault_id)?,
        })
    }

    pub fn serve_push_json(&mut self, vault_id: Uuid, json: &str) -> Result<String> {
        let request: PushRequest = from_json(json, "push request")?;
        Ok(serde_json::to_string(
            &self.serve_push(vault_id, &request)?,
        )?)
    }

    pub fn serve_pull_json(&self, vault_id: Uuid, since: i64, limit: usize) -> Result<String> {
        Ok(serde_json::to_string(
            &self.serve_pull(vault_id, since, limit)?,
        )?)
    }
}

// ---------------------------------------------------------------------------
// Client side
// ---------------------------------------------------------------------------

/// Where a vault stands with respect to the server.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SyncState {
    /// What to pull from: the highest server seq this database holds with no
    /// hole before it. 0 when never synced.
    pub last_server_seq: i64,
    /// Applied local commands not yet confirmed.
    pub outbox: u32,
    /// Commands the server refused, kept for the UI until dismissed.
    pub rejected: u32,
}

/// A command the server refused (or that failed during a rebase).
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RejectedCommand {
    pub command_id: Uuid,
    pub kind: String,
    pub code: String,
    pub message: String,
    /// The server's `detail` for the refusal: for `not_a_member`, the name
    /// it refused. `None` when the server sent none, and for a refusal
    /// made here (a rebase).
    #[uniffi(default = None)]
    pub detail: Option<String>,
}

/// Outcome of a push response or a pull integration.
#[derive(Clone, Debug, PartialEq, Eq, Default, uniffi::Record)]
pub struct SyncReport {
    /// Local commands that got their server seq.
    pub confirmed: u32,
    /// Commands from other members applied locally.
    pub received: u32,
    /// Whether the projection was rebuilt.
    pub rebased: bool,
    pub rejected: Vec<RejectedCommand>,
    /// The vault's last seq as the server reported it in the body this report
    /// comes from. The app never reads it out of the JSON itself.
    pub server_last_seq: i64,
    /// `true` when the server holds seqs past the local watermark, so another
    /// page is waiting.
    pub has_more: bool,
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

    /// Highest applied seq of every vault at once, for a server listing the
    /// vaults of one account without a query per vault. Vaults whose log is
    /// empty are absent.
    pub fn last_seqs(&self) -> Result<Vec<(Uuid, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT vault_id, MAX(seq) FROM commands
             WHERE status = 'applied' GROUP BY vault_id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, Uuid>(0)?, r.get::<_, i64>(1)?)))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Vaults the log still knows and the projection no longer holds: the ones
    /// a `DeleteVault` removed. Their outbox still has to reach the server,
    /// the deletion itself first of all, which is why the app pushes them
    /// alongside the live vaults (`docs/v2/SYNC.md` §4 point 6).
    pub fn deleted_vaults(&self) -> Result<Vec<Uuid>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT vault_id FROM commands
             WHERE vault_id NOT IN (SELECT id FROM vaults) ORDER BY vault_id",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, Uuid>(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Where the vault stands with the server. A vault this database has never
    /// seen reports all zeros rather than failing: the join flow asks before
    /// the first pull creates it (`docs/v2/SYNC.md` §4 point 3).
    pub fn sync_state(&self, vault_id: Uuid) -> Result<SyncState> {
        let (outbox, rejected) = self.conn.query_row(
            "SELECT COUNT(*) FILTER (WHERE status = 'applied' AND server_seq IS NULL),
                    COUNT(*) FILTER (WHERE status = 'rejected')
             FROM commands WHERE vault_id = ?1",
            params![vault_id],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        Ok(SyncState {
            last_server_seq: last_server_seq(&self.conn, vault_id)?,
            outbox: count(outbox),
            rejected: count(rejected),
        })
    }

    /// Applied local commands without a server seq, in local order.
    pub fn outbox(&self, vault_id: Uuid) -> Result<Vec<CommandRecord>> {
        outbox_records(&self.conn, vault_id)
    }

    /// The body of `POST /vaults/{id}/push`: the oldest `limit` commands of
    /// the outbox, in local order.
    ///
    /// A batch is capped because the server refuses an oversized body; the
    /// caller pushes again until [`Self::sync_state`] reports an empty outbox.
    pub fn push_request(&self, vault_id: Uuid, limit: usize) -> Result<PushRequest> {
        let mut commands: Vec<CommandEnvelope> = self
            .outbox(vault_id)?
            .into_iter()
            .map(|record| record.envelope)
            .collect();
        commands.truncate(limit);
        Ok(PushRequest { commands })
    }

    /// Records what the server did with a push.
    ///
    /// `applied` stamps the server seq on the local row; a result for a command
    /// this database does not have, or already confirmed, is ignored. One or
    /// more `rejected` results mark those rows and rebuild the projection
    /// without them, which may in turn reject the commands that depended on
    /// them.
    pub fn apply_push_response(
        &mut self,
        vault_id: Uuid,
        response: &PushResponse,
    ) -> Result<SyncReport> {
        let tx = self.conn.transaction()?;
        let mut report = SyncReport::default();
        let mut refused: Vec<(Uuid, Reason)> = Vec::new();
        for result in &response.results {
            match &result.outcome {
                PushOutcome::Applied { seq, .. } => {
                    let updated = tx.execute(
                        "UPDATE commands SET server_seq = ?1
                         WHERE id = ?2 AND vault_id = ?3
                           AND status = 'applied' AND server_seq IS NULL",
                        params![seq, result.command_id, vault_id],
                    )?;
                    if updated > 0 {
                        report.confirmed += 1;
                    }
                }
                PushOutcome::Rejected {
                    code,
                    message,
                    detail,
                } => {
                    if is_outbox(&tx, vault_id, result.command_id)? {
                        let reason = Reason {
                            code: code.clone(),
                            message: message.clone(),
                            detail: detail.clone(),
                        };
                        refused.push((result.command_id, reason));
                    }
                }
            }
        }
        if refused.is_empty() {
            tx.commit()?;
            return self.stamp(vault_id, report, response.last_seq);
        }

        let dropped: HashSet<Uuid> = refused.iter().map(|(id, _)| *id).collect();
        let ordered = planned(confirmed_records(&tx, vault_id)?);
        let outbox = outbox_records(&tx, vault_id)?;
        let retry: Vec<Planned> = outbox
            .iter()
            .filter(|record| !dropped.contains(&record.envelope.id))
            .map(|record| Planned {
                envelope: record.envelope.clone(),
                server_seq: None,
                created_at: record.created_at,
            })
            .collect();
        let mut keep = rejected_rows(&tx, vault_id)?;
        for (id, reason) in refused {
            let Some(record) = outbox.iter().find(|record| record.envelope.id == id) else {
                continue;
            };
            keep.push(RejectedRow {
                envelope: record.envelope.clone(),
                created_at: record.created_at,
                reason: reason.stored(),
            });
            report
                .rejected
                .push(reason.rejected(id, record.envelope.command.kind_name()));
        }

        report
            .rejected
            .extend(rebuild(&tx, vault_id, &ordered, &retry, keep)?);
        report.rebased = true;
        tx.commit()?;
        self.stamp(vault_id, report, response.last_seq)
    }

    /// Folds a pull into the local log.
    ///
    /// Records at or below the last known server seq are ignored. When what is
    /// left is exactly the head of the outbox the commands only need their
    /// server seq (fast path, the projection does not move). Otherwise the
    /// vault is rebuilt: confirmed and incoming commands in server order, then
    /// the rest of the outbox in local order. A confirmed or incoming command
    /// that no longer applies is a divergence and nothing is written.
    pub fn integrate_pull(
        &mut self,
        vault_id: Uuid,
        response: &PullResponse,
    ) -> Result<SyncReport> {
        let known = last_server_seq(&self.conn, vault_id)?;
        let mut incoming: Vec<&SyncRecord> = response
            .commands
            .iter()
            .filter(|record| record.seq > known)
            .collect();
        incoming.sort_by_key(|record| record.seq);
        if incoming.is_empty() {
            return self.stamp(vault_id, SyncReport::default(), response.last_seq);
        }
        if let Some(stray) = incoming
            .iter()
            .find(|record| record.envelope.vault_id != vault_id)
        {
            return Err(DomainError::InvalidCommand(format!(
                "pull: command {} belongs to vault {}",
                stray.envelope.id, stray.envelope.vault_id
            )));
        }

        let outbox = self.outbox(vault_id)?;
        if incoming.len() <= outbox.len()
            && incoming
                .iter()
                .zip(&outbox)
                .all(|(record, local)| record.envelope.id == local.envelope.id)
        {
            let tx = self.conn.transaction()?;
            for (record, local) in incoming.iter().zip(&outbox) {
                tx.execute(
                    "UPDATE commands SET server_seq = ?1 WHERE id = ?2 AND vault_id = ?3",
                    params![record.seq, local.envelope.id, vault_id],
                )?;
            }
            tx.commit()?;
            let report = SyncReport {
                confirmed: count_usize(incoming.len()),
                ..SyncReport::default()
            };
            return self.stamp(vault_id, report, response.last_seq);
        }

        let tx = self.conn.transaction()?;
        let mut report = SyncReport::default();
        let local: HashMap<Uuid, Option<i64>> =
            log_records(&tx, "vault_id = ?1", params![vault_id])?
                .into_iter()
                .map(|record| (record.envelope.id, record.server_seq))
                .collect();
        for record in &incoming {
            match local.get(&record.envelope.id) {
                // A command of ours the server has now taken.
                Some(None) => report.confirmed += 1,
                // Already confirmed under another seq: keep the first one.
                Some(Some(_)) => {}
                None => report.received += 1,
            }
        }

        let mut ordered = planned(confirmed_records(&tx, vault_id)?);
        ordered.extend(incoming.iter().map(|record| Planned {
            envelope: record.envelope.clone(),
            server_seq: Some(record.seq),
            created_at: record.created_at,
        }));
        ordered.sort_by_key(|item| item.server_seq);
        let mut seen = HashSet::new();
        ordered.retain(|item| seen.insert(item.envelope.id));
        let taken: HashSet<Uuid> = seen;

        let retry: Vec<Planned> = planned(outbox)
            .into_iter()
            .filter(|item| !taken.contains(&item.envelope.id))
            .collect();
        let keep: Vec<RejectedRow> = rejected_rows(&tx, vault_id)?
            .into_iter()
            .filter(|row| !taken.contains(&row.envelope.id))
            .collect();

        report.rejected = rebuild(&tx, vault_id, &ordered, &retry, keep)?;
        report.rebased = true;
        tx.commit()?;
        self.stamp(vault_id, report, response.last_seq)
    }

    /// Fills in what the report says about the server: the last seq the body
    /// carried, and whether it sits past the contiguous local watermark, which
    /// is how the app knows another page is waiting.
    fn stamp(
        &self,
        vault_id: Uuid,
        mut report: SyncReport,
        server_last_seq: i64,
    ) -> Result<SyncReport> {
        report.server_last_seq = server_last_seq;
        report.has_more = server_last_seq > last_server_seq(&self.conn, vault_id)?;
        Ok(report)
    }

    /// Rewrites the author of the outbox and rebuilds the projection, so that
    /// `created_by` and `owner_user_id` follow the account the app just logged
    /// into.
    ///
    /// A person or owner the outbox names by one of its old author names is
    /// the same user, so it becomes the username too: left as it was, the
    /// server would refuse it as somebody who is not a member. It becomes the
    /// username rather than nothing, so a patch that only changes the person
    /// keeps a field to carry. Other names stay as they are, and so do the
    /// rejected rows, which never reach the server again.
    pub fn relabel_outbox(&mut self, vault_id: Uuid, author: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        let outbox = outbox_records(&tx, vault_id)?;
        if outbox.iter().all(|record| record.envelope.author == author) {
            tx.commit()?;
            return Ok(());
        }
        let old_names: HashSet<String> = outbox
            .iter()
            .map(|record| record.envelope.author.clone())
            .filter(|name| name != author)
            .collect();
        let ordered = planned(confirmed_records(&tx, vault_id)?);
        let retry: Vec<Planned> = planned(outbox)
            .into_iter()
            .map(|mut item| {
                for old in &old_names {
                    item.envelope.command.rename_person(old, author);
                }
                item.envelope.author = author.to_string();
                item
            })
            .collect();
        let keep = rejected_rows(&tx, vault_id)?;
        rebuild(&tx, vault_id, &ordered, &retry, keep)?;
        tx.commit()?;
        Ok(())
    }

    /// Commands the server (or a rebase) refused, oldest first.
    pub fn rejected_commands(&self, vault_id: Uuid) -> Result<Vec<RejectedCommand>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, rejection FROM commands
             WHERE vault_id = ?1 AND status = 'rejected' ORDER BY seq",
        )?;
        let rows = stmt.query_map(params![vault_id], |r| {
            Ok((
                r.get::<_, Uuid>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (command_id, kind, rejection) = row?;
            out.push(
                Reason::parse(rejection.as_deref().unwrap_or_default()).rejected(command_id, &kind),
            );
        }
        Ok(out)
    }

    /// Forgets one rejected command. Nothing happens when it is already gone.
    pub fn dismiss_rejected(&mut self, vault_id: Uuid, command_id: Uuid) -> Result<()> {
        self.conn.execute(
            "DELETE FROM commands WHERE vault_id = ?1 AND id = ?2 AND status = 'rejected'",
            params![vault_id, command_id],
        )?;
        Ok(())
    }

    // -- JSON entry points, for the FFI -------------------------------------

    /// [`Self::push_request`] as the JSON body to POST.
    pub fn push_request_json(&self, vault_id: Uuid, limit: usize) -> Result<String> {
        Ok(serde_json::to_string(&self.push_request(vault_id, limit)?)?)
    }

    /// [`Self::apply_push_response`] on a raw push response body.
    pub fn apply_push_response_json(&mut self, vault_id: Uuid, json: &str) -> Result<SyncReport> {
        self.apply_push_response(vault_id, &from_json(json, "push response")?)
    }

    /// [`Self::integrate_pull`] on a raw pull response body.
    pub fn integrate_pull_json(&mut self, vault_id: Uuid, json: &str) -> Result<SyncReport> {
        self.integrate_pull(vault_id, &from_json(json, "pull response")?)
    }
}

// ---------------------------------------------------------------------------
// Rebase
// ---------------------------------------------------------------------------

/// One command with the position a rebuild will give it.
#[derive(Clone, Debug)]
struct Planned {
    envelope: CommandEnvelope,
    server_seq: Option<i64>,
    /// Original wall clock, so a rebuilt projection is byte-identical.
    created_at: i64,
}

/// A rejected row, kept across a rebuild for the UI.
#[derive(Clone, Debug)]
struct RejectedRow {
    envelope: CommandEnvelope,
    created_at: i64,
    /// What [`Reason::stored`] wrote, kept as it is.
    reason: String,
}

/// Why a command was refused, as a rejected row keeps it in the log's
/// `rejection` column and hands it back as a [`RejectedCommand`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Reason {
    code: String,
    message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

impl Reason {
    /// The text the column holds: `<code>: <message>`, as every rejected row
    /// written before the detail existed, or a JSON object when there is a
    /// detail, which a sentence could not carry apart from its words. No
    /// code starts with `{`, so [`Self::parse`] tells the two apart.
    fn stored(&self) -> String {
        let plain = || format!("{}: {}", self.code, self.message);
        if self.detail.is_none() {
            return plain();
        }
        serde_json::to_string(self).unwrap_or_else(|_| plain())
    }

    /// Reads back what [`Self::stored`] wrote, either form.
    fn parse(stored: &str) -> Self {
        if stored.starts_with('{')
            && let Ok(reason) = serde_json::from_str(stored)
        {
            return reason;
        }
        let (code, message) = split_reason(stored);
        Self {
            code,
            message,
            detail: None,
        }
    }

    fn rejected(self, command_id: Uuid, kind: &str) -> RejectedCommand {
        RejectedCommand {
            command_id,
            kind: kind.to_string(),
            code: self.code,
            message: self.message,
            detail: self.detail,
        }
    }
}

/// Throws away a vault's projection and log and replays it.
///
/// `ordered` (confirmed plus incoming, in server order) must apply: a failure
/// there means the server and this client disagree on the past, so the caller
/// gets a `storage` error and the surrounding transaction is rolled back.
/// `retry` is the outbox, re-executed in local order; whatever no longer holds
/// becomes a rejected row and comes back in the return value. `rejected` are
/// the rows to keep as they are.
fn rebuild(
    tx: &Transaction<'_>,
    vault_id: Uuid,
    ordered: &[Planned],
    retry: &[Planned],
    rejected: Vec<RejectedRow>,
) -> Result<Vec<RejectedCommand>> {
    // Deleting the vault cascades to every projection table; the log has no
    // foreign key, so it goes explicitly.
    tx.execute("DELETE FROM vaults WHERE id = ?1", params![vault_id])?;
    tx.execute(
        "DELETE FROM commands WHERE vault_id = ?1",
        params![vault_id],
    )?;

    let mut seq = 0;
    for item in ordered {
        seq += 1;
        apply_envelope(tx, &item.envelope, seq, item.server_seq, item.created_at).map_err(
            |err| {
                DomainError::Storage(format!(
                    "sync divergence: {} {} no longer applies: {err}",
                    item.envelope.command.kind_name(),
                    item.envelope.id
                ))
            },
        )?;
    }

    let mut kept = rejected;
    let mut fresh = Vec::new();
    for item in retry {
        seq += 1;
        if let Some(err) = try_apply_envelope(tx, &item.envelope, seq, None, item.created_at)? {
            let reason = Reason {
                code: err.code().to_string(),
                message: err.to_string(),
                detail: None,
            };
            kept.push(RejectedRow {
                envelope: item.envelope.clone(),
                created_at: item.created_at,
                reason: reason.stored(),
            });
            fresh.push(reason.rejected(item.envelope.id, item.envelope.command.kind_name()));
        }
    }

    // Rejected rows carry no projection, so they sit at the tail of the log
    // where they cannot shift anyone else's position.
    for row in kept {
        seq += 1;
        log_row(
            tx,
            &row.envelope,
            seq,
            None,
            row.created_at,
            LogRow::Rejected(row.reason),
        )?;
    }
    Ok(fresh)
}

// ---------------------------------------------------------------------------
// Log reads
// ---------------------------------------------------------------------------

/// How far the vault is synced: the highest server seq for which this database
/// holds every earlier one too.
///
/// A push confirms the client's own commands, which may sit past a gap when
/// another member wrote in between; pulling from the raw maximum would step
/// over the commands in the gap, so the watermark stops at the first hole.
fn last_server_seq(conn: &Connection, vault_id: Uuid) -> Result<i64> {
    let mut stmt = conn.prepare(
        "SELECT server_seq FROM commands
         WHERE vault_id = ?1 AND status = 'applied' AND server_seq IS NOT NULL
         ORDER BY server_seq",
    )?;
    let mut watermark = 0;
    for seq in stmt.query_map(params![vault_id], |r| r.get::<_, i64>(0))? {
        if seq? != watermark + 1 {
            break;
        }
        watermark += 1;
    }
    Ok(watermark)
}

/// Confirmed commands in server order.
fn confirmed_records(conn: &Connection, vault_id: Uuid) -> Result<Vec<CommandRecord>> {
    let mut records = log_records(
        conn,
        "vault_id = ?1 AND status = 'applied' AND server_seq IS NOT NULL",
        params![vault_id],
    )?;
    records.sort_by_key(|record| record.server_seq);
    Ok(records)
}

fn outbox_records(conn: &Connection, vault_id: Uuid) -> Result<Vec<CommandRecord>> {
    log_records(
        conn,
        "vault_id = ?1 AND status = 'applied' AND server_seq IS NULL",
        params![vault_id],
    )
}

fn rejected_rows(conn: &Connection, vault_id: Uuid) -> Result<Vec<RejectedRow>> {
    let reasons: HashMap<Uuid, String> = {
        let mut stmt = conn.prepare(
            "SELECT id, COALESCE(rejection, '') FROM commands
             WHERE vault_id = ?1 AND status = 'rejected'",
        )?;
        let rows = stmt.query_map(params![vault_id], |r| {
            Ok((r.get::<_, Uuid>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    Ok(log_records(
        conn,
        "vault_id = ?1 AND status = 'rejected'",
        params![vault_id],
    )?
    .into_iter()
    .map(|record| RejectedRow {
        reason: reasons
            .get(&record.envelope.id)
            .cloned()
            .unwrap_or_default(),
        envelope: record.envelope,
        created_at: record.created_at,
    })
    .collect())
}

fn is_outbox(conn: &Connection, vault_id: Uuid, command_id: Uuid) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM commands
         WHERE id = ?1 AND vault_id = ?2 AND status = 'applied' AND server_seq IS NULL)",
        params![command_id, vault_id],
        |r| r.get(0),
    )?)
}

// ---------------------------------------------------------------------------
// Refusing the whole outbox
// ---------------------------------------------------------------------------

impl Core {
    /// Turns every command still in the outbox into a rejected row with
    /// `code` and `message`, as if the server had refused each of them, and
    /// rebuilds the projection without them. The app calls it when a push
    /// comes back `403` (a viewer wrote locally), so the vault can keep
    /// pulling instead of retrying forever.
    ///
    /// It is [`Self::apply_push_response`] on a response that refuses every
    /// outbox command, so the report and the rebuild are the ones a real
    /// refusal gives. The response carries the highest server seq this
    /// database knows as the server's last seq; with an empty outbox nothing
    /// changes and the report only carries that seq.
    pub fn reject_outbox(
        &mut self,
        vault_id: Uuid,
        code: &str,
        message: &str,
    ) -> Result<SyncReport> {
        let results = outbox_records(&self.conn, vault_id)?
            .into_iter()
            .map(|record| PushResult {
                command_id: record.envelope.id,
                outcome: PushOutcome::Rejected {
                    code: code.to_string(),
                    message: message.to_string(),
                    detail: None,
                },
            })
            .collect();
        let response = PushResponse {
            results,
            last_seq: known_server_seq(&self.conn, vault_id)?,
        };
        self.apply_push_response(vault_id, &response)
    }
}

/// The highest server seq the log holds for the vault, holes or not: the last
/// seq the server is known to have reached. 0 when never synced.
fn known_server_seq(conn: &Connection, vault_id: Uuid) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COALESCE(MAX(server_seq), 0) FROM commands
         WHERE vault_id = ?1 AND status = 'applied'",
        params![vault_id],
        |r| r.get(0),
    )?)
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn planned(records: Vec<CommandRecord>) -> Vec<Planned> {
    records
        .into_iter()
        .map(|record| Planned {
            envelope: record.envelope,
            server_seq: record.server_seq,
            created_at: record.created_at,
        })
        .collect()
}

fn from_json<T: serde::de::DeserializeOwned>(json: &str, what: &str) -> Result<T> {
    serde_json::from_str(json)
        .map_err(|err| DomainError::InvalidCommand(format!("malformed {what}: {err}")))
}

/// Splits a stored `<code>: <message>` reason.
fn split_reason(reason: &str) -> (String, String) {
    reason.split_once(": ").map_or_else(
        || (reason.to_string(), String::new()),
        |(code, message)| (code.to_string(), message.to_string()),
    )
}

/// Counts never overflow in practice; saturate rather than panic if they do.
fn count(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn count_usize(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;

    #[test]
    fn a_stored_reason_splits_into_code_and_message() {
        assert_eq!(
            split_reason("insufficient_funds: no room in 'Vacanze'"),
            (
                "insufficient_funds".to_string(),
                "no room in 'Vacanze'".to_string()
            )
        );
        // A reason without the separator is all code, so nothing is lost.
        assert_eq!(split_reason("boom"), ("boom".to_string(), String::new()));
        assert_eq!(split_reason(""), (String::new(), String::new()));
    }

    #[test]
    fn a_stored_reason_keeps_its_detail_and_reads_the_old_form() {
        let refused = Reason {
            code: "not_a_member".to_string(),
            message: "elisa: is not a member of this vault".to_string(),
            detail: Some("elisa".to_string()),
        };
        assert_eq!(Reason::parse(&refused.stored()), refused);

        // Without a detail the column holds what it always held.
        let plain = Reason {
            detail: None,
            ..refused.clone()
        };
        assert_eq!(
            plain.stored(),
            "not_a_member: elisa: is not a member of this vault"
        );
        assert_eq!(Reason::parse(&plain.stored()), plain);
        assert_eq!(
            Reason::parse("insufficient_funds: no room"),
            Reason {
                code: "insufficient_funds".to_string(),
                message: "no room".to_string(),
                detail: None,
            }
        );
    }

    #[test]
    fn a_rejection_without_a_detail_reads_and_writes_as_before() {
        let old = r#"{"command_id":"0192f0c8-0000-7000-8000-000000000001","status":"rejected","code":"insufficient_funds","message":"no room"}"#;
        let result: PushResult = serde_json::from_str(old).unwrap();
        assert_eq!(
            result.outcome,
            PushOutcome::Rejected {
                code: "insufficient_funds".to_string(),
                message: "no room".to_string(),
                detail: None,
            }
        );
        assert!(!serde_json::to_string(&result).unwrap().contains("detail"));

        let named: PushResult = serde_json::from_str(
            r#"{"command_id":"0192f0c8-0000-7000-8000-000000000001","status":"rejected","code":"not_a_member","message":"elisa is not a member of this vault","detail":"elisa"}"#,
        )
        .unwrap();
        assert!(matches!(
            named.outcome,
            PushOutcome::Rejected { detail: Some(ref name), .. } if name == "elisa"
        ));
    }

    #[test]
    fn the_watermark_stops_at_the_first_hole() {
        let core = Core::open_in_memory().unwrap();
        let vault = Uuid::now_v7();
        let insert = |seq: i64, server_seq: Option<i64>| {
            core.conn
                .execute(
                    "INSERT INTO commands
                        (id, vault_id, seq, author, kind, payload, created_at, status, server_seq)
                     VALUES (?1, ?2, ?3, 'alice', 'expense', '{}', 0, 'applied', ?4)",
                    params![Uuid::now_v7(), vault, seq, server_seq],
                )
                .unwrap();
        };

        assert_eq!(last_server_seq(&core.conn, vault).unwrap(), 0);
        insert(1, Some(1));
        insert(2, Some(2));
        assert_eq!(last_server_seq(&core.conn, vault).unwrap(), 2);
        // Seq 3 belongs to another member and has not been pulled yet, so a
        // push that confirmed 4 must not move the watermark past the hole.
        insert(3, Some(4));
        assert_eq!(last_server_seq(&core.conn, vault).unwrap(), 2);
        insert(4, Some(3));
        assert_eq!(last_server_seq(&core.conn, vault).unwrap(), 4);
        // The outbox never counts.
        insert(5, None);
        assert_eq!(last_server_seq(&core.conn, vault).unwrap(), 4);
    }
}
