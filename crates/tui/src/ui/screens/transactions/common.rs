//! Common utilities and helpers shared across transaction screen modules.
//!
//! This module contains:
//! - Display formatting helpers (amounts, dates, currencies)
//! - Color and style utilities
//! - Name resolution functions (wallet, flow)
//! - Shared constants and theme values

use api_types::transaction::TransactionKind;
use engine::{Currency, Money};
use ratatui::{
    style::{Modifier, Style},
    text::Span,
};

use crate::{
    app::{AppState, GroupingMode},
    text::{TextKey, t},
    ui::theme::Theme,
};

// Re-export consolidated functions so existing `super::common::X` imports
// in sibling modules continue to work.
pub(crate) use crate::ui::common::{format_date_label, resolve_flow_name, resolve_wallet_name};

/// Returns the scope label for the header (e.g., "All", "Wallet: Main", "Flow: Income")
pub fn scope_label(state: &AppState) -> String {
    let locale = state.locale;
    if let Some(flow_id) = state.transactions.scope_flow_id {
        return state
            .snapshot
            .as_ref()
            .and_then(|snap| {
                snap.flows
                    .iter()
                    .find(|flow| flow.id == flow_id)
                    .map(|flow| format!("Flow: {}", flow.name))
            })
            .unwrap_or_else(|| "Flow: ?".to_string());
    }

    if let Some(wallet_id) = state.transactions.scope_wallet_id {
        return state
            .snapshot
            .as_ref()
            .and_then(|snap| {
                snap.wallets
                    .iter()
                    .find(|wallet| wallet.id == wallet_id)
                    .map(|wallet| format!("Wallet: {}", wallet.name))
            })
            .unwrap_or_else(|| "Wallet: ?".to_string());
    }

    t(locale, TextKey::TxnScopeAll).to_string()
}

/// Returns a colored kind chip (icon) for the transaction kind
pub fn kind_chip(kind: TransactionKind, theme: &Theme) -> Span<'static> {
    let (icon, color) = crate::ui::common::tx_icon_color(kind, theme);
    Span::styled(icon.to_string(), Style::default().fg(color))
}

/// Returns a VOID chip if the transaction is voided
pub fn void_chip(voided: bool, theme: &Theme) -> Option<Span<'static>> {
    if voided {
        Some(Span::styled(
            "[VOID]",
            Style::default()
                .fg(theme.negative)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        None
    }
}

/// Formats an amount with sign based on transaction kind
pub fn amount_span(
    kind: TransactionKind,
    amount_minor: i64,
    currency: Currency,
    theme: &Theme,
) -> Span<'static> {
    let signed = match kind {
        TransactionKind::Expense => -amount_minor,
        TransactionKind::Income | TransactionKind::Refund => amount_minor,
        TransactionKind::TransferWallet | TransactionKind::TransferFlow => amount_minor,
    };
    let color = if signed < 0 {
        theme.negative
    } else if signed > 0 {
        theme.positive
    } else {
        theme.text_muted
    };
    let amount = Money::new(signed).format(currency);
    Span::styled(format!("{amount:<14}"), Style::default().fg(color))
}

/// Formats a leg amount (signed integer)
pub fn leg_amount_span(amount_minor: i64, currency: Currency, theme: &Theme) -> Span<'static> {
    let color = if amount_minor < 0 {
        theme.negative
    } else if amount_minor > 0 {
        theme.positive
    } else {
        theme.text_muted
    };
    let amount = Money::new(amount_minor).format(currency);
    Span::styled(amount, Style::default().fg(color))
}

/// Formats a group total with sign and color
pub fn group_total_span(total_minor: i64, currency: Currency, theme: &Theme) -> Span<'static> {
    let color = if total_minor >= 0 {
        theme.positive
    } else {
        theme.negative
    };
    Span::styled(
        Money::new(total_minor).format(currency),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}


/// Returns recent wallet names
pub fn recent_wallet_names(state: &AppState) -> Vec<String> {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return Vec::new();
    };
    state
        .transactions
        .recent_wallet_ids
        .iter()
        .filter_map(|wallet_id| {
            snapshot
                .wallets
                .iter()
                .find(|wallet| wallet.id == *wallet_id && !wallet.archived)
                .map(|wallet| wallet.name.clone())
        })
        .collect()
}

/// Returns recent flow names
pub fn recent_flow_names(state: &AppState) -> Vec<String> {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return Vec::new();
    };
    state
        .transactions
        .recent_flow_ids
        .iter()
        .filter_map(|flow_id| {
            snapshot
                .flows
                .iter()
                .find(|flow| flow.id == *flow_id && !flow.archived)
                .map(|flow| flow.name.clone())
        })
        .collect()
}

// Re-export from app layer where the business logic now lives.
pub(crate) use crate::app::default_wallet_flow_names;

/// Converts a transaction kind and amount to a signed amount (for totals)
pub fn signed_amount_minor(kind: TransactionKind, amount_minor: i64) -> i64 {
    if kind == TransactionKind::Expense {
        -amount_minor.abs()
    } else {
        amount_minor
    }
}

/// Returns the grouping key and label for a transaction based on grouping mode
pub fn grouping_key_label(
    state: &AppState,
    tx: &api_types::transaction::TransactionView,
    mode: GroupingMode,
    today: chrono::NaiveDate,
    yesterday: chrono::NaiveDate,
) -> (String, String) {
    match mode {
        GroupingMode::Date => {
            let date = tx.occurred_at.date_naive();
            (
                date.format("%Y-%m-%d").to_string(),
                format_date_label(date, today, yesterday),
            )
        }
        GroupingMode::Category => {
            let label = tx
                .category
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(t(state.locale, TextKey::TxnUncategorized))
                .to_string();
            (label.clone(), label)
        }
        GroupingMode::Wallet => {
            if let Some(id) = tx.wallet_id {
                (format!("wallet:{id}"), resolve_wallet_name(state, id))
            } else {
                ("wallet:none".to_string(), t(state.locale, TextKey::TxnNoWallet).to_string())
            }
        }
        GroupingMode::Envelope => {
            if let Some(id) = tx.flow_id {
                (format!("flow:{id}"), resolve_flow_name(state, id))
            } else {
                ("flow:none".to_string(), t(state.locale, TextKey::TxnNoEnvelope).to_string())
            }
        }
    }
}


/// Builds a recents summary line for the form footer
pub fn recents_line(state: &AppState) -> Option<String> {
    let locale = state.locale;
    let mut parts = Vec::new();
    let categories = state
        .transactions
        .recent_categories
        .iter()
        .take(3)
        .map(|cat| format!("#{cat}"))
        .collect::<Vec<_>>();
    if !categories.is_empty() {
        parts.push(format!("{}{}", t(locale, TextKey::TxnRecentsCategories), categories.join(" ")));
    }

    let wallets = recent_wallet_names(state)
        .into_iter()
        .take(3)
        .collect::<Vec<_>>();
    if !wallets.is_empty() {
        parts.push(format!("{}{}", t(locale, TextKey::TxnRecentsWallet), wallets.join(", ")));
    }

    let flows = recent_flow_names(state).into_iter().take(3).collect::<Vec<_>>();
    if !flows.is_empty() {
        parts.push(format!("{}{}", t(locale, TextKey::TxnRecentsFlow), flows.join(", ")));
    }

    if parts.is_empty() {
        None
    } else {
        Some(format!("{}{}", t(locale, TextKey::TxnRecentsPrefix), parts.join(" • ")))
    }
}
