//! Applying commands to the projection and appending them to the log.
//!
//! Every command runs inside one SQLite transaction: validation, state
//! changes and the log row commit together or not at all.

mod batch;
pub(crate) mod entities;
mod recurring;
mod update;

use chrono::{DateTime, FixedOffset, Utc};
use rusqlite::{OptionalExtension, Transaction, params};
use uuid::Uuid;

use crate::{
    Command, CommandEnvelope, Core, DomainError, Entry, Flow, FlowMode, Receipt, Result,
    TransactionKind,
    category::{OPENING_KEY, UNCATEGORIZED_KEY, normalize_category_key, validate_category_name},
    flow::UNALLOCATED_NAME,
};

impl Core {
    /// Apply a command. A command id already in the log is a no-op that
    /// returns the original receipt (idempotency).
    pub fn execute(&mut self, env: CommandEnvelope) -> Result<Receipt> {
        let tx = self.conn.transaction()?;
        let existing = tx
            .query_row(
                "SELECT seq, result_id FROM commands WHERE id = ?1",
                params![env.id],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<Uuid>>(1)?)),
            )
            .optional()?;
        if let Some((seq, result_id)) = existing {
            return Ok(Receipt {
                command_id: env.id,
                seq,
                result_id,
                deduplicated: true,
            });
        }

        let now = Utc::now().timestamp();
        let seq = next_seq(&tx, env.vault_id)?;
        let result_id = apply_envelope(&tx, &env, seq, None, now)?;
        tx.commit()?;
        Ok(Receipt {
            command_id: env.id,
            seq,
            result_id,
            deduplicated: false,
        })
    }
}

/// Next free local position in a vault's log. Rejected rows count: seq is a
/// key, not a measure of how much was applied.
pub(crate) fn next_seq(tx: &Transaction<'_>, vault_id: Uuid) -> Result<i64> {
    Ok(tx.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM commands WHERE vault_id = ?1",
        params![vault_id],
        |r| r.get(0),
    )?)
}

/// Applies one envelope inside an open transaction at an explicit position and
/// appends its log row. The caller owns the transaction, the seq and the
/// idempotency check, which is what lets a rebase rebuild a whole vault in one
/// transaction.
pub(crate) fn apply_envelope(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    seq: i64,
    server_seq: Option<i64>,
    now: i64,
) -> Result<Option<Uuid>> {
    let result_id = apply(tx, env, now)?;
    log_row(tx, env, seq, server_seq, now, LogRow::Applied(result_id))?;
    Ok(result_id)
}

/// Same as [`apply_envelope`], but a domain failure only rolls the command
/// back: `Ok(Some(err))` means nothing of it was written and the surrounding
/// transaction is still usable.
pub(crate) fn try_apply_envelope(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    seq: i64,
    server_seq: Option<i64>,
    now: i64,
) -> Result<Option<DomainError>> {
    tx.execute_batch("SAVEPOINT sparagne_apply")?;
    match apply_envelope(tx, env, seq, server_seq, now) {
        Ok(_) => {
            tx.execute_batch("RELEASE sparagne_apply")?;
            Ok(None)
        }
        Err(err) => {
            tx.execute_batch("ROLLBACK TO sparagne_apply; RELEASE sparagne_apply")?;
            Ok(Some(err))
        }
    }
}

/// What a log row records: a command that went through, or one the server (or
/// a rebase) refused, kept for the UI.
pub(crate) enum LogRow {
    Applied(Option<Uuid>),
    /// `<code>: <message>`.
    Rejected(String),
}

/// Writes one row of the log.
pub(crate) fn log_row(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    seq: i64,
    server_seq: Option<i64>,
    created_at: i64,
    row: LogRow,
) -> Result<()> {
    let (status, result_id, rejection) = match row {
        LogRow::Applied(result_id) => ("applied", result_id, None),
        LogRow::Rejected(reason) => ("rejected", None, Some(reason)),
    };
    tx.execute(
        "INSERT INTO commands
            (id, vault_id, seq, author, kind, payload, occurred_at, created_at,
             status, rejection, result_id, server_seq)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            env.id,
            env.vault_id,
            seq,
            env.author,
            env.command.kind_name(),
            serde_json::to_string(&env.command)?,
            env.command.occurred_at().map(|d| d.timestamp()),
            created_at,
            status,
            rejection,
            result_id,
            server_seq,
        ],
    )?;
    Ok(())
}

fn apply(tx: &Transaction<'_>, env: &CommandEnvelope, now: i64) -> Result<Option<Uuid>> {
    match &env.command {
        Command::CreateVault { name, currency } => {
            create_vault(tx, env, name, currency.code(), now).map(Some)
        }
        Command::RenameVault { name } => rename_vault(tx, env, name).map(|()| None),
        Command::DeleteVault => delete_vault(tx, env).map(|()| None),
        Command::CreateWallet {
            name,
            opening_balance,
            occurred_at,
        } => create_wallet(tx, env, name, *opening_balance, *occurred_at, now).map(Some),
        Command::CreateFlow {
            name,
            mode,
            allow_negative,
            opening_allocation,
            occurred_at,
        } => create_flow(
            tx,
            env,
            name,
            *mode,
            *allow_negative,
            *opening_allocation,
            *occurred_at,
            now,
        )
        .map(Some),
        Command::RenameWallet { wallet_id, name } => {
            entities::rename_wallet(tx, env, *wallet_id, name).map(|()| None)
        }
        Command::ArchiveWallet { wallet_id } => {
            entities::set_wallet_archived(tx, env, *wallet_id, true).map(|()| None)
        }
        Command::RestoreWallet { wallet_id } => {
            entities::set_wallet_archived(tx, env, *wallet_id, false).map(|()| None)
        }
        Command::UpdateFlow {
            flow_id,
            name,
            mode,
            allow_negative,
        } => entities::update_flow(tx, env, *flow_id, name.as_deref(), *mode, *allow_negative)
            .map(|()| None),
        Command::ArchiveFlow { flow_id } => {
            entities::set_flow_archived(tx, env, *flow_id, true).map(|()| None)
        }
        Command::RestoreFlow { flow_id } => {
            entities::set_flow_archived(tx, env, *flow_id, false).map(|()| None)
        }
        Command::CreateCategory { name } => create_category(tx, env, name).map(Some),
        Command::RenameCategory { category_id, name } => {
            entities::rename_category(tx, env, *category_id, name).map(|()| None)
        }
        Command::ArchiveCategory { category_id } => {
            entities::set_category_archived(tx, env, *category_id, true).map(|()| None)
        }
        Command::RestoreCategory { category_id } => {
            entities::set_category_archived(tx, env, *category_id, false).map(|()| None)
        }
        Command::AddAlias { category_id, alias } => {
            entities::add_alias(tx, env, *category_id, alias).map(Some)
        }
        Command::RemoveAlias { category_id, alias } => {
            entities::remove_alias(tx, env, *category_id, alias).map(|()| None)
        }
        Command::MergeCategory {
            source_id,
            target_id,
        } => entities::merge_category(tx, env, *source_id, *target_id).map(|()| None),
        Command::Income(e) => entry(tx, env, TransactionKind::Income, e).map(Some),
        Command::Expense(e) => entry(tx, env, TransactionKind::Expense, e).map(Some),
        Command::Refund(e) => entry(tx, env, TransactionKind::Refund, e).map(Some),
        Command::TransferWallet {
            amount,
            from_wallet_id,
            to_wallet_id,
            note,
            occurred_at,
        } => transfer_wallet(
            tx,
            env,
            *amount,
            *from_wallet_id,
            *to_wallet_id,
            note.as_deref(),
            *occurred_at,
        )
        .map(Some),
        Command::TransferFlow {
            amount,
            from_flow_id,
            to_flow_id,
            note,
            occurred_at,
        } => {
            let category = system_category_id(tx, env.vault_id, UNCATEGORIZED_KEY)?;
            post_transfer_flow(
                tx,
                env,
                env.id,
                *amount,
                *from_flow_id,
                *to_flow_id,
                category,
                normalize_note(note.as_deref()),
                *occurred_at,
            )
            .map(Some)
        }
        Command::UpdateTransaction {
            transaction_id,
            patch,
        } => update::update_transaction(tx, env, *transaction_id, patch).map(|()| None),
        Command::VoidTransaction { transaction_id } => {
            void_transaction(tx, env, *transaction_id, now).map(|()| None)
        }
        Command::CreateRecurring {
            transaction_kind,
            amount,
            wallet_id,
            flow_id,
            category,
            note,
            schedule,
        } => recurring::create_recurring(
            tx,
            env,
            &recurring::RecurringSpec {
                kind: *transaction_kind,
                amount: *amount,
                wallet_id: *wallet_id,
                flow_id: *flow_id,
                category: category.as_deref(),
                note: note.as_deref(),
                schedule: *schedule,
            },
            now,
        )
        .map(Some),
        Command::UpdateRecurring {
            recurring_id,
            patch,
        } => recurring::update_recurring(tx, env, *recurring_id, patch).map(|()| None),
        Command::ArchiveRecurring { recurring_id } => {
            recurring::archive_recurring(tx, env, *recurring_id, now).map(|()| None)
        }
        Command::RestoreRecurring { recurring_id } => {
            recurring::restore_recurring(tx, env, *recurring_id).map(|()| None)
        }
        Command::ExecuteRecurring {
            recurring_id,
            period_date,
            occurred_at,
        } => recurring::execute_recurring(tx, env, *recurring_id, *period_date, *occurred_at, now)
            .map(Some),
        Command::SkipRecurring {
            recurring_id,
            period_date,
        } => recurring::skip_recurring(tx, env, *recurring_id, *period_date, now).map(|()| None),
    }
}

// ---------------------------------------------------------------------------
// Vault, wallet, flow, category
// ---------------------------------------------------------------------------

fn create_vault(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    name: &str,
    currency: &str,
    now: i64,
) -> Result<Uuid> {
    if env.vault_id != env.id {
        return Err(DomainError::InvalidCommand(
            "create_vault: vault_id must equal the command id".to_string(),
        ));
    }
    let name = normalize_name(name, "vault")?;
    let taken: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM vaults WHERE owner_user_id = ?1 AND lower(name) = lower(?2))",
        params![env.author, name],
        |r| r.get(0),
    )?;
    if taken {
        return Err(DomainError::AlreadyExists(name));
    }
    tx.execute(
        "INSERT INTO vaults (id, name, currency, owner_user_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![env.id, name, currency, env.author, now],
    )?;
    tx.execute(
        "INSERT INTO flows (id, vault_id, name, system_kind, balance, cap, income_total, allow_negative, archived, created_at)
         VALUES (?1, ?2, ?3, 'unallocated', 0, NULL, NULL, 1, 0, ?4)",
        params![derived_id(env.id, "unallocated"), env.id, UNALLOCATED_NAME, now],
    )?;
    insert_system_category(
        tx,
        env.id,
        derived_id(env.id, "uncategorized"),
        "Uncategorized",
        UNCATEGORIZED_KEY,
    )?;
    insert_system_category(
        tx,
        env.id,
        derived_id(env.id, "opening"),
        "Opening",
        OPENING_KEY,
    )?;
    Ok(env.id)
}

/// The name stays unique among the vaults of the same owner, which on the
/// server means every vault that account created; the check runs against the
/// vault's owner, not the author, because an editor may rename too.
fn rename_vault(tx: &Transaction<'_>, env: &CommandEnvelope, name: &str) -> Result<()> {
    let owner = vault_owner(tx, env.vault_id)?;
    let name = normalize_name(name, "vault")?;
    let taken: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM vaults
         WHERE owner_user_id = ?1 AND lower(name) = lower(?2) AND id <> ?3)",
        params![owner, name, env.vault_id],
        |r| r.get(0),
    )?;
    if taken {
        return Err(DomainError::AlreadyExists(name));
    }
    tx.execute(
        "UPDATE vaults SET name = ?1 WHERE id = ?2",
        params![name, env.vault_id],
    )?;
    Ok(())
}

/// Only the owner may delete. Deleting the vault row cascades to every
/// projection table (`schema.sql`); the log is left alone on purpose, so the
/// command reaches the server and the other members like any other, and so
/// that a replay ends where this database is now.
fn delete_vault(tx: &Transaction<'_>, env: &CommandEnvelope) -> Result<()> {
    let owner = vault_owner(tx, env.vault_id)?;
    if env.author != owner {
        return Err(DomainError::Forbidden(format!(
            "only the owner '{owner}' can delete the vault"
        )));
    }
    tx.execute("DELETE FROM vaults WHERE id = ?1", params![env.vault_id])?;
    Ok(())
}

fn create_wallet(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    name: &str,
    opening_balance: i64,
    occurred_at: DateTime<FixedOffset>,
    now: i64,
) -> Result<Uuid> {
    require_vault(tx, env.vault_id)?;
    let name = normalize_name(name, "wallet")?;
    if name_taken(tx, "wallets", env.vault_id, &name)? {
        return Err(DomainError::AlreadyExists(name));
    }
    tx.execute(
        "INSERT INTO wallets (id, vault_id, name, balance, archived, created_at) VALUES (?1, ?2, ?3, 0, 0, ?4)",
        params![env.id, env.vault_id, name, now],
    )?;
    if opening_balance != 0 {
        let kind = if opening_balance > 0 {
            TransactionKind::Income
        } else {
            TransactionKind::Expense
        };
        post_entry(
            tx,
            env,
            derived_id(env.id, "opening"),
            kind,
            opening_balance,
            env.id,
            unallocated_id(tx, env.vault_id)?,
            system_category_id(tx, env.vault_id, OPENING_KEY)?,
            Some(format!("opening balance for wallet '{name}'")),
            occurred_at,
        )?;
    }
    Ok(env.id)
}

#[allow(clippy::too_many_arguments)]
fn create_flow(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    name: &str,
    mode: FlowMode,
    allow_negative: bool,
    opening_allocation: i64,
    occurred_at: DateTime<FixedOffset>,
    now: i64,
) -> Result<Uuid> {
    require_vault(tx, env.vault_id)?;
    let name = normalize_name(name, "flow")?;
    if name.eq_ignore_ascii_case(UNALLOCATED_NAME) {
        return Err(DomainError::InvalidFlow(
            "flow name is reserved".to_string(),
        ));
    }
    if name_taken(tx, "flows", env.vault_id, &name)? {
        return Err(DomainError::AlreadyExists(name));
    }
    mode.validate()?;
    if opening_allocation < 0 {
        return Err(DomainError::InvalidAmount(
            "opening allocation must be >= 0".to_string(),
        ));
    }
    let (cap, income_total) = match mode {
        FlowMode::Unlimited => (None, None),
        FlowMode::NetCapped { cap } => (Some(cap), None),
        FlowMode::IncomeCapped { cap } => (Some(cap), Some(0)),
    };
    tx.execute(
        "INSERT INTO flows (id, vault_id, name, system_kind, balance, cap, income_total, allow_negative, archived, created_at)
         VALUES (?1, ?2, ?3, NULL, 0, ?4, ?5, ?6, 0, ?7)",
        params![env.id, env.vault_id, name, cap, income_total, allow_negative, now],
    )?;
    if opening_allocation > 0 {
        post_transfer_flow(
            tx,
            env,
            derived_id(env.id, "opening"),
            opening_allocation,
            unallocated_id(tx, env.vault_id)?,
            env.id,
            system_category_id(tx, env.vault_id, OPENING_KEY)?,
            Some(format!("opening allocation for flow '{name}'")),
            occurred_at,
        )?;
    }
    Ok(env.id)
}

fn create_category(tx: &Transaction<'_>, env: &CommandEnvelope, name: &str) -> Result<Uuid> {
    require_vault(tx, env.vault_id)?;
    let (display, key) = validate_category_name(name)?;
    if category_key_taken(tx, env.vault_id, &key)? {
        return Err(DomainError::AlreadyExists(display));
    }
    tx.execute(
        "INSERT INTO categories (id, vault_id, name, name_norm, is_system, archived) VALUES (?1, ?2, ?3, ?4, 0, 0)",
        params![env.id, env.vault_id, display, key],
    )?;
    Ok(env.id)
}

// ---------------------------------------------------------------------------
// Transactions
// ---------------------------------------------------------------------------

fn entry(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    kind: TransactionKind,
    e: &Entry,
) -> Result<Uuid> {
    require_vault(tx, env.vault_id)?;
    if e.amount <= 0 {
        return Err(DomainError::InvalidAmount("amount must be > 0".to_string()));
    }
    let wallet_id = resolve_wallet(tx, env.vault_id, e.wallet_id)?;
    let flow_id = match e.flow_id {
        None => unallocated_id(tx, env.vault_id)?,
        Some(id) => {
            let flow = load_flow(tx, env.vault_id, id)?;
            if flow.archived {
                return Err(DomainError::InvalidCommand("flow is archived".to_string()));
            }
            id
        }
    };
    let category_id = resolve_category(tx, env, e.category.as_deref())?;
    let signed = match kind {
        TransactionKind::Expense => -e.amount,
        _ => e.amount,
    };
    post_entry(
        tx,
        env,
        env.id,
        kind,
        signed,
        wallet_id,
        flow_id,
        category_id,
        normalize_note(e.note.as_deref()),
        e.occurred_at,
    )?;
    Ok(env.id)
}

/// Write a two-leg entry (wallet + flow, same signed amount) and apply it.
#[allow(clippy::too_many_arguments)]
fn post_entry(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    tx_id: Uuid,
    kind: TransactionKind,
    signed: i64,
    wallet_id: Uuid,
    flow_id: Uuid,
    category_id: Uuid,
    note: Option<String>,
    occurred_at: DateTime<FixedOffset>,
) -> Result<()> {
    let mut flow = load_flow(tx, env.vault_id, flow_id)?;
    flow.apply_leg_change(0, signed)?;
    save_flow(tx, &flow)?;
    adjust_wallet(tx, wallet_id, signed)?;
    insert_transaction(
        tx,
        env,
        tx_id,
        kind,
        signed.abs(),
        category_id,
        note,
        occurred_at,
    )?;
    insert_leg(tx, tx_id, 0, "wallet", wallet_id, signed)?;
    insert_leg(tx, tx_id, 1, "flow", flow_id, signed)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn transfer_wallet(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    amount: i64,
    from: Uuid,
    to: Uuid,
    note: Option<&str>,
    occurred_at: DateTime<FixedOffset>,
) -> Result<Uuid> {
    require_vault(tx, env.vault_id)?;
    if amount <= 0 {
        return Err(DomainError::InvalidAmount("amount must be > 0".to_string()));
    }
    if from == to {
        return Err(DomainError::InvalidCommand(
            "from and to must differ".to_string(),
        ));
    }
    resolve_wallet(tx, env.vault_id, Some(from))?;
    resolve_wallet(tx, env.vault_id, Some(to))?;
    adjust_wallet(tx, from, -amount)?;
    adjust_wallet(tx, to, amount)?;
    let category = system_category_id(tx, env.vault_id, UNCATEGORIZED_KEY)?;
    insert_transaction(
        tx,
        env,
        env.id,
        TransactionKind::TransferWallet,
        amount,
        category,
        normalize_note(note),
        occurred_at,
    )?;
    insert_leg(tx, env.id, 0, "wallet", from, -amount)?;
    insert_leg(tx, env.id, 1, "wallet", to, amount)?;
    Ok(env.id)
}

#[allow(clippy::too_many_arguments)]
fn post_transfer_flow(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    tx_id: Uuid,
    amount: i64,
    from: Uuid,
    to: Uuid,
    category_id: Uuid,
    note: Option<String>,
    occurred_at: DateTime<FixedOffset>,
) -> Result<Uuid> {
    require_vault(tx, env.vault_id)?;
    if amount <= 0 {
        return Err(DomainError::InvalidAmount("amount must be > 0".to_string()));
    }
    if from == to {
        return Err(DomainError::InvalidCommand(
            "from and to must differ".to_string(),
        ));
    }
    let mut source = load_flow(tx, env.vault_id, from)?;
    let mut target = load_flow(tx, env.vault_id, to)?;
    if source.archived || target.archived {
        return Err(DomainError::InvalidCommand("flow is archived".to_string()));
    }
    source.apply_leg_change(0, -amount)?;
    target.apply_leg_change(0, amount)?;
    save_flow(tx, &source)?;
    save_flow(tx, &target)?;
    insert_transaction(
        tx,
        env,
        tx_id,
        TransactionKind::TransferFlow,
        amount,
        category_id,
        note,
        occurred_at,
    )?;
    insert_leg(tx, tx_id, 0, "flow", from, -amount)?;
    insert_leg(tx, tx_id, 1, "flow", to, amount)?;
    Ok(tx_id)
}

fn void_transaction(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    transaction_id: Uuid,
    now: i64,
) -> Result<()> {
    let voided_at: Option<i64> = tx
        .query_row(
            "SELECT voided_at FROM transactions WHERE id = ?1 AND vault_id = ?2",
            params![transaction_id, env.vault_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| DomainError::NotFound("transaction".to_string()))?;
    if voided_at.is_some() {
        return Err(DomainError::InvalidCommand(
            "transaction already voided".to_string(),
        ));
    }
    let legs: Vec<(String, Uuid, i64)> = {
        let mut stmt = tx.prepare(
            "SELECT target_kind, target_id, amount FROM legs WHERE transaction_id = ?1 ORDER BY ordinal",
        )?;
        let rows = stmt.query_map(params![transaction_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    for (kind, target, amount) in legs {
        match kind.as_str() {
            "wallet" => adjust_wallet(tx, target, -amount)?,
            "flow" => {
                let mut flow = load_flow(tx, env.vault_id, target)?;
                flow.apply_leg_change_unchecked(amount, 0);
                save_flow(tx, &flow)?;
            }
            other => {
                return Err(DomainError::Storage(format!(
                    "unknown leg target '{other}'"
                )));
            }
        }
    }
    tx.execute(
        "UPDATE transactions SET voided_at = ?1, voided_by = ?2 WHERE id = ?3",
        params![now, env.author, transaction_id],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Resolution helpers
// ---------------------------------------------------------------------------

/// Ids of entities created inside a command are derived from the command id,
/// so replaying the log yields the same ids.
fn derived_id(command_id: Uuid, role: &str) -> Uuid {
    Uuid::new_v5(&command_id, role.as_bytes())
}

fn normalize_name(value: &str, label: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DomainError::InvalidName(format!(
            "{label} name must not be empty"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalize_note(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

fn require_vault(tx: &Transaction<'_>, vault_id: Uuid) -> Result<()> {
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM vaults WHERE id = ?1)",
        params![vault_id],
        |r| r.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(DomainError::NotFound("vault".to_string()))
    }
}

/// The `owner_user_id` of a vault: the author of its `CreateVault`.
fn vault_owner(tx: &Transaction<'_>, vault_id: Uuid) -> Result<String> {
    tx.query_row(
        "SELECT owner_user_id FROM vaults WHERE id = ?1",
        params![vault_id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| DomainError::NotFound("vault".to_string()))
}

fn name_taken(tx: &Transaction<'_>, table: &str, vault_id: Uuid, name: &str) -> Result<bool> {
    let sql = format!(
        "SELECT EXISTS(SELECT 1 FROM {table} WHERE vault_id = ?1 AND lower(name) = lower(?2))"
    );
    Ok(tx.query_row(&sql, params![vault_id, name], |r| r.get(0))?)
}

fn category_key_taken(tx: &Transaction<'_>, vault_id: Uuid, key: &str) -> Result<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM categories WHERE vault_id = ?1 AND name_norm = ?2)
             OR EXISTS(SELECT 1 FROM category_aliases WHERE vault_id = ?1 AND alias_norm = ?2)",
        params![vault_id, key],
        |r| r.get(0),
    )?)
}

fn insert_system_category(
    tx: &Transaction<'_>,
    vault_id: Uuid,
    id: Uuid,
    name: &str,
    key: &str,
) -> Result<()> {
    tx.execute(
        "INSERT INTO categories (id, vault_id, name, name_norm, is_system, archived) VALUES (?1, ?2, ?3, ?4, 1, 0)",
        params![id, vault_id, name, key],
    )?;
    Ok(())
}

fn system_category_id(tx: &Transaction<'_>, vault_id: Uuid, key: &str) -> Result<Uuid> {
    tx.query_row(
        "SELECT id FROM categories WHERE vault_id = ?1 AND name_norm = ?2 AND is_system = 1",
        params![vault_id, key],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| DomainError::NotFound(format!("system category '{key}'")))
}

fn unallocated_id(tx: &Transaction<'_>, vault_id: Uuid) -> Result<Uuid> {
    tx.query_row(
        "SELECT id FROM flows WHERE vault_id = ?1 AND system_kind = 'unallocated'",
        params![vault_id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| DomainError::InvalidFlow("missing Unallocated flow".to_string()))
}

fn resolve_wallet(tx: &Transaction<'_>, vault_id: Uuid, wallet_id: Option<Uuid>) -> Result<Uuid> {
    match wallet_id {
        Some(id) => {
            let archived: bool = tx
                .query_row(
                    "SELECT archived FROM wallets WHERE id = ?1 AND vault_id = ?2",
                    params![id, vault_id],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| DomainError::NotFound("wallet".to_string()))?;
            if archived {
                return Err(DomainError::InvalidCommand(
                    "wallet is archived".to_string(),
                ));
            }
            Ok(id)
        }
        None => {
            let mut stmt =
                tx.prepare("SELECT id FROM wallets WHERE vault_id = ?1 AND archived = 0 LIMIT 2")?;
            let ids: Vec<Uuid> = stmt
                .query_map(params![vault_id], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            match ids.as_slice() {
                [only] => Ok(*only),
                [] => Err(DomainError::NotFound("wallet".to_string())),
                _ => Err(DomainError::InvalidCommand(
                    "wallet_id is required when more than one wallet exists".to_string(),
                )),
            }
        }
    }
}

/// Free text -> category id: blank = Uncategorized; exact key; alias; else
/// auto-create with an id derived from the command.
fn resolve_category(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    input: Option<&str>,
) -> Result<Uuid> {
    let vault_id = env.vault_id;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return system_category_id(tx, vault_id, UNCATEGORIZED_KEY);
    };
    let key = normalize_category_key(raw)?;
    if key == UNCATEGORIZED_KEY {
        return system_category_id(tx, vault_id, UNCATEGORIZED_KEY);
    }
    let direct = tx
        .query_row(
            "SELECT id, archived FROM categories WHERE vault_id = ?1 AND name_norm = ?2",
            params![vault_id, key],
            |r| Ok((r.get::<_, Uuid>(0)?, r.get::<_, bool>(1)?)),
        )
        .optional()?;
    let via_alias = match direct {
        Some(found) => Some(found),
        None => tx
            .query_row(
                "SELECT c.id, c.archived FROM category_aliases a
                 JOIN categories c ON c.id = a.category_id
                 WHERE a.vault_id = ?1 AND a.alias_norm = ?2",
                params![vault_id, key],
                |r| Ok((r.get::<_, Uuid>(0)?, r.get::<_, bool>(1)?)),
            )
            .optional()?,
    };
    if let Some((id, archived)) = via_alias {
        if archived {
            return Err(DomainError::InvalidName("category is archived".to_string()));
        }
        return Ok(id);
    }
    let (display, key) = validate_category_name(raw)?;
    let id = derived_id(env.id, "category");
    tx.execute(
        "INSERT INTO categories (id, vault_id, name, name_norm, is_system, archived) VALUES (?1, ?2, ?3, ?4, 0, 0)",
        params![id, vault_id, display, key],
    )?;
    Ok(id)
}

// ---------------------------------------------------------------------------
// Row-level helpers
// ---------------------------------------------------------------------------

pub(crate) fn load_flow(tx: &Transaction<'_>, vault_id: Uuid, flow_id: Uuid) -> Result<Flow> {
    let flow = tx
        .query_row(
            "SELECT id, name, system_kind, balance, cap, income_total, allow_negative, archived
             FROM flows WHERE id = ?1 AND vault_id = ?2",
            params![flow_id, vault_id],
            |r| {
                Ok(Flow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    is_unallocated: r.get::<_, Option<String>>(2)?.as_deref()
                        == Some("unallocated"),
                    balance: r.get(3)?,
                    cap: r.get(4)?,
                    income_total: r.get(5)?,
                    allow_negative: r.get(6)?,
                    archived: r.get(7)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| DomainError::NotFound("flow".to_string()))?;
    flow.validate_mode_fields()?;
    Ok(flow)
}

fn save_flow(tx: &Transaction<'_>, flow: &Flow) -> Result<()> {
    tx.execute(
        "UPDATE flows SET balance = ?1, income_total = ?2 WHERE id = ?3",
        params![flow.balance, flow.income_total, flow.id],
    )?;
    Ok(())
}

fn adjust_wallet(tx: &Transaction<'_>, wallet_id: Uuid, delta: i64) -> Result<()> {
    let changed = tx.execute(
        "UPDATE wallets SET balance = balance + ?1 WHERE id = ?2",
        params![delta, wallet_id],
    )?;
    if changed == 1 {
        Ok(())
    } else {
        Err(DomainError::NotFound("wallet".to_string()))
    }
}

#[allow(clippy::too_many_arguments)]
fn insert_transaction(
    tx: &Transaction<'_>,
    env: &CommandEnvelope,
    id: Uuid,
    kind: TransactionKind,
    amount_abs: i64,
    category_id: Uuid,
    note: Option<String>,
    occurred_at: DateTime<FixedOffset>,
) -> Result<()> {
    tx.execute(
        "INSERT INTO transactions
            (id, vault_id, kind, occurred_at, occurred_offset, amount, category_id, note, created_by, command_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            env.vault_id,
            kind.as_str(),
            occurred_at.timestamp(),
            occurred_at.offset().local_minus_utc(),
            amount_abs,
            category_id,
            note,
            env.author,
            env.id,
        ],
    )?;
    Ok(())
}

fn insert_leg(
    tx: &Transaction<'_>,
    transaction_id: Uuid,
    ordinal: i64,
    target_kind: &str,
    target_id: Uuid,
    amount: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO legs (transaction_id, ordinal, target_kind, target_id, amount) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![transaction_id, ordinal, target_kind, target_id, amount],
    )?;
    Ok(())
}
