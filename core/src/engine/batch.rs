//! Several commands applied in one SQLite transaction.
//!
//! The batch is atomic on this device only: every command keeps its own seq
//! and log row, so the outbox pushes them one by one and the server (or a
//! later rebase) accepts or refuses each of them on its own. Nothing in
//! `sync.rs` assumes one command per SQLite transaction: the outbox, push
//! results and rebases all work row by row on the log, keyed by command id.

use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use super::{apply_envelope, next_seq};
use crate::{CommandEnvelope, Core, DomainError, Receipt, Result};

impl Core {
    /// Applies `envelopes` in order, all of them or none. A command id already
    /// in the log is deduplicated like in [`Core::execute`]; the first domain
    /// error rolls the whole batch back and is returned unchanged.
    ///
    /// Every envelope must target the same vault (`InvalidCommand` otherwise),
    /// since a batch is one step of one vault's log and syncs with it. An id
    /// repeated inside the batch is deduplicated against its first occurrence.
    /// An empty batch writes nothing and returns no receipts.
    pub fn execute_batch(&mut self, envelopes: Vec<CommandEnvelope>) -> Result<Vec<Receipt>> {
        let Some(first) = envelopes.first() else {
            return Ok(Vec::new());
        };
        let vault_id = first.vault_id;
        if let Some(stray) = envelopes.iter().find(|env| env.vault_id != vault_id) {
            return Err(DomainError::InvalidCommand(format!(
                "batch: command {} belongs to vault {}, not {vault_id}",
                stray.id, stray.vault_id
            )));
        }

        let tx = self.conn.transaction()?;
        let now = Utc::now().timestamp();
        let mut receipts = Vec::with_capacity(envelopes.len());
        for env in &envelopes {
            let existing = tx
                .query_row(
                    "SELECT seq, result_id FROM commands WHERE id = ?1",
                    params![env.id],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<Uuid>>(1)?)),
                )
                .optional()?;
            if let Some((seq, result_id)) = existing {
                receipts.push(Receipt {
                    command_id: env.id,
                    seq,
                    result_id,
                    deduplicated: true,
                });
                continue;
            }
            // Dropping `tx` on the early return rolls back every command
            // applied so far.
            let seq = next_seq(&tx, vault_id)?;
            let result_id = apply_envelope(&tx, env, seq, None, now)?;
            receipts.push(Receipt {
                command_id: env.id,
                seq,
                result_id,
                deduplicated: false,
            });
        }
        tx.commit()?;
        Ok(receipts)
    }
}
