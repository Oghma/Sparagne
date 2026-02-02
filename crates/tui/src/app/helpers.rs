use api_types::{
    error::ErrorCode,
    membership::MembershipRole,
    transaction::{TransactionDetailResponse, TransactionKind, TransactionView},
    vault::{FlowView, WalletView},
};
use engine::Currency;
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    app::{AppState, PaletteCommand},
    error::AppError,
};

/// Filters and sorts palette commands, prioritizing MRU when query is empty.
pub(crate) fn filter_commands(query: &str, mru: &[PaletteCommand]) -> Vec<PaletteCommand> {
    let query = query.trim().to_lowercase();
    let all = PaletteCommand::all();

    if query.is_empty() {
        // When no query, show MRU first, then remaining commands
        let mut result = Vec::with_capacity(all.len());
        for cmd in mru {
            if !result.contains(cmd) {
                result.push(*cmd);
            }
        }
        for cmd in &all {
            if !result.contains(cmd) {
                result.push(*cmd);
            }
        }
        return result;
    }

    let mut scored = all
        .into_iter()
        .filter_map(|cmd| {
            let label = cmd.label().to_lowercase();
            fuzzy_score(&label, &query).map(|score| (score, cmd))
        })
        .collect::<Vec<_>>();

    scored.sort_by_key(|(score, _)| *score);
    scored.into_iter().map(|(_, cmd)| cmd).collect()
}

fn fuzzy_score(label: &str, query: &str) -> Option<usize> {
    let mut score = 0usize;
    let mut pos = 0usize;
    for ch in query.chars() {
        if let Some(idx) = label[pos..].find(ch) {
            score += idx;
            pos += idx + 1;
        } else {
            return None;
        }
    }
    Some(score)
}

pub(crate) fn login_message_for_error(err: crate::client::ClientError) -> String {
    match err {
        crate::client::ClientError::Unauthorized => {
            "Credenziali errate o pairing mancante.".to_string()
        }
        crate::client::ClientError::Forbidden(payload) => match payload.code {
            ErrorCode::MembershipLastOwner => {
                "Non puoi rimuovere l'ultimo owner del flow.".to_string()
            }
            ErrorCode::MembershipOwnerImmutable => {
                "Non puoi cambiare il ruolo dell'owner del vault.".to_string()
            }
            ErrorCode::MembershipOwnerRemoveForbidden => {
                "Non puoi rimuovere l'owner del vault.".to_string()
            }
            _ => "Operazione non permessa.".to_string(),
        },
        crate::client::ClientError::NotFound(payload) => match payload.code {
            ErrorCode::NotFound => "Risorsa non trovata.".to_string(),
            _ => payload.message,
        },
        crate::client::ClientError::Conflict(payload) => format!("Conflitto: {}", payload.message),
        crate::client::ClientError::Validation(payload) => {
            if payload.message.contains("ambiguous vault name") {
                "Vault name is ambiguous. Use \"Main (owner)\" or a vault id.".to_string()
            } else {
                format!("Errore di validazione: {}", payload.message)
            }
        }
        crate::client::ClientError::BadRequest(payload) => {
            format!("Richiesta non valida: {}", payload.message)
        }
        crate::client::ClientError::Server(payload) => {
            format!("Errore server: {}", payload.message)
        }
        crate::client::ClientError::Client(message) => message,
        crate::client::ClientError::Transport(err) => format!("Server non raggiungibile: {err}"),
    }
}

pub(crate) fn extract_wallet_flow(
    detail: &TransactionDetailResponse,
) -> (Option<Uuid>, Option<Uuid>) {
    let mut wallet_id = None;
    let mut flow_id = None;
    for leg in &detail.legs {
        match leg.target {
            api_types::transaction::LegTarget::Wallet { wallet_id: id } => {
                wallet_id = Some(id);
            }
            api_types::transaction::LegTarget::Flow { flow_id: id } => {
                flow_id = Some(id);
            }
        }
    }
    (wallet_id, flow_id)
}

pub(crate) fn extract_wallet_transfer(
    detail: &TransactionDetailResponse,
) -> Result<(Uuid, Uuid), AppError> {
    let mut from_wallet = None;
    let mut to_wallet = None;
    for leg in &detail.legs {
        if let api_types::transaction::LegTarget::Wallet { wallet_id } = leg.target {
            if leg.amount_minor < 0 {
                from_wallet = Some(wallet_id);
            } else if leg.amount_minor > 0 {
                to_wallet = Some(wallet_id);
            }
        }
    }
    match (from_wallet, to_wallet) {
        (Some(from), Some(to)) => Ok((from, to)),
        _ => Err(AppError::Terminal(
            "impossibile determinare i wallet del transfer".to_string(),
        )),
    }
}

pub(crate) fn extract_flow_transfer(
    detail: &TransactionDetailResponse,
) -> Result<(Uuid, Uuid), AppError> {
    let mut from_flow = None;
    let mut to_flow = None;
    for leg in &detail.legs {
        if let api_types::transaction::LegTarget::Flow { flow_id } = leg.target {
            if leg.amount_minor < 0 {
                from_flow = Some(flow_id);
            } else if leg.amount_minor > 0 {
                to_flow = Some(flow_id);
            }
        }
    }
    match (from_flow, to_flow) {
        (Some(from), Some(to)) => Ok((from, to)),
        _ => Err(AppError::Terminal(
            "impossibile determinare i flow del transfer".to_string(),
        )),
    }
}

pub(crate) fn map_currency(currency: &api_types::Currency) -> Currency {
    match currency {
        api_types::Currency::Eur => Currency::Eur,
    }
}

pub(crate) fn month_label(month: u32) -> String {
    let label = match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    };
    label.to_string()
}

pub(crate) fn default_wallet_flow(
    state: &AppState,
) -> Result<(Uuid, Uuid, String, String), String> {
    let snapshot = state
        .snapshot
        .as_ref()
        .ok_or_else(|| "Snapshot non disponibile.".to_string())?;

    let wallet = state
        .transactions
        .scope_wallet_id
        .and_then(|wallet_id| {
            snapshot
                .wallets
                .iter()
                .find(|wallet| wallet.id == wallet_id && !wallet.archived)
        })
        .or_else(|| {
            state.default_wallet_id.and_then(|wallet_id| {
                snapshot
                    .wallets
                    .iter()
                    .find(|wallet| wallet.id == wallet_id && !wallet.archived)
            })
        })
        .or_else(|| {
            state
                .transactions
                .recent_wallet_ids
                .iter()
                .find_map(|recent_id| {
                    snapshot
                        .wallets
                        .iter()
                        .find(|wallet| wallet.id == *recent_id && !wallet.archived)
                })
        })
        .or_else(|| snapshot.wallets.iter().find(|wallet| !wallet.archived))
        .ok_or_else(|| "Nessun wallet disponibile.".to_string())?;
    let flow = state
        .transactions
        .scope_flow_id
        .and_then(|flow_id| {
            snapshot
                .flows
                .iter()
                .find(|flow| flow.id == flow_id && !flow.archived)
        })
        .or_else(|| {
            state.default_flow_id.and_then(|flow_id| {
                snapshot
                    .flows
                    .iter()
                    .find(|flow| flow.id == flow_id && !flow.archived)
            })
        })
        .or_else(|| {
            state
                .transactions
                .recent_flow_ids
                .iter()
                .find_map(|recent_id| {
                    snapshot
                        .flows
                        .iter()
                        .find(|flow| flow.id == *recent_id && !flow.archived)
                })
        })
        .or_else(|| {
            state.last_flow_id.and_then(|last_id| {
                snapshot
                    .flows
                    .iter()
                    .find(|flow| flow.id == last_id && !flow.archived)
            })
        })
        .or_else(|| snapshot.flows.iter().find(|flow| flow.is_unallocated))
        .ok_or_else(|| "Flow Unallocated mancante.".to_string())?;

    Ok((wallet.id, flow.id, wallet.name.clone(), flow.name.clone()))
}

pub(crate) fn transactions_visible_indices(state: &AppState) -> Vec<usize> {
    let query = normalize_query(state.transactions.search_query.as_str());
    let hidden_ids = &state.transactions.pending_delete_ids;
    if query.is_empty() {
        return state
            .transactions
            .items
            .iter()
            .enumerate()
            .filter_map(|(idx, tx)| {
                if hidden_ids.contains(&tx.id) {
                    None
                } else {
                    Some(idx)
                }
            })
            .collect();
    }

    state
        .transactions
        .items
        .iter()
        .enumerate()
        .filter_map(|(idx, tx)| {
            if hidden_ids.contains(&tx.id) {
                return None;
            }
            if transaction_matches_query(tx, query.as_str()) {
                Some(idx)
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn home_feed_indices(state: &AppState) -> Vec<usize> {
    let hidden_ids = &state.transactions.pending_delete_ids;
    state
        .transactions
        .items
        .iter()
        .enumerate()
        .filter_map(|(idx, tx)| {
            if hidden_ids.contains(&tx.id) {
                None
            } else {
                Some(idx)
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
pub(crate) enum HomeFeedItem {
    FlowAlert(FlowAlertItem),
    Transaction { index: usize },
}

#[derive(Debug, Clone)]
pub(crate) struct FlowAlertItem {
    pub flow_id: Uuid,
    pub name: String,
    pub balance_minor: i64,
    pub threshold_minor: i64,
    pub severity: FlowAlertSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlowAlertSeverity {
    Warning,
    Critical,
}

pub(crate) fn home_feed_items(state: &AppState) -> Vec<HomeFeedItem> {
    let mut items = flow_alert_items(state);
    items.extend(
        home_feed_indices(state)
            .into_iter()
            .map(|index| HomeFeedItem::Transaction { index }),
    );
    items
}

fn flow_alert_items(state: &AppState) -> Vec<HomeFeedItem> {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return Vec::new();
    };
    let low_balance_minor = state.home_low_balance_minor.max(0);

    let mut alerts: Vec<FlowAlertItem> = snapshot
        .flows
        .iter()
        .filter(|flow| !flow.archived && !flow.is_unallocated)
        .filter_map(|flow| {
            let balance = flow.balance_minor;
            let (severity, threshold_minor) = if balance < 0 {
                (FlowAlertSeverity::Critical, 0)
            } else if low_balance_minor > 0 && balance <= low_balance_minor {
                (FlowAlertSeverity::Warning, low_balance_minor)
            } else {
                return None;
            };

            Some(FlowAlertItem {
                flow_id: flow.id,
                name: flow.name.clone(),
                balance_minor: balance,
                threshold_minor,
                severity,
            })
        })
        .collect();

    alerts.sort_by(|a, b| {
        severity_rank(a.severity)
            .cmp(&severity_rank(b.severity))
            .then_with(|| a.balance_minor.cmp(&b.balance_minor))
            .then_with(|| a.name.cmp(&b.name))
    });

    alerts.into_iter().map(HomeFeedItem::FlowAlert).collect()
}

fn severity_rank(severity: FlowAlertSeverity) -> u8 {
    match severity {
        FlowAlertSeverity::Critical => 0,
        FlowAlertSeverity::Warning => 1,
    }
}

pub(crate) fn wallets_visible_indices(state: &AppState) -> Vec<usize> {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return Vec::new();
    };
    let query = normalize_query(state.wallets.search_query.as_str());
    if query.is_empty() {
        return (0..snapshot.wallets.len()).collect();
    }

    snapshot
        .wallets
        .iter()
        .enumerate()
        .filter_map(|(idx, wallet)| {
            if wallet.name.to_lowercase().contains(query.as_str()) {
                Some(idx)
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn flows_visible_indices(state: &AppState) -> Vec<usize> {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return Vec::new();
    };
    let query = normalize_query(state.flows.search_query.as_str());
    if query.is_empty() {
        return (0..snapshot.flows.len()).collect();
    }

    snapshot
        .flows
        .iter()
        .enumerate()
        .filter_map(|(idx, flow)| {
            if flow.name.to_lowercase().contains(query.as_str()) {
                Some(idx)
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn member_role_rank(role: MembershipRole) -> u8 {
    match role {
        MembershipRole::Owner => 0,
        MembershipRole::Editor => 1,
        MembershipRole::Viewer => 2,
    }
}

pub(crate) fn ordered_wallet_ids_from_state(state: &AppState) -> Vec<Uuid> {
    let active_ids = state
        .snapshot
        .as_ref()
        .map(|snap| {
            snap.wallets
                .iter()
                .filter(|wallet| !wallet.archived)
                .map(|wallet| wallet.id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut priority = Vec::new();
    if let Some(default_id) = state.default_wallet_id {
        priority.push(default_id);
    }
    priority.extend(state.transactions.recent_wallet_ids.iter().copied());
    ordered_ids(active_ids, &priority)
}

pub(crate) fn ordered_flow_ids_from_state(state: &AppState) -> Vec<Uuid> {
    let active_ids = state
        .snapshot
        .as_ref()
        .map(|snap| {
            snap.flows
                .iter()
                .filter(|flow| !flow.archived)
                .map(|flow| flow.id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut priority = Vec::new();
    if let Some(default_id) = state.default_flow_id {
        priority.push(default_id);
    }
    priority.extend(state.transactions.recent_flow_ids.iter().copied());
    ordered_ids(active_ids, &priority)
}

pub(crate) fn resolve_flow_name(state: &AppState, query: &str) -> Option<(Uuid, String, bool)> {
    let query = normalize_query(query);
    if query.is_empty() {
        return None;
    }
    let ordered = ordered_active_flows(state);
    let (exact, prefix, contains) = flow_name_buckets(&ordered, query.as_str());

    exact
        .first()
        .map(|flow| (flow.id, flow.name.clone(), true))
        .or_else(|| {
            prefix
                .first()
                .map(|flow| (flow.id, flow.name.clone(), false))
        })
        .or_else(|| {
            contains
                .first()
                .map(|flow| (flow.id, flow.name.clone(), false))
        })
}

/// Returns all matching categories for a query, ordered by match quality (exact
/// > prefix > contains).
pub(crate) fn resolve_category_matches(state: &AppState, query: &str) -> Vec<(Uuid, String)> {
    let query = normalize_query(query);
    if query.is_empty() {
        return Vec::new();
    }

    let mut exact = Vec::new();
    let mut prefix = Vec::new();
    let mut contains = Vec::new();

    for category in &state.categories.items {
        if category.archived {
            continue;
        }
        let name = category.name.to_lowercase();
        if name == query {
            exact.push((category.id, category.name.clone()));
        } else if name.starts_with(&query) {
            prefix.push((category.id, category.name.clone()));
        } else if name.contains(&query) {
            contains.push((category.id, category.name.clone()));
        }
    }

    let mut results = Vec::new();
    results.extend(exact);
    results.extend(prefix);
    results.extend(contains);
    results
}

/// Returns all matching flows for a query, ordered by match quality (exact >
/// prefix > contains).
pub(crate) fn resolve_flow_matches(state: &AppState, query: &str) -> Vec<(Uuid, String)> {
    let query = normalize_query(query);
    if query.is_empty() {
        return Vec::new();
    }
    let ordered = ordered_active_flows(state);
    let (exact, prefix, contains) = flow_name_buckets(&ordered, query.as_str());

    let mut results = Vec::new();
    for flow in exact {
        results.push((flow.id, flow.name.clone()));
    }
    for flow in prefix {
        results.push((flow.id, flow.name.clone()));
    }
    for flow in contains {
        results.push((flow.id, flow.name.clone()));
    }
    results
}

pub(crate) fn flow_name_suggestions(state: &AppState, query: &str, limit: usize) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let query = normalize_query(query);
    if query.is_empty() {
        return Vec::new();
    }
    let ordered = ordered_active_flows(state);
    let (exact, prefix, contains) = flow_name_buckets(&ordered, query.as_str());
    let source = if !exact.is_empty() {
        exact
    } else if !prefix.is_empty() {
        prefix
    } else {
        contains
    };
    source
        .into_iter()
        .take(limit)
        .map(|flow| flow.name.clone())
        .collect()
}

fn ordered_active_flows(state: &AppState) -> Vec<&FlowView> {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return Vec::new();
    };
    let ordered_ids = ordered_flow_ids_from_state(state);
    let mut by_id: HashMap<Uuid, &FlowView> = HashMap::with_capacity(snapshot.flows.len());
    for flow in snapshot.flows.iter().filter(|flow| !flow.archived) {
        by_id.insert(flow.id, flow);
    }

    let mut ordered = Vec::with_capacity(by_id.len());
    for id in ordered_ids {
        if let Some(flow) = by_id.get(&id) {
            ordered.push(*flow);
        }
    }
    ordered
}

fn flow_name_buckets<'a>(
    flows: &'a [&FlowView],
    query: &str,
) -> (Vec<&'a FlowView>, Vec<&'a FlowView>, Vec<&'a FlowView>) {
    let mut exact = Vec::new();
    let mut prefix = Vec::new();
    let mut contains = Vec::new();

    for flow in flows {
        let name = flow.name.to_lowercase();
        if name == query {
            exact.push(*flow);
        } else if name.starts_with(query) {
            prefix.push(*flow);
        } else if name.contains(query) {
            contains.push(*flow);
        }
    }

    (exact, prefix, contains)
}

pub(crate) fn resolve_wallet_name(state: &AppState, query: &str) -> Option<(Uuid, String, bool)> {
    let query = normalize_query(query);
    if query.is_empty() {
        return None;
    }
    let ordered = ordered_active_wallets(state);
    let (exact, prefix, contains) = wallet_name_buckets(&ordered, query.as_str());

    exact
        .first()
        .map(|wallet| (wallet.id, wallet.name.clone(), true))
        .or_else(|| {
            prefix
                .first()
                .map(|wallet| (wallet.id, wallet.name.clone(), false))
        })
        .or_else(|| {
            contains
                .first()
                .map(|wallet| (wallet.id, wallet.name.clone(), false))
        })
}

/// Returns all matching wallets for a query, ordered by match quality (exact >
/// prefix > contains).
pub(crate) fn resolve_wallet_matches(state: &AppState, query: &str) -> Vec<(Uuid, String)> {
    let query = normalize_query(query);
    if query.is_empty() {
        return Vec::new();
    }
    let ordered = ordered_active_wallets(state);
    let (exact, prefix, contains) = wallet_name_buckets(&ordered, query.as_str());

    let mut results = Vec::new();
    for wallet in exact {
        results.push((wallet.id, wallet.name.clone()));
    }
    for wallet in prefix {
        results.push((wallet.id, wallet.name.clone()));
    }
    for wallet in contains {
        results.push((wallet.id, wallet.name.clone()));
    }
    results
}

fn ordered_active_wallets(state: &AppState) -> Vec<&WalletView> {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return Vec::new();
    };
    let ordered_ids = ordered_wallet_ids_from_state(state);
    let mut by_id: HashMap<Uuid, &WalletView> = HashMap::with_capacity(snapshot.wallets.len());
    for wallet in snapshot.wallets.iter().filter(|wallet| !wallet.archived) {
        by_id.insert(wallet.id, wallet);
    }

    let mut ordered = Vec::with_capacity(by_id.len());
    for id in ordered_ids {
        if let Some(wallet) = by_id.get(&id) {
            ordered.push(*wallet);
        }
    }
    ordered
}

fn wallet_name_buckets<'a>(
    wallets: &'a [&WalletView],
    query: &str,
) -> (
    Vec<&'a WalletView>,
    Vec<&'a WalletView>,
    Vec<&'a WalletView>,
) {
    let mut exact = Vec::new();
    let mut prefix = Vec::new();
    let mut contains = Vec::new();

    for wallet in wallets {
        let name = wallet.name.to_lowercase();
        if name == query {
            exact.push(*wallet);
        } else if name.starts_with(query) {
            prefix.push(*wallet);
        } else if name.contains(query) {
            contains.push(*wallet);
        }
    }

    (exact, prefix, contains)
}

pub(crate) fn transaction_matches_query(tx: &TransactionView, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let kind = transaction_kind_label(tx.kind);
    if kind.contains(query) {
        return true;
    }
    if tx
        .note
        .as_ref()
        .map(|note| note.to_lowercase().contains(query))
        .unwrap_or(false)
    {
        return true;
    }
    if tx
        .category
        .as_ref()
        .map(|category| category.to_lowercase().contains(query))
        .unwrap_or(false)
    {
        return true;
    }
    let amount = tx.amount_minor.abs().to_string();
    if amount.contains(query) {
        return true;
    }
    let when = tx.occurred_at.format("%Y-%m-%d %H:%M").to_string();
    when.contains(query)
}

pub(crate) fn transaction_kind_label(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Income => "income",
        TransactionKind::Expense => "expense",
        TransactionKind::Refund => "refund",
        TransactionKind::TransferWallet => "transfer wallet",
        TransactionKind::TransferFlow => "transfer flow",
    }
}

pub(crate) fn normalize_query(query: &str) -> String {
    query.trim().to_lowercase()
}

pub(crate) fn format_amount_input(amount_minor: i64, currency: Currency) -> String {
    let sign = if amount_minor < 0 { "-" } else { "" };
    let abs = amount_minor.unsigned_abs();
    let scale = 10u64.pow(currency.minor_units() as u32);
    if scale == 1 {
        return format!("{sign}{abs}");
    }
    let major = abs / scale;
    let minor = abs % scale;
    format!(
        "{sign}{major}.{minor:0width$}",
        width = currency.minor_units() as usize
    )
}

pub(crate) fn ordered_ids(active: Vec<Uuid>, recents: &[Uuid]) -> Vec<Uuid> {
    let mut ordered = Vec::with_capacity(active.len());
    for recent in recents {
        if active.contains(recent) && !ordered.contains(recent) {
            ordered.push(*recent);
        }
    }
    for id in active {
        if !ordered.contains(&id) {
            ordered.push(id);
        }
    }
    ordered
}

pub(crate) fn push_recent_id(target: &mut Vec<Uuid>, value: Uuid, limit: usize) {
    if target.contains(&value) {
        return;
    }
    if target.len() >= limit {
        return;
    }
    target.push(value);
}

#[cfg(test)]
mod tests {
    use super::filter_commands;
    use crate::app::PaletteCommand;

    #[test]
    fn palette_includes_category_commands() {
        let all = PaletteCommand::all();
        assert!(all.contains(&PaletteCommand::Categories));
        assert!(all.contains(&PaletteCommand::CategoryAliases));
        assert!(all.contains(&PaletteCommand::Members));
    }

    #[test]
    fn filter_commands_matches_category_queries() {
        let commands = filter_commands("cat", &[]);
        assert!(commands.contains(&PaletteCommand::Categories));
        let commands = filter_commands("alias", &[]);
        assert!(commands.contains(&PaletteCommand::CategoryAliases));
        let commands = filter_commands("member", &[]);
        assert!(commands.contains(&PaletteCommand::Members));
    }

    #[test]
    fn filter_commands_prioritizes_mru_when_empty() {
        let mru = vec![PaletteCommand::Refresh, PaletteCommand::Categories];
        let commands = filter_commands("", &mru);
        // MRU commands should be first
        assert_eq!(commands[0], PaletteCommand::Refresh);
        assert_eq!(commands[1], PaletteCommand::Categories);
    }
}
