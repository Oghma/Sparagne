//! Transaction list header rendering.
//!
//! Displays:
//! - Grouping mode and scope information
//! - Active filters summary

use api_types::transaction::TransactionKind;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::{AppState, GroupingMode},
    text::{TextKey, t},
    ui::{common::themed_block, theme::Theme},
};

use super::common::scope_label;

/// Renders the header area with filters, search, and hints
pub fn render_header(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let locale = state.locale;

    // Determine grouping mode label
    let grouping_label = match state.transactions.grouping_mode {
        GroupingMode::Date => t(locale, TextKey::TxnGroupDate),
        GroupingMode::Category => t(locale, TextKey::TxnGroupCategory),
        GroupingMode::Wallet => t(locale, TextKey::TxnGroupWallet),
        GroupingMode::Envelope => t(locale, TextKey::TxnGroupEnvelope),
    };

    let scope = scope_label(state);

    // Build title with grouping and scope info
    let title = format!("Transactions (Group: {grouping_label}, Scope: {scope})");

    // Row 1: Filter summary only
    let line1 = if let Some(summary) = filter_summary(state) {
        vec![Span::styled(
            format!("Filters [{summary}]"),
            Style::default().fg(theme.warning),
        )]
    } else {
        vec![Span::styled(
            t(locale, TextKey::TxnHeaderFiltersOff),
            Style::default().fg(theme.text_muted),
        )]
    };

    let block = themed_block(&title, theme.border, theme);

    let content = Paragraph::new(vec![Line::from(line1)]).block(block);
    frame.render_widget(content, area);
}

/// Returns a summary of active filters
fn filter_summary(state: &AppState) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(from) = state.transactions.filter_from {
        parts.push(format!("from {}", from.format("%Y-%m-%d")));
    }
    if let Some(to) = state.transactions.filter_to {
        parts.push(format!("to {}", to.format("%Y-%m-%d")));
    }
    if let Some(kinds) = state.transactions.filter_kinds.as_ref()
        && !kinds.is_empty()
    {
        let labels = kinds
            .iter()
            .map(|kind| match kind {
                TransactionKind::Income => "inc",
                TransactionKind::Expense => "exp",
                TransactionKind::Refund => "ref",
                TransactionKind::TransferWallet => "tw",
                TransactionKind::TransferFlow => "tf",
            })
            .collect::<Vec<_>>()
            .join(",");
        parts.push(format!("kinds {labels}"));
    }
    if !state.transactions.include_transfers {
        parts.push("transfers off".to_string());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" \u{2022} "))
    }
}
