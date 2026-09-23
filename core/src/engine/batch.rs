//! Several commands applied in one SQLite transaction.
//!
//! The batch is atomic on this device only: every command keeps its own seq
//! and log row, so the outbox pushes them one by one and the server (or a
//! later rebase) accepts or refuses each of them on its own.

use crate::{CommandEnvelope, Core, DomainError, Receipt, Result};

impl Core {
    /// Applies `envelopes` in order, all of them or none. A command id already
    /// in the log is deduplicated like in [`Core::execute`]; the first domain
    /// error rolls the whole batch back and is returned unchanged.
    pub fn execute_batch(&mut self, envelopes: Vec<CommandEnvelope>) -> Result<Vec<Receipt>> {
        let _ = envelopes;
        Err(DomainError::InvalidCommand(
            "execute_batch: not implemented".to_string(),
        ))
    }
}
