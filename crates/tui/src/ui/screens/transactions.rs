use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use api_types::transaction::{LegTarget, TransactionKind};
use engine::{Currency, Money};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    app::{
        AppState, FilterField, GroupingMode, QuickAddAmbiguousKind, TransactionFormField,
        TransactionsMode, TransferField, flow_name_suggestions, ordered_flow_ids_from_state,
        ordered_wallet_ids_from_state, resolve_category_matches, resolve_flow_matches,
        resolve_wallet_matches, transactions_visible_indices,
    },
    ui::{components::centered_rect, theme::Theme},
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);

    render_header(frame, layout[0], state);
    match state.transactions.mode {
        TransactionsMode::List
        | TransactionsMode::PickWallet
        | TransactionsMode::PickFlow
        | TransactionsMode::TransferPicker
        | TransactionsMode::TransferWallet
        | TransactionsMode::TransferFlow
        | TransactionsMode::Filter
        | TransactionsMode::Form
        | TransactionsMode::Edit => {
            render_list(frame, layout[1], state, &theme);
            if matches!(
                state.transactions.mode,
                TransactionsMode::PickWallet | TransactionsMode::PickFlow
            ) {
                render_scope_picker(frame, layout[1], state, &theme);
            } else if state.transactions.mode == TransactionsMode::TransferPicker {
                render_transfer_picker(frame, layout[1], state, &theme);
            } else if matches!(
                state.transactions.mode,
                TransactionsMode::TransferWallet | TransactionsMode::TransferFlow
            ) {
                render_transfer_form(frame, layout[1], state, &theme);
            } else if matches!(
                state.transactions.mode,
                TransactionsMode::Form | TransactionsMode::Edit
            ) {
                render_transaction_form(frame, layout[1], state, &theme);
            } else if state.transactions.mode == TransactionsMode::Filter {
                render_filter_form(frame, layout[1], state, &theme);
            }
        }
        TransactionsMode::Detail => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(layout[1]);
            render_list(frame, columns[0], state, &theme);
            render_detail(frame, columns[1], state, &theme);
        }
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();

    // Determine grouping mode label
    let grouping_label = match state.transactions.grouping_mode {
        GroupingMode::Date => "Date",
        GroupingMode::Category => "Category",
        GroupingMode::Wallet => "Wallet",
        GroupingMode::Envelope => "Envelope",
    };

    let scope = scope_label(state);

    // Build title with grouping and scope info
    let title = format!(" Transactions (Group: {grouping_label}, Scope: {scope}) ");

    // Row 1: Voided toggle, Transfers toggle, Filters status
    let voided_status = if state.transactions.include_voided {
        Span::styled("[On]", Style::default().fg(theme.positive))
    } else {
        Span::styled("[Off]", Style::default().fg(theme.dim))
    };
    let transfers_status = if state.transactions.include_transfers {
        Span::styled("[On]", Style::default().fg(theme.positive))
    } else {
        Span::styled("[Off]", Style::default().fg(theme.dim))
    };

    let mut line1 = vec![
        Span::styled("Voided ", Style::default().fg(theme.text_muted)),
        voided_status,
        Span::raw("  "),
        Span::styled("Transfers ", Style::default().fg(theme.text_muted)),
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
        line1.push(Span::styled("Filters [off]", Style::default().fg(theme.dim)));
    }

    // Row 2: Search field and hints
    let search_query = state.transactions.search_query.trim();
    let mut line2 = vec![];

    if !search_query.is_empty() || state.transactions.search_active {
        line2.push(Span::styled("Search: ", Style::default().fg(theme.text_muted)));
        let shown = if search_query.is_empty() {
            "…"
        } else {
            search_query
        };
        let mut style = Style::default().fg(theme.text);
        if state.transactions.search_active {
            style = style.fg(theme.accent).add_modifier(Modifier::BOLD);
        }
        line2.push(Span::styled(format!("\"{shown}\""), style));
        line2.push(Span::raw("  "));
    }

    line2.push(Span::styled(
        "[Ctrl+F] search  [g] group  [f] filters  [w/W] scope",
        Style::default().fg(theme.dim),
    ));

    // Add error if present
    if let Some(err) = &state.transactions.error {
        line2.push(Span::raw("  "));
        line2.push(Span::styled(err.as_str(), Style::default().fg(theme.error)));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(title, Style::default().fg(theme.accent)));

    let content = Paragraph::new(vec![Line::from(line1), Line::from(line2)]).block(block);
    frame.render_widget(content, area);
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
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

    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur);

    let mut rows = Vec::new();
    let mut selected_row = None;

    let visible = transactions_visible_indices(state);
    if visible.is_empty() {
        let query = state.transactions.search_query.trim();
        let mut lines = Vec::new();
        if !query.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("No results for "),
                Span::styled(format!("\"{query}\""), Style::default().fg(theme.accent)),
                Span::raw("."),
            ]));
            lines.push(Line::from(Span::styled(
                "Ctrl+F to edit • Esc to clear",
                Style::default().fg(theme.dim),
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
        frame.render_widget(empty_msg, layout[1]);
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
        rows.push(group_header_item(
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
                    Style::default().fg(theme.dim),
                ));
                line1_spans.push(Span::raw(" "));
            }
            line1_spans.push(Span::styled(
                tx.occurred_at.format("%H:%M").to_string(),
                Style::default().fg(theme.dim),
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
                    Style::default().fg(theme.dim),
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
                item = item.style(Style::default().fg(theme.dim));
            }
            rows.push(item);
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

fn render_scope_picker(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return;
    };

    let (title, items) = match state.transactions.mode {
        TransactionsMode::PickWallet => {
            let mut list = vec![ListItem::new(Line::from("All wallets"))];
            for wallet in &snapshot.wallets {
                let archived = if wallet.archived { " (archived)" } else { "" };
                list.push(ListItem::new(Line::from(format!(
                    "{}{archived}",
                    wallet.name
                ))));
            }
            ("Select wallet scope", list)
        }
        TransactionsMode::PickFlow => {
            let mut list = vec![ListItem::new(Line::from("All flows"))];
            for flow in &snapshot.flows {
                let archived = if flow.archived { " (archived)" } else { "" };
                let marker = if flow.is_unallocated {
                    " [Unallocated]"
                } else {
                    ""
                };
                list.push(ListItem::new(Line::from(format!(
                    "{}{marker}{archived}",
                    flow.name
                ))));
            }
            ("Select flow scope", list)
        }
        _ => return,
    };

    let popup_area = centered_rect(60, 60, area);
    frame.render_widget(Clear, popup_area);

    let mut list_state = ListState::default();
    if !items.is_empty() {
        list_state.select(Some(state.transactions.picker_index));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.accent))
                .style(Style::default().bg(theme.background)),
        )
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("» ");

    frame.render_stateful_widget(list, popup_area, &mut list_state);
}

fn render_transfer_picker(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let items = vec![
        ListItem::new(Line::from("Wallet Transfer")),
        ListItem::new(Line::from("Flow Transfer")),
    ];

    let popup_area = centered_rect(40, 25, area);
    frame.render_widget(Clear, popup_area);

    let mut list_state = ListState::default();
    list_state.select(Some(state.transactions.picker_index));

    let list = List::new(items)
        .block(
            Block::default()
                .title("Transfer Type")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.accent))
                .style(Style::default().bg(theme.background)),
        )
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("» ");

    frame.render_stateful_widget(list, popup_area, &mut list_state);
}

fn render_transfer_form(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return;
    };
    let (title, items) = match state.transactions.mode {
        TransactionsMode::TransferWallet => {
            let list = snapshot
                .wallets
                .iter()
                .filter(|wallet| !wallet.archived)
                .map(|wallet| wallet.name.clone())
                .collect::<Vec<_>>();
            if state.transactions.transfer.editing_id.is_some() {
                ("Edit Transfer Wallet", list)
            } else {
                ("Transfer Wallet", list)
            }
        }
        TransactionsMode::TransferFlow => {
            let list = snapshot
                .flows
                .iter()
                .filter(|flow| !flow.archived)
                .map(|flow| flow.name.clone())
                .collect::<Vec<_>>();
            if state.transactions.transfer.editing_id.is_some() {
                ("Edit Transfer Flow", list)
            } else {
                ("Transfer Flow", list)
            }
        }
        _ => return,
    };

    let transfer = &state.transactions.transfer;
    let from = items
        .get(transfer.from_index)
        .map(|name| name.as_str())
        .unwrap_or("-");
    let to = items
        .get(transfer.to_index)
        .map(|name| name.as_str())
        .unwrap_or("-");

    let popup = centered_rect(70, 60, area);
    frame.render_widget(Clear, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(0)])
        .split(popup);

    let mut lines = vec![
        render_transfer_field("From", from, transfer.focus == TransferField::From, theme),
        render_transfer_field("To", to, transfer.focus == TransferField::To, theme),
        render_transfer_field(
            "Amount",
            transfer.amount.value(),
            transfer.focus == TransferField::Amount,
            theme,
        ),
        render_transfer_field(
            "Note",
            transfer.note.value(),
            transfer.focus == TransferField::Note,
            theme,
        ),
        render_transfer_field(
            "When",
            if transfer.occurred_at.value.trim().is_empty() {
                "-"
            } else {
                transfer.occurred_at.value.as_str()
            },
            transfer.focus == TransferField::OccurredAt,
            theme,
        ),
        Line::from(Span::styled(
            "Tab: next • ↑/↓: change • Enter: save • Esc: cancel",
            Style::default().fg(theme.dim),
        )),
    ];

    if let Some(err) = transfer.error.as_ref() {
        lines.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(theme.error),
        )));
    }

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.background));
    frame.render_widget(Paragraph::new(lines).block(block), layout[0]);

    let hint_block = Block::default()
        .title("Available")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.background));
    let list_items = items
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            let marker = if idx == transfer.from_index {
                " [from]"
            } else if idx == transfer.to_index {
                " [to]"
            } else {
                ""
            };
            ListItem::new(Line::from(format!("{name}{marker}")))
        })
        .collect::<Vec<_>>();

    let list = List::new(list_items).block(hint_block);
    frame.render_widget(list, layout[1]);
}

fn render_transfer_field(label: &str, value: &str, focused: bool, theme: &Theme) -> Line<'static> {
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

fn render_transaction_form(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return;
    };
    let form = &state.transactions.form;
    let wallet_ids = ordered_wallet_ids_from_state(state);
    let flow_ids = ordered_flow_ids_from_state(state);

    let wallets = wallet_ids
        .iter()
        .filter_map(|id| snapshot.wallets.iter().find(|wallet| wallet.id == *id))
        .collect::<Vec<_>>();
    let flows = flow_ids
        .iter()
        .filter_map(|id| snapshot.flows.iter().find(|flow| flow.id == *id))
        .collect::<Vec<_>>();

    let wallet_name = wallets
        .get(form.wallet_index)
        .map(|wallet| wallet.name.as_str())
        .unwrap_or("-");
    let flow_name = flows
        .get(form.flow_index)
        .map(|flow| flow.name.as_str())
        .unwrap_or("-");

    let category_raw = form.category.value().trim().trim_start_matches('#');
    let category = if category_raw.is_empty() {
        "-".to_string()
    } else {
        format!("#{category_raw}")
    };
    let note = if form.note.value().trim().is_empty() {
        "-".to_string()
    } else {
        form.note.value().trim().to_string()
    };
    let occurred_at = if form.occurred_at.value.trim().is_empty() {
        "-".to_string()
    } else {
        form.occurred_at.value.trim().to_string()
    };

    let is_edit = form.editing_id.is_some();
    let title = match form.kind {
        TransactionKind::Income => {
            if is_edit {
                "Edit Income"
            } else {
                "New Income"
            }
        }
        TransactionKind::Expense => {
            if is_edit {
                "Edit Expense"
            } else {
                "New Expense"
            }
        }
        TransactionKind::Refund => {
            if is_edit {
                "Edit Refund"
            } else {
                "New Refund"
            }
        }
        TransactionKind::TransferWallet | TransactionKind::TransferFlow => {
            if is_edit {
                "Edit Transaction"
            } else {
                "New Transaction"
            }
        }
    };

    let popup = centered_rect(70, 70, area);
    frame.render_widget(Clear, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(0)])
        .split(popup);

    let mut lines = vec![
        render_transaction_field(
            "Amount",
            form.amount.value(),
            form.focus == TransactionFormField::Amount,
            "Enter numerical amount (required)",
            theme,
        ),
        render_transaction_field(
            "Wallet",
            wallet_name,
            form.focus == TransactionFormField::Wallet,
            "Source/destination wallet",
            theme,
        ),
        render_transaction_field(
            "Flow",
            flow_name,
            form.focus == TransactionFormField::Flow,
            "Envelope/budget allocation",
            theme,
        ),
        render_transaction_field(
            "Category",
            category.as_str(),
            form.focus == TransactionFormField::Category,
            "Tag for analytics",
            theme,
        ),
        render_transaction_field(
            "Note",
            note.as_str(),
            form.focus == TransactionFormField::Note,
            "Optional description",
            theme,
        ),
        render_transaction_field(
            "When",
            occurred_at.as_str(),
            form.focus == TransactionFormField::OccurredAt,
            "Date & time (default: now)",
            theme,
        ),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Enter]", Style::default().fg(theme.accent)),
            Span::styled(" Save  ", Style::default().fg(theme.text_muted)),
            Span::styled("[Esc]", Style::default().fg(theme.accent)),
            Span::styled(" Cancel  ", Style::default().fg(theme.text_muted)),
            Span::styled("[Tab]", Style::default().fg(theme.accent)),
            Span::styled(" Next field  ", Style::default().fg(theme.text_muted)),
            Span::styled("[↑↓]", Style::default().fg(theme.accent)),
            Span::styled(" Cycle choices", Style::default().fg(theme.text_muted)),
        ]),
    ];

    if let Some(err) = form.error.as_ref() {
        lines.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(theme.error),
        )));
    }

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.background));
    frame.render_widget(Paragraph::new(lines).block(block), layout[0]);

    let bottom_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(5),
            Constraint::Length(2),
        ])
        .split(layout[1]);

    let list_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(bottom_layout[0]);

    render_picker_list(
        frame,
        list_layout[0],
        "Wallets",
        wallets
            .iter()
            .map(|wallet| wallet.name.as_str())
            .collect::<Vec<_>>(),
        form.wallet_index,
        form.focus == TransactionFormField::Wallet,
        theme,
    );
    render_picker_list(
        frame,
        list_layout[1],
        "Flows",
        flows
            .iter()
            .map(|flow| flow.name.as_str())
            .collect::<Vec<_>>(),
        form.flow_index,
        form.focus == TransactionFormField::Flow,
        theme,
    );

    render_category_list(frame, bottom_layout[1], state, theme);
    render_recents_footer(frame, bottom_layout[2], state, theme);
}

fn render_picker_list(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: Vec<&str>,
    selected: usize,
    focused: bool,
    theme: &Theme,
) {
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.background));
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from("Nessun elemento."))
                .alignment(ratatui::layout::Alignment::Center)
                .block(block),
            area,
        );
        return;
    }

    let items = items
        .into_iter()
        .map(|name| ListItem::new(Line::from(name.to_string())))
        .collect::<Vec<_>>();
    let mut list_state = ListState::default();
    list_state.select(Some(selected.min(items.len() - 1)));

    let highlight_style = if focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };

    let list = List::new(items)
        .block(block)
        .highlight_style(highlight_style)
        .highlight_symbol("» ");
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_category_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let block = Block::default()
        .title("Categorie recenti")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.background));

    if state.transactions.recent_categories.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from("Nessuna categoria recente."))
                .alignment(ratatui::layout::Alignment::Center)
                .block(block),
            area,
        );
        return;
    }

    let items = state
        .transactions
        .recent_categories
        .iter()
        .map(|category| ListItem::new(Line::from(format!("#{category}"))))
        .collect::<Vec<_>>();

    let mut list_state = ListState::default();
    if let Some(idx) = state.transactions.form.category_index {
        list_state.select(Some(idx.min(items.len() - 1)));
    }

    let highlight_style = if state.transactions.form.focus == TransactionFormField::Category {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };

    let list = List::new(items)
        .block(block)
        .highlight_style(highlight_style)
        .highlight_symbol("» ");
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_transaction_field(
    label: &str,
    value: &str,
    focused: bool,
    helper: &str,
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
    let cursor = if focused { "▏" } else { "" };
    let helper_style = if focused {
        Style::default().fg(theme.text_muted)
    } else {
        Style::default().fg(theme.dim)
    };
    Line::from(vec![
        Span::styled(format!("{label:<10}"), label_style),
        Span::styled(format!("[{value}{cursor}]"), value_style),
        Span::raw("  "),
        Span::styled(format!("← {helper}"), helper_style),
    ])
}

fn render_filter_form(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let filter = &state.transactions.filter;
    let popup = centered_rect(75, 60, area);
    frame.render_widget(Clear, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(11), Constraint::Min(0)])
        .split(popup);

    let kinds_focused = filter.focus == FilterField::Kinds;
    let kinds_label_style = if kinds_focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };

    let mut lines = vec![
        render_filter_field(
            "From",
            filter.from_input.as_str(),
            filter.focus == FilterField::From,
            theme,
        ),
        render_filter_field(
            "To",
            filter.to_input.as_str(),
            filter.focus == FilterField::To,
            theme,
        ),
        Line::from(""),
        Line::from(vec![
            Span::styled("Transaction Types ", kinds_label_style),
            Span::styled("(press key to toggle)", Style::default().fg(theme.dim)),
        ]),
        // Row 1: Income, Expense, Refund
        Line::from(vec![
            Span::raw("  "),
            filter_toggle_with_icon("▲", "Income", "i", filter.kind_income, theme),
            Span::raw("    "),
            filter_toggle_with_icon("▼", "Expense", "e", filter.kind_expense, theme),
            Span::raw("    "),
            filter_toggle_with_icon("↩", "Refund", "r", filter.kind_refund, theme),
        ]),
        // Row 2: Transfers
        Line::from(vec![
            Span::raw("  "),
            filter_toggle_with_icon("⇄", "Wallet Transfer", "w", filter.kind_transfer_wallet, theme),
            Span::raw("    "),
            filter_toggle_with_icon("⇄", "Flow Transfer", "f", filter.kind_transfer_flow, theme),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Tab]", Style::default().fg(theme.accent)),
            Span::styled(" next  ", Style::default().fg(theme.text_muted)),
            Span::styled("[Enter]", Style::default().fg(theme.accent)),
            Span::styled(" apply  ", Style::default().fg(theme.text_muted)),
            Span::styled("[Esc]", Style::default().fg(theme.accent)),
            Span::styled(" cancel", Style::default().fg(theme.text_muted)),
        ]),
    ];

    if let Some(err) = filter.error.as_ref() {
        lines.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(theme.error),
        )));
    }

    let block = Block::default()
        .title(Span::styled(" Filters ", Style::default().fg(theme.accent)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.background));
    frame.render_widget(Paragraph::new(lines).block(block), layout[0]);
}

/// Renders a filter toggle with icon, label, and key hint
fn filter_toggle_with_icon(
    icon: &str,
    label: &str,
    key: &str,
    enabled: bool,
    theme: &Theme,
) -> Span<'static> {
    let (checkbox, style) = if enabled {
        ("[✓]", Style::default().fg(theme.positive))
    } else {
        ("[✗]", Style::default().fg(theme.dim))
    };
    let text = format!("{checkbox} {icon} {label} ({key})");
    Span::styled(text, style)
}

fn render_filter_field(label: &str, value: &str, focused: bool, theme: &Theme) -> Line<'static> {
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

fn render_recents_footer(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(text) = recents_line(state) else {
        return;
    };
    let line = Line::from(Span::styled(text, Style::default().fg(theme.dim)));
    frame.render_widget(Paragraph::new(line), area);
}

fn render_quick_add(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    use crate::quick_add::{QuickAddKind, parse};

    let (default_wallet_name, default_flow_name) = default_wallet_flow_names(state);
    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur);

    let input = state.transactions.quick_input.as_str();

    // Try to parse the input for live preview
    let parsed = if !input.trim().is_empty() {
        parse(input, currency).ok()
    } else {
        None
    };

    let border_style = if state.transactions.quick_active {
        Style::default().fg(theme.accent)
    } else {
        Style::default().fg(theme.border)
    };

    let placeholder = "Press [a] to add transaction...";
    let (input_text, input_style) = if input.is_empty() {
        (placeholder, Style::default().fg(theme.text_muted))
    } else {
        (input, Style::default().fg(theme.text))
    };

    let cursor = if state.transactions.quick_active {
        "_"
    } else {
        ""
    };

    // First line: input field
    let mut lines = vec![Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.accent)),
        Span::styled(input_text.to_string(), input_style),
        Span::styled(cursor, Style::default().fg(theme.accent)),
    ])];

    // Show preview or error or help
    if let Some(err) = &state.transactions.quick_error {
        lines.push(Line::from(Span::styled(
            format!("⚠ {err}"),
            Style::default().fg(theme.negative),
        )));
    } else if let Some(p) = &parsed {
        // Show live preview
        let (type_icon, type_color) = match p.kind {
            QuickAddKind::Income => ("▲", theme.positive),
            QuickAddKind::Expense => ("▼", theme.negative),
            QuickAddKind::Refund => ("↩", theme.warning),
            QuickAddKind::TransferWallet | QuickAddKind::TransferFlow => ("⇄", theme.transfer),
        };
        let amount_str = Money::new(p.amount_minor).format(currency);
        let note = p.note.as_deref().unwrap_or("-");

        // Check for ambiguous matches
        let category_matches = p
            .category
            .as_ref()
            .map(|c| resolve_category_matches(state, c))
            .unwrap_or_default();
        let wallet_matches = p
            .wallet
            .as_ref()
            .map(|w| resolve_wallet_matches(state, w))
            .unwrap_or_default();
        let flow_matches = p
            .flow
            .as_ref()
            .map(|f| resolve_flow_matches(state, f))
            .unwrap_or_default();

        // Determine display values considering ambiguous state
        let (category_display, category_style, category_ambiguous) = resolve_ambiguous_display(
            state,
            &p.category,
            &category_matches,
            QuickAddAmbiguousKind::Category,
            "#",
            theme,
        );

        let (wallet_display, wallet_style, wallet_ambiguous) = if p.wallet.is_some() {
            resolve_ambiguous_display(
                state,
                &p.wallet,
                &wallet_matches,
                QuickAddAmbiguousKind::Wallet,
                "@",
                theme,
            )
        } else {
            (
                format!("@{default_wallet_name}"),
                Style::default().fg(theme.text_muted),
                false,
            )
        };

        let (flow_display, flow_style, flow_ambiguous) = if p.flow.is_some() {
            resolve_ambiguous_display(
                state,
                &p.flow,
                &flow_matches,
                QuickAddAmbiguousKind::Flow,
                ">",
                theme,
            )
        } else {
            (
                format!(">{default_flow_name}"),
                Style::default().fg(theme.text_muted),
                false,
            )
        };

        // Build preview line based on transaction type
        if p.kind == QuickAddKind::TransferWallet {
            let from = p.from_wallet.as_deref().unwrap_or("-");
            let to = p.to_wallet.as_deref().unwrap_or("-");
            lines.push(Line::from(vec![
                Span::styled(type_icon, Style::default().fg(type_color)),
                Span::raw(" "),
                Span::styled(amount_str, Style::default().fg(type_color)),
                Span::raw("  "),
                Span::styled(note, Style::default().fg(theme.text)),
                Span::raw("  │  "),
                Span::styled(format!("@{from} → @{to}"), Style::default().fg(theme.transfer)),
                Span::raw("  │  "),
                Span::styled("Today", Style::default().fg(theme.text_muted)),
            ]));
        } else if p.kind == QuickAddKind::TransferFlow {
            let from = p.from_flow.as_deref().unwrap_or("-");
            let to = p.to_flow.as_deref().unwrap_or("-");
            lines.push(Line::from(vec![
                Span::styled(type_icon, Style::default().fg(type_color)),
                Span::raw(" "),
                Span::styled(amount_str, Style::default().fg(type_color)),
                Span::raw("  "),
                Span::styled(note, Style::default().fg(theme.text)),
                Span::raw("  │  "),
                Span::styled(format!(">{from} → >{to}"), Style::default().fg(theme.transfer)),
                Span::raw("  │  "),
                Span::styled("Today", Style::default().fg(theme.text_muted)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(type_icon, Style::default().fg(type_color)),
                Span::raw(" "),
                Span::styled(amount_str, Style::default().fg(type_color)),
                Span::raw("  "),
                Span::styled(note, Style::default().fg(theme.text)),
                Span::raw("  │  "),
                Span::styled(category_display, category_style),
                Span::raw("  │  "),
                Span::styled(flow_display, flow_style),
                Span::raw("  │  "),
                Span::styled(wallet_display, wallet_style),
                Span::raw("  │  "),
                Span::styled("Today", Style::default().fg(theme.text_muted)),
            ]));
        }

        // Show ambiguous options if any
        let has_ambiguous = category_ambiguous || wallet_ambiguous || flow_ambiguous;
        if state.transactions.quick_active && has_ambiguous {
            if let Some(amb) = &state.transactions.quick_ambiguous {
                let options_str = amb
                    .options
                    .iter()
                    .enumerate()
                    .map(|(i, (_, name))| {
                        if i == amb.selected {
                            format!("[{name}]")
                        } else {
                            name.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" | ");
                let kind_prefix = match amb.kind {
                    QuickAddAmbiguousKind::Category => "#",
                    QuickAddAmbiguousKind::Wallet => "@",
                    QuickAddAmbiguousKind::Flow => ">",
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{kind_prefix}? "),
                        Style::default().fg(theme.warning),
                    ),
                    Span::styled(options_str, Style::default().fg(theme.text_muted)),
                    Span::raw("  "),
                    Span::styled("[Ctrl+R]", Style::default().fg(theme.accent)),
                    Span::styled(" cycle", Style::default().fg(theme.text_muted)),
                ]));
            } else {
                // Build ambiguous hint for fields with multiple matches
                let mut hints = Vec::new();
                if category_matches.len() > 1 {
                    let names: Vec<&str> = category_matches
                        .iter()
                        .take(3)
                        .map(|(_id, name)| name.as_str())
                        .collect();
                    hints.push(format!("#? {}", names.join(" | ")));
                }
                if wallet_matches.len() > 1 {
                    let names: Vec<&str> = wallet_matches
                        .iter()
                        .take(3)
                        .map(|(_id, name)| name.as_str())
                        .collect();
                    hints.push(format!("@? {}", names.join(" | ")));
                }
                if flow_matches.len() > 1 {
                    let names: Vec<&str> = flow_matches
                        .iter()
                        .take(3)
                        .map(|(_id, name)| name.as_str())
                        .collect();
                    hints.push(format!(">? {}", names.join(" | ")));
                }
                if !hints.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled(hints.join("  "), Style::default().fg(theme.warning)),
                        Span::raw("  "),
                        Span::styled("[Ctrl+R]", Style::default().fg(theme.accent)),
                        Span::styled(" cycle", Style::default().fg(theme.text_muted)),
                    ]));
                }
            }
        } else if state.transactions.quick_active
            && let Some(flow_query) = p.flow.as_deref()
            && flow_matches
                .first()
                .is_none_or(|(_id, name)| name.to_lowercase() != flow_query.to_lowercase())
        {
            // Show envelope suggestions if not exact match
            let suggestions = flow_name_suggestions(state, flow_query, 3);
            if !suggestions.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("Envelope suggestions: {}", suggestions.join(", ")),
                    Style::default().fg(theme.text_muted),
                )));
            }
        }
    } else if state.transactions.quick_active {
        lines.push(Line::from(Span::styled(
            "Syntax: [+]amount note [#cat] [@wallet] [>envelope]  |  + income, r refund",
            Style::default().fg(theme.text_muted),
        )));
    } else {
        // Collapsed state - show syntax and shortcuts
        lines.push(Line::from(vec![
            Span::styled("⚡ ", Style::default().fg(theme.warning)),
            Span::styled("Syntax: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                "[+]amount note [#category] [@wallet] [>envelope]",
                Style::default().fg(theme.dim),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("   Examples: ", Style::default().fg(theme.text_muted)),
            Span::styled("50 lunch #food @main", Style::default().fg(theme.dim)),
            Span::styled("  |  ", Style::default().fg(theme.border)),
            Span::styled("+100 salary >income", Style::default().fg(theme.dim)),
            Span::styled("  |  ", Style::default().fg(theme.border)),
            Span::styled("r30 refund", Style::default().fg(theme.dim)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("   [a]", Style::default().fg(theme.accent)),
            Span::styled(" quick add  ", Style::default().fg(theme.text_muted)),
            Span::styled("[i]", Style::default().fg(theme.accent)),
            Span::styled(" income  ", Style::default().fg(theme.text_muted)),
            Span::styled("[e]", Style::default().fg(theme.accent)),
            Span::styled(" expense  ", Style::default().fg(theme.text_muted)),
            Span::styled("[t]", Style::default().fg(theme.accent)),
            Span::styled(" transfer  ", Style::default().fg(theme.text_muted)),
            Span::styled("[?]", Style::default().fg(theme.accent)),
            Span::styled(" help", Style::default().fg(theme.text_muted)),
        ]));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Span::styled(
            " Quick Add ",
            Style::default().fg(theme.accent),
        ));
    let widget = Paragraph::new(lines).block(block);
    frame.render_widget(widget, area);
}

/// Helper to resolve display value for potentially ambiguous fields.
/// Returns (display_string, style, is_ambiguous)
fn resolve_ambiguous_display(
    state: &AppState,
    query: &Option<String>,
    matches: &[(Uuid, String)],
    kind: QuickAddAmbiguousKind,
    prefix: &str,
    theme: &Theme,
) -> (String, Style, bool) {
    let Some(query_str) = query else {
        return (
            "-".to_string(),
            Style::default().fg(theme.text_muted),
            false,
        );
    };

    if matches.is_empty() {
        // No matches - show warning
        return (
            format!("?{prefix}{query_str}"),
            Style::default().fg(theme.warning),
            false,
        );
    }

    if matches.len() == 1 {
        // Single match - resolved
        return (
            format!("{prefix}{}", matches[0].1),
            Style::default().fg(theme.accent),
            false,
        );
    }

    // Multiple matches - ambiguous
    // Check if we have a selection in quick_ambiguous
    if let Some(amb) = &state.transactions.quick_ambiguous
        && amb.kind == kind
        && let Some((_, name)) = amb.current()
    {
        return (
            format!("{prefix}{name}"),
            Style::default().fg(theme.warning),
            true,
        );
    }

    // No selection yet - show first match with warning style
    (
        format!("{prefix}{}", matches[0].1),
        Style::default().fg(theme.warning),
        true,
    )
}

fn recents_line(state: &AppState) -> Option<String> {
    let mut parts = Vec::new();
    let categories = state
        .transactions
        .recent_categories
        .iter()
        .take(3)
        .map(|cat| format!("#{cat}"))
        .collect::<Vec<_>>();
    if !categories.is_empty() {
        parts.push(format!("Categorie: {}", categories.join(" ")));
    }

    let wallets = recent_wallet_names(state)
        .into_iter()
        .take(3)
        .collect::<Vec<_>>();
    if !wallets.is_empty() {
        parts.push(format!("Wallet: {}", wallets.join(", ")));
    }

    let flows = recent_flow_names(state)
        .into_iter()
        .take(3)
        .collect::<Vec<_>>();
    if !flows.is_empty() {
        parts.push(format!("Flow: {}", flows.join(", ")));
    }

    if parts.is_empty() {
        None
    } else {
        Some(format!("Recenti: {}", parts.join(" • ")))
    }
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(detail) = &state.transactions.detail else {
        let block = Block::default()
            .title("Transaction")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.accent));
        frame.render_widget(
            Paragraph::new(Line::from("Nessun dettaglio disponibile."))
                .block(block)
                .alignment(ratatui::layout::Alignment::Center),
            area,
        );
        return;
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(0)])
        .split(area);

    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur);

    let header = &detail.transaction;
    let occurred_at = header.occurred_at.format("%d %b %Y %H:%M").to_string();
    let amount = Money::new(header.amount_minor).format(currency);
    let category = header
        .category
        .as_deref()
        .map(|c| format!("#{c}"))
        .unwrap_or_else(|| "-".to_string());
    let note = header.note.as_deref().unwrap_or("-");
    let voided = if header.voided { "YES" } else { "NO" };

    let lines = vec![
        Line::from(vec![
            Span::styled("Kind", Style::default().fg(theme.dim)),
            Span::raw(": "),
            kind_chip(header.kind, theme),
            Span::raw("   "),
            Span::styled("Voided", Style::default().fg(theme.dim)),
            Span::raw(": "),
            Span::styled(
                voided.to_string(),
                Style::default().fg(if header.voided {
                    theme.error
                } else {
                    theme.text
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("When", Style::default().fg(theme.dim)),
            Span::raw(format!(": {occurred_at}")),
        ]),
        Line::from(vec![
            Span::styled("Amount", Style::default().fg(theme.dim)),
            Span::raw(format!(": {amount}")),
        ]),
        Line::from(vec![
            Span::styled("Category", Style::default().fg(theme.dim)),
            Span::raw(format!(": {category}")),
        ]),
        Line::from(vec![
            Span::styled("Note", Style::default().fg(theme.dim)),
            Span::raw(format!(": {note}")),
        ]),
    ];

    let header_block = Block::default()
        .title("Transaction Detail")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(header_block), layout[0]);

    let legs = detail
        .legs
        .iter()
        .map(|leg| {
            let name = match leg.target {
                LegTarget::Wallet { wallet_id } => resolve_wallet_name(state, wallet_id),
                LegTarget::Flow { flow_id } => resolve_flow_name(state, flow_id),
            };
            let label = match leg.target {
                LegTarget::Wallet { .. } => "Wallet",
                LegTarget::Flow { .. } => "Flow",
            };
            let amount = leg_amount_span(leg.amount_minor, currency, theme);
            ListItem::new(Line::from(vec![
                Span::styled(format!("{label:<6}"), Style::default().fg(theme.dim)),
                Span::raw(": "),
                Span::raw(name),
                Span::raw("  "),
                amount,
            ]))
        })
        .collect::<Vec<_>>();

    let legs_block = Block::default()
        .title("Legs")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    let list = List::new(legs).block(legs_block);
    frame.render_widget(list, layout[1]);
}

fn kind_chip(kind: TransactionKind, theme: &Theme) -> Span<'static> {
    let (icon, color) = match kind {
        TransactionKind::Income => ("▲", theme.income),
        TransactionKind::Expense => ("▼", theme.expense),
        TransactionKind::Refund => ("↩", theme.refund),
        TransactionKind::TransferWallet | TransactionKind::TransferFlow => ("⇄", theme.transfer),
    };
    Span::styled(icon.to_string(), Style::default().fg(color))
}

fn void_chip(voided: bool, theme: &Theme) -> Option<Span<'static>> {
    if voided {
        Some(Span::styled(
            "[VOID]",
            Style::default()
                .fg(theme.error)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        None
    }
}

fn amount_span(
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
        theme.dim
    };
    let amount = Money::new(signed).format(currency);
    Span::styled(format!("{amount:<14}"), Style::default().fg(color))
}

fn leg_amount_span(amount_minor: i64, currency: Currency, theme: &Theme) -> Span<'static> {
    let color = if amount_minor < 0 {
        theme.negative
    } else if amount_minor > 0 {
        theme.positive
    } else {
        theme.dim
    };
    let amount = Money::new(amount_minor).format(currency);
    Span::styled(amount, Style::default().fg(color))
}

fn map_currency(currency: &api_types::Currency) -> Currency {
    match currency {
        api_types::Currency::Eur => Currency::Eur,
    }
}

fn resolve_wallet_name(state: &AppState, wallet_id: Uuid) -> String {
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

fn resolve_flow_name(state: &AppState, flow_id: Uuid) -> String {
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

fn recent_wallet_names(state: &AppState) -> Vec<String> {
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

fn recent_flow_names(state: &AppState) -> Vec<String> {
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

fn default_wallet_flow_names(state: &AppState) -> (String, String) {
    let snapshot = match state.snapshot.as_ref() {
        Some(snapshot) => snapshot,
        None => return ("-".to_string(), "-".to_string()),
    };

    let wallet_name = state
        .transactions
        .scope_wallet_id
        .and_then(|wallet_id| {
            snapshot
                .wallets
                .iter()
                .find(|wallet| wallet.id == wallet_id && !wallet.archived)
                .map(|wallet| wallet.name.clone())
        })
        .or_else(|| {
            state.default_wallet_id.and_then(|wallet_id| {
                snapshot
                    .wallets
                    .iter()
                    .find(|wallet| wallet.id == wallet_id && !wallet.archived)
                    .map(|wallet| wallet.name.clone())
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
                        .map(|wallet| wallet.name.clone())
                })
        })
        .or_else(|| {
            snapshot
                .wallets
                .iter()
                .find(|wallet| !wallet.archived)
                .map(|wallet| wallet.name.clone())
        })
        .unwrap_or_else(|| "-".to_string());

    let flow_name = state
        .transactions
        .scope_flow_id
        .and_then(|flow_id| {
            snapshot
                .flows
                .iter()
                .find(|flow| flow.id == flow_id && !flow.archived)
                .map(|flow| flow.name.clone())
        })
        .or_else(|| {
            state.default_flow_id.and_then(|flow_id| {
                snapshot
                    .flows
                    .iter()
                    .find(|flow| flow.id == flow_id && !flow.archived)
                    .map(|flow| flow.name.clone())
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
                        .map(|flow| flow.name.clone())
                })
        })
        .or_else(|| {
            state.last_flow_id.and_then(|flow_id| {
                snapshot
                    .flows
                    .iter()
                    .find(|flow| flow.id == flow_id && !flow.archived)
                    .map(|flow| flow.name.clone())
            })
        })
        .or_else(|| {
            snapshot
                .flows
                .iter()
                .find(|flow| flow.is_unallocated)
                .map(|flow| flow.name.clone())
        })
        .unwrap_or_else(|| "Non in flow".to_string());

    (wallet_name, flow_name)
}

fn scope_label(state: &AppState) -> String {
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

    "All".to_string()
}

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

fn signed_amount_minor(kind: TransactionKind, amount_minor: i64) -> i64 {
    if kind == TransactionKind::Expense {
        -amount_minor.abs()
    } else {
        amount_minor
    }
}

fn grouping_key_label(
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
                .unwrap_or("Uncategorized")
                .to_string();
            (label.clone(), label)
        }
        GroupingMode::Wallet => {
            if let Some(id) = tx.wallet_id {
                (format!("wallet:{id}"), resolve_wallet_name(state, id))
            } else {
                ("wallet:none".to_string(), "No wallet".to_string())
            }
        }
        GroupingMode::Envelope => {
            if let Some(id) = tx.flow_id {
                (format!("flow:{id}"), resolve_flow_name(state, id))
            } else {
                ("flow:none".to_string(), "No envelope".to_string())
            }
        }
    }
}

fn group_header_item(
    mode: GroupingMode,
    label: &str,
    total_minor: i64,
    currency: Currency,
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

fn group_total_span(total_minor: i64, currency: Currency, theme: &Theme) -> Span<'static> {
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

fn format_date_label(
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
