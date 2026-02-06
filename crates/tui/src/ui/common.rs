//! Common UI utilities shared across rendering modules.
//!
//! This module consolidates duplicated helper functions that were previously
//! scattered across multiple screen-specific `common.rs` files. It provides:
//!
//! - Currency conversion helpers
//! - String truncation and text highlighting
//! - Name resolution functions (wallet, flow)
//! - Layout helpers (inset)
//! - Progress bar rendering
//! - Date label formatting
//! - Transaction type icon constants

use api_types::transaction::TransactionKind;
use engine::Currency;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};
use uuid::Uuid;

use crate::{app::AppState, ui::theme::Theme};

// ---------------------------------------------------------------------------
// Icon constants
// ---------------------------------------------------------------------------

/// Icon for income transactions.
pub(crate) const ICON_INCOME: &str = "\u{25b2}";
/// Icon for expense transactions.
pub(crate) const ICON_EXPENSE: &str = "\u{25bc}";
/// Icon for refund transactions.
pub(crate) const ICON_REFUND: &str = "\u{21a9}";
/// Icon for transfer transactions (wallet-to-wallet or flow-to-flow).
pub(crate) const ICON_TRANSFER: &str = "\u{21c4}";

// ---------------------------------------------------------------------------
// Currency helpers
// ---------------------------------------------------------------------------

/// Resolves the currency for the current vault, falling back to EUR.
pub(crate) fn get_currency(state: &AppState) -> Currency {
    state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(|c| match c {
            api_types::Currency::Eur => Currency::Eur,
        })
        .unwrap_or(Currency::Eur)
}

// ---------------------------------------------------------------------------
// String helpers
// ---------------------------------------------------------------------------

/// Truncates a string to the given maximum length, adding an ellipsis if needed.
///
/// Uses character-level counting so multi-byte characters are handled correctly.
pub(crate) fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!(
            "{}\u{2026}",
            s.chars().take(max_len.saturating_sub(1)).collect::<String>()
        )
    }
}

/// Highlights characters in `label` that fuzzy-match `query`.
///
/// Returns a sequence of [`Span`]s where matched characters are rendered
/// with the theme accent colour and bold modifier.
pub(crate) fn highlight_matches<'a>(label: &str, query: &str, theme: &Theme) -> Vec<Span<'a>> {
    if query.is_empty() {
        return vec![Span::styled(
            label.to_string(),
            Style::default().fg(theme.text),
        )];
    }

    let query_lower = query.to_lowercase();
    let label_lower = label.to_lowercase();

    let mut spans = Vec::new();
    let mut last_end = 0;
    let mut query_chars = query_lower.chars().peekable();

    for (i, c) in label_lower.char_indices() {
        if query_chars.peek() == Some(&c) {
            // Add non-matching prefix
            if i > last_end {
                spans.push(Span::styled(
                    label[last_end..i].to_string(),
                    Style::default().fg(theme.text),
                ));
            }
            // Add matching character with highlight
            let char_end = i + c.len_utf8();
            spans.push(Span::styled(
                label[i..char_end].to_string(),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
            last_end = char_end;
            query_chars.next();
        }
    }

    // Add remaining non-matching suffix
    if last_end < label.len() {
        spans.push(Span::styled(
            label[last_end..].to_string(),
            Style::default().fg(theme.text),
        ));
    }

    spans
}

// ---------------------------------------------------------------------------
// Name resolution
// ---------------------------------------------------------------------------

/// Resolves a wallet name from its ID using the current snapshot.
///
/// Falls back to the string representation of the UUID when the wallet is
/// not found.
pub(crate) fn resolve_wallet_name(state: &AppState, wallet_id: Uuid) -> String {
    state
        .snapshot
        .as_ref()
        .and_then(|snap| {
            snap.wallets
                .iter()
                .find(|wallet| wallet.id == wallet_id)
                .map(|wallet| wallet.name.clone())
        })
        .unwrap_or_else(|| wallet_id.to_string())
}

/// Resolves a flow name from its ID using the current snapshot.
///
/// Falls back to the string representation of the UUID when the flow is
/// not found.
pub(crate) fn resolve_flow_name(state: &AppState, flow_id: Uuid) -> String {
    state
        .snapshot
        .as_ref()
        .and_then(|snap| {
            snap.flows
                .iter()
                .find(|flow| flow.id == flow_id)
                .map(|flow| flow.name.clone())
        })
        .unwrap_or_else(|| flow_id.to_string())
}

// ---------------------------------------------------------------------------
// Progress bar
// ---------------------------------------------------------------------------

/// Renders a text-based progress bar with filled (`\u{2588}`) and empty (`\u{2591}`) blocks.
pub(crate) fn progress_bar(value: i64, max: i64, width: usize) -> String {
    if max == 0 {
        return "\u{2591}".repeat(width);
    }

    let ratio = (value.unsigned_abs() as f64 / max.unsigned_abs() as f64).clamp(0.0, 1.0);
    let filled = ((ratio * width as f64) as usize).min(width);
    let empty = width.saturating_sub(filled);

    format!("{}{}", "\u{2588}".repeat(filled), "\u{2591}".repeat(empty))
}

// ---------------------------------------------------------------------------
// Date formatting
// ---------------------------------------------------------------------------

/// Formats a date label with special handling for today and yesterday.
///
/// - Today -> `"Today"`
/// - Yesterday -> `"Yesterday"`
/// - Same year -> `"%A, %d %b"` (e.g. "Monday, 03 Feb")
/// - Different year -> `"%d %b %Y"` (e.g. "03 Feb 2024")
pub(crate) fn format_date_label(
    date: chrono::NaiveDate,
    today: chrono::NaiveDate,
    yesterday: chrono::NaiveDate,
) -> String {
    use chrono::Datelike;
    if date == today {
        "Today".to_string()
    } else if date == yesterday {
        "Yesterday".to_string()
    } else if date.year() == today.year() {
        date.format("%A, %d %b").to_string()
    } else {
        date.format("%d %b %Y").to_string()
    }
}

// ---------------------------------------------------------------------------
// Layout helpers
// ---------------------------------------------------------------------------

/// Returns a rect inset by the given horizontal and vertical margins.
pub(crate) fn inset(area: Rect, horizontal: u16, vertical: u16) -> Rect {
    let x = area.x.saturating_add(horizontal);
    let y = area.y.saturating_add(vertical);
    let width = area.width.saturating_sub(horizontal.saturating_mul(2));
    let height = area.height.saturating_sub(vertical.saturating_mul(2));
    Rect {
        x,
        y,
        width,
        height,
    }
}

// ---------------------------------------------------------------------------
// Transaction display helpers
// ---------------------------------------------------------------------------

/// Returns the icon string and color for a transaction kind.
pub(crate) fn tx_icon_color(kind: TransactionKind, theme: &Theme) -> (&'static str, ratatui::style::Color) {
    match kind {
        TransactionKind::Income => (ICON_INCOME, theme.income),
        TransactionKind::Expense => (ICON_EXPENSE, theme.expense),
        TransactionKind::Refund => (ICON_REFUND, theme.refund),
        TransactionKind::TransferWallet | TransactionKind::TransferFlow => {
            (ICON_TRANSFER, theme.transfer)
        }
    }
}

/// Returns the color to use for a transaction amount based on its kind.
pub(crate) fn tx_amount_color(kind: TransactionKind, theme: &Theme) -> ratatui::style::Color {
    match kind {
        TransactionKind::Income | TransactionKind::Refund => theme.positive,
        TransactionKind::Expense => theme.negative,
        _ => theme.text,
    }
}

// ---------------------------------------------------------------------------
// Label-value field
// ---------------------------------------------------------------------------

/// Renders a label-value pair as a styled line, with bold highlighting when focused.
pub(crate) fn render_label_value_field(
    label: &str,
    value: &str,
    focused: bool,
    theme: &Theme,
) -> Line<'static> {
    let label_style = if focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };
    let value_style = if focused {
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };
    Line::from(vec![
        Span::styled(format!("{label:<8}"), label_style),
        Span::raw(": "),
        Span::styled(value.to_string(), value_style),
    ])
}
