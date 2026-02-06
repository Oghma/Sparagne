//! Transaction list view rendering.
//!
//! Displays:
//! - Grouped transaction list (by date/category/wallet/envelope)
//! - Group headers with totals
//! - Individual transaction rows (2 lines each)
//! - Empty state messages
//! - Visual selection mode markers

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};
use std::collections::HashMap;

use crate::{
    app::{AppState, GroupingMode, transactions_visible_indices},
    ui::theme::Theme,
};

use super::{
    common::{
        amount_span, group_total_span, grouping_key_label, kind_chip,
        resolve_flow_name, resolve_wallet_name, signed_amount_minor, void_chip,
    },
    quick_add::render_quick_add,
};
use crate::ui::common::get_currency;

/// Renders the main transaction list with grouping
pub fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let quick_height = if state.transactions.quick_active {
        6
    } else {
        5
    };
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(quick_height), Constraint::Min(0)])
        .split(area);

    render_quick_add(frame, layout[0], state, theme);

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let currency = get_currency(state);

    let mut rows = Vec::new();
    let mut selected_row = None;

    let visible = transactions_visible_indices(state);
    if visible.is_empty() {
        render_empty_state(frame, layout[1], state, theme, list_block);
        return;
    }

    use chrono::Local;
    let today = Local::now().date_naive();
    let yesterday = today - chrono::Duration::days(1);
    let selected_tx_id = visible
        .get(state.transactions.selected)
        .and_then(|idx| state.transactions.items.get(*idx))
        .map(|tx| tx.id);

    struct GroupBucket {
        label: String,
        total_minor: i64,
        tx_indices: Vec<usize>,
    }

    let mut groups: Vec<GroupBucket> = Vec::new();
    let mut group_index: HashMap<String, usize> = HashMap::new();

    for idx in visible.iter().copied() {
        let tx = &state.transactions.items[idx];
        let (key, label) = grouping_key_label(
            state,
            tx,
            state.transactions.grouping_mode,
            today,
            yesterday,
        );
        let entry = if let Some(existing) = group_index.get(key.as_str()) {
            *existing
        } else {
            let next = groups.len();
            groups.push(GroupBucket {
                label,
                total_minor: 0,
                tx_indices: Vec::new(),
            });
            group_index.insert(key, next);
            next
        };

        groups[entry].total_minor += signed_amount_minor(tx.kind, tx.amount_minor);
        groups[entry].tx_indices.push(idx);
    }

    for group in groups {
        rows.push(render_group_header(
            state.transactions.grouping_mode,
            group.label.as_str(),
            group.total_minor,
            currency,
            theme,
        ));

        for idx in group.tx_indices {
            let tx = &state.transactions.items[idx];
            if selected_tx_id == Some(tx.id) {
                selected_row = Some(rows.len());
            }

            rows.push(render_transaction_row(state, tx, theme));
        }
    }

    let mut list_state = ListState::default();
    if let Some(row) = selected_row {
        list_state.select(Some(row));
    }

    let list = List::new(rows)
        .block(list_block)
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("» ");

    frame.render_stateful_widget(list, layout[1], &mut list_state);
}

/// Renders the empty state message when no transactions are visible
fn render_empty_state(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    list_block: Block<'_>,
) {
    let query = state.transactions.search.query.trim();
    let mut lines = Vec::new();
    if !query.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("No results for "),
            Span::styled(format!("\"{query}\""), Style::default().fg(theme.accent)),
            Span::raw("."),
        ]));
        lines.push(Line::from(Span::styled(
            "Ctrl+F to edit • Esc to clear",
            Style::default().fg(theme.text_muted),
        )));
    } else {
        lines.push(Line::from(vec![
            Span::raw("No transactions yet. Press "),
            Span::styled("n", Style::default().fg(theme.accent)),
            Span::raw(" to add one."),
        ]));
    }
    let empty_msg = Paragraph::new(lines)
        .alignment(ratatui::layout::Alignment::Center)
        .block(list_block);
    frame.render_widget(empty_msg, area);
}

/// Renders a group header with label and total
fn render_group_header(
    mode: GroupingMode,
    label: &str,
    total_minor: i64,
    currency: engine::Currency,
    theme: &Theme,
) -> ListItem<'static> {
    let title = match mode {
        GroupingMode::Date => label.to_string(),
        GroupingMode::Category => format!("Category: {label}"),
        GroupingMode::Wallet => format!("Wallet: {label}"),
        GroupingMode::Envelope => format!("Envelope: {label}"),
    };

    let spans = vec![
        Span::styled(
            format!("  {title}"),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        group_total_span(total_minor, currency, theme),
    ];

    ListItem::new(Line::from(spans))
}

/// Renders a single transaction row (2 lines)
fn render_transaction_row(
    state: &AppState,
    tx: &api_types::transaction::TransactionView,
    theme: &Theme,
) -> ListItem<'static> {
    let currency = get_currency(state);

    let note = tx.note.as_deref().unwrap_or("");
    let category = tx
        .category
        .as_deref()
        .map(|c| format!("#{c}"))
        .unwrap_or_default();

    let wallet_name = tx
        .wallet_id
        .map(|id| resolve_wallet_name(state, id))
        .unwrap_or_default();
    let flow_name = tx
        .flow_id
        .map(|id| resolve_flow_name(state, id))
        .unwrap_or_default();

    // Build 2-line transaction display
    // Line 1: time, amount with direction indicator, note
    let mut line1_spans = Vec::new();
    if state.transactions.visual_mode {
        let marker = if state.transactions.visual_selected.contains(&tx.id) {
            "*"
        } else {
            " "
        };
        line1_spans.push(Span::styled(marker, Style::default().fg(theme.warning)));
        line1_spans.push(Span::raw(" "));
    }
    if state.transactions.grouping_mode != GroupingMode::Date {
        line1_spans.push(Span::styled(
            tx.occurred_at.format("%d %b").to_string(),
            Style::default().fg(theme.text_muted),
        ));
        line1_spans.push(Span::raw(" "));
    }
    line1_spans.push(Span::styled(
        tx.occurred_at.format("%H:%M").to_string(),
        Style::default().fg(theme.text_muted),
    ));
    line1_spans.push(Span::raw("  "));
    line1_spans.push(kind_chip(tx.kind, theme));
    line1_spans.push(Span::raw(" "));
    if let Some(voided) = void_chip(tx.voided, theme) {
        line1_spans.push(voided);
        line1_spans.push(Span::raw(" "));
    }
    line1_spans.push(amount_span(tx.kind, tx.amount_minor, currency, theme));
    line1_spans.push(Span::raw(" "));
    line1_spans.push(Span::styled(note.to_string(), Style::default().fg(theme.text)));

    // Line 2: category, wallet, envelope (indented)
    let mut line2_spans = Vec::new();
    line2_spans.push(Span::raw("      ")); // Indentation to align with content
    if !category.is_empty() {
        line2_spans.push(Span::styled(category, Style::default().fg(theme.accent)));
        line2_spans.push(Span::raw("  "));
    }
    if !wallet_name.is_empty() {
        line2_spans.push(Span::styled(
            format!("@{wallet_name}"),
            Style::default().fg(theme.text_muted),
        ));
        line2_spans.push(Span::raw("  "));
    }
    if !flow_name.is_empty() {
        line2_spans.push(Span::styled(
            format!(">{flow_name}"),
            Style::default().fg(theme.info),
        ));
    }

    // Create 2-line list item
    let lines = vec![Line::from(line1_spans), Line::from(line2_spans)];
    let mut item = ListItem::new(lines);
    if tx.voided {
        item = item.style(Style::default().fg(theme.text_muted));
    }
    item
}
