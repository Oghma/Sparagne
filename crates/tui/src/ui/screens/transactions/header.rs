//! Transaction list header rendering.
//!
//! Displays:
//! - Grouping mode and scope information
//! - Toggle states (voided, transfers)
//! - Active filters summary
//! - Search query
//! - Keyboard hints

use api_types::transaction::TransactionKind;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::{
    app::{AppState, GroupingMode},
    text::{TextKey, t},
    ui::theme::Theme,
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
    let title = format!(" Transactions (Group: {grouping_label}, Scope: {scope}) ");

    // Row 1: Voided toggle, Transfers toggle, Filters status
    let voided_status = if state.transactions.include_voided {
        Span::styled("[On]", Style::default().fg(theme.positive))
    } else {
        Span::styled("[Off]", Style::default().fg(theme.text_muted))
    };
    let transfers_status = if state.transactions.include_transfers {
        Span::styled("[On]", Style::default().fg(theme.positive))
    } else {
        Span::styled("[Off]", Style::default().fg(theme.text_muted))
    };

    let mut line1 = vec![
        Span::styled(t(locale, TextKey::TxnHeaderVoided), Style::default().fg(theme.text_muted)),
        voided_status,
        Span::raw("  "),
        Span::styled(t(locale, TextKey::TxnHeaderTransfers), Style::default().fg(theme.text_muted)),
        transfers_status,
        Span::raw("     │     "),
    ];

    // Add filter status
    if let Some(summary) = filter_summary(state) {
        line1.push(Span::styled(
            format!("Filters [{summary}]"),
            Style::default().fg(theme.warning),
        ));
    } else {
        line1.push(Span::styled(t(locale, TextKey::TxnHeaderFiltersOff), Style::default().fg(theme.text_muted)));
    }

    // Row 2: Search field and hints
    let search_query = state.transactions.search.query.trim();
    let mut line2 = vec![];

    if !search_query.is_empty() || state.transactions.search.active {
        line2.push(Span::styled(t(locale, TextKey::TxnHeaderSearch), Style::default().fg(theme.text_muted)));
        let shown = if search_query.is_empty() {
            "…"
        } else {
            search_query
        };
        let mut style = Style::default().fg(theme.text);
        if state.transactions.search.active {
            style = style.fg(theme.accent).add_modifier(Modifier::BOLD);
        }
        line2.push(Span::styled(format!("\"{shown}\""), style));
        line2.push(Span::raw("  "));
    }

    line2.push(Span::styled(
        t(locale, TextKey::TxnHeaderHints),
        Style::default().fg(theme.text_muted),
    ));

    // Add error if present
    if let Some(err) = &state.transactions.error {
        line2.push(Span::raw("  "));
        line2.push(Span::styled(err.as_str(), Style::default().fg(theme.negative)));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(title, Style::default().fg(theme.accent)));

    let content = Paragraph::new(vec![Line::from(line1), Line::from(line2)]).block(block);
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
    if parts.is_empty() {
        None
    } else {
        Some(format!("Filters: {}", parts.join(" • ")))
    }
}
