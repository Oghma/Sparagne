use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use api_types::transaction::{LegTarget, TransactionKind};
use engine::{Currency, Money};
use uuid::Uuid;

use crate::{
    app::{
        AppState, FilterField, TransactionFormField, TransactionsMode, TransferField,
        ordered_flow_ids_from_state, ordered_wallet_ids_from_state, transactions_visible_indices,
    },
    ui::{components::centered_rect, theme::Theme},
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    render_header(frame, layout[0], state);
    match state.transactions.mode {
        TransactionsMode::List
        | TransactionsMode::PickWallet
        | TransactionsMode::PickFlow
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
    let include_voided = if state.transactions.include_voided {
        "On"
    } else {
        "Off"
    };
    let include_transfers = if state.transactions.include_transfers {
        "On"
    } else {
        "Off"
    };

    let scope = scope_label(state);
    let filter_summary = filter_summary(state);
    let mut line = vec![
        Span::styled("Scope", Style::default().fg(theme.dim)),
        Span::raw(format!(": {scope}   ")),
        Span::styled("Voided", Style::default().fg(theme.dim)),
        Span::raw(format!(": {include_voided}   ")),
        Span::styled("Transfers", Style::default().fg(theme.dim)),
        Span::raw(format!(": {include_transfers}")),
    ];

    if let Some(summary) = filter_summary {
        line.push(Span::raw("   "));
        line.push(Span::styled(summary, Style::default().fg(theme.dim)));
    }

    let search_query = state.transactions.search_query.trim();
    if !search_query.is_empty() || state.transactions.search_active {
        line.push(Span::raw("   "));
        line.push(Span::styled("Search", Style::default().fg(theme.dim)));
        line.push(Span::raw(": "));
        let shown = if search_query.is_empty() {
            "…"
        } else {
            search_query
        };
        let mut style = Style::default().fg(theme.text);
        if state.transactions.search_active {
            style = style.fg(theme.accent).add_modifier(Modifier::BOLD);
        }
        line.push(Span::styled(shown.to_string(), style));
    }

    line.push(Span::raw("   "));
    line.push(Span::styled(
        "Ctrl+F: search",
        Style::default().fg(theme.dim),
    ));

    if let Some(err) = &state.transactions.error {
        line.push(Span::raw("   "));
        line.push(Span::styled(err.as_str(), Style::default().fg(theme.error)));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title("Transactions");
    let content = Paragraph::new(Line::from(line)).block(block);
    frame.render_widget(content, area);
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
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
    let mut last_day = None;

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
                Span::styled("a", Style::default().fg(theme.accent)),
                Span::raw(" to add one."),
            ]));
        }
        let empty_msg = Paragraph::new(lines)
            .alignment(ratatui::layout::Alignment::Center)
            .block(list_block);
        frame.render_widget(empty_msg, layout[1]);
        return;
    }
    for (visible_idx, idx) in visible.iter().enumerate() {
        let tx = &state.transactions.items[*idx];
        let day_label = tx.occurred_at.format("%Y-%m-%d").to_string();
        if last_day.as_ref() != Some(&day_label) {
            last_day = Some(day_label.clone());
            rows.push(ListItem::new(Line::from(Span::styled(
                format!("── {day_label} ──"),
                Style::default().fg(theme.dim),
            ))));
        }

        if visible_idx == state.transactions.selected {
            selected_row = Some(rows.len());
        }

        let time = tx.occurred_at.format("%H:%M").to_string();
        let kind = kind_label(tx.kind);
        let amount = Money::new(tx.amount_minor).format(currency);
        let note = tx.note.as_deref().unwrap_or("");
        let category = tx
            .category
            .as_deref()
            .map(|c| format!("#{c} "))
            .unwrap_or_default();
        let badge = badge_label(tx.kind, tx.voided);

        let text = format!("{time}  {badge:<8} {kind:<14} {amount:<14} {category}{note}");
        rows.push(ListItem::new(Line::from(text)));
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
                .border_style(Style::default().fg(theme.accent)),
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
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(0)])
        .split(popup);

    let mut lines = vec![
        render_transfer_field("From", from, transfer.focus == TransferField::From, theme),
        render_transfer_field("To", to, transfer.focus == TransferField::To, theme),
        render_transfer_field(
            "Amount",
            transfer.amount.as_str(),
            transfer.focus == TransferField::Amount,
            theme,
        ),
        render_transfer_field(
            "Note",
            transfer.note.as_str(),
            transfer.focus == TransferField::Note,
            theme,
        ),
        render_transfer_field(
            "When",
            if transfer.occurred_at.trim().is_empty() {
                "-"
            } else {
                transfer.occurred_at.as_str()
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
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), layout[0]);

    let hint_block = Block::default()
        .title("Available")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
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

    let category_raw = form.category.trim().trim_start_matches('#');
    let category = if category_raw.is_empty() {
        "-".to_string()
    } else {
        format!("#{category_raw}")
    };
    let note = if form.note.trim().is_empty() {
        "-".to_string()
    } else {
        form.note.trim().to_string()
    };
    let occurred_at = if form.occurred_at.trim().is_empty() {
        "-".to_string()
    } else {
        form.occurred_at.trim().to_string()
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
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(0)])
        .split(popup);

    let mut lines = vec![
        render_transaction_field(
            "Amount",
            form.amount.as_str(),
            form.focus == TransactionFormField::Amount,
            theme,
        ),
        render_transaction_field(
            "Wallet",
            wallet_name,
            form.focus == TransactionFormField::Wallet,
            theme,
        ),
        render_transaction_field(
            "Flow",
            flow_name,
            form.focus == TransactionFormField::Flow,
            theme,
        ),
        render_transaction_field(
            "Category",
            category.as_str(),
            form.focus == TransactionFormField::Category,
            theme,
        ),
        render_transaction_field(
            "Note",
            note.as_str(),
            form.focus == TransactionFormField::Note,
            theme,
        ),
        render_transaction_field(
            "When",
            occurred_at.as_str(),
            form.focus == TransactionFormField::OccurredAt,
            theme,
        ),
        Line::from(Span::styled(
            "Tab: next • ↑/↓: change • Enter: save • Esc: cancel",
            Style::default().fg(theme.dim),
        )),
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
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), layout[0]);

    let bottom_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(5)])
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
        .border_style(Style::default().fg(theme.border));
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
        .border_style(Style::default().fg(theme.border));

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

fn render_filter_form(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let filter = &state.transactions.filter;
    let popup = centered_rect(70, 60, area);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(0)])
        .split(popup);

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
        Line::from(vec![
            Span::styled(
                "Kinds",
                if filter.focus == FilterField::Kinds {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text)
                },
            ),
            Span::raw(": "),
            kind_chip("Income", filter.kind_income, theme),
            Span::raw(" "),
            kind_chip("Expense", filter.kind_expense, theme),
            Span::raw(" "),
            kind_chip("Refund", filter.kind_refund, theme),
            Span::raw(" "),
            kind_chip("T.Wallet", filter.kind_transfer_wallet, theme),
            Span::raw(" "),
            kind_chip("T.Flow", filter.kind_transfer_flow, theme),
        ]),
        Line::from(Span::styled(
            "Tab: next • i/e/r/w/f toggle kinds • Enter: apply • Esc: cancel",
            Style::default().fg(theme.dim),
        )),
    ];

    if let Some(err) = filter.error.as_ref() {
        lines.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(theme.error),
        )));
    }

    let block = Block::default()
        .title("Filters")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), layout[0]);
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

fn kind_chip(label: &str, enabled: bool, theme: &Theme) -> Span<'static> {
    let style = if enabled {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.dim)
    };
    Span::styled(format!("[{label}]"), style)
}

fn render_quick_add(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let (wallet_name, flow_name) = default_wallet_flow_names(state);
    let focus = if state.transactions.quick_active {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.dim)
    };

    let input = state.transactions.quick_input.as_str();
    let placeholder = "scrivi qui...";
    let (input_text, input_style) = if input.is_empty() {
        (placeholder, Style::default().fg(theme.dim))
    } else {
        (input, Style::default().fg(theme.text))
    };
    let cursor = if state.transactions.quick_active {
        "|"
    } else {
        ""
    };

    let mut lines = vec![Line::from(vec![
        Span::styled("Quick add", focus),
        Span::raw(": "),
        Span::styled(">", Style::default().fg(theme.accent)),
        Span::raw(" "),
        Span::styled("[", Style::default().fg(theme.dim)),
        Span::styled(input_text.to_string(), input_style),
        Span::styled(cursor, Style::default().fg(theme.accent)),
        Span::styled("]", Style::default().fg(theme.dim)),
        Span::raw("   "),
        Span::styled("wallet", Style::default().fg(theme.dim)),
        Span::raw(format!(": {wallet_name}   ")),
        Span::styled("flow", Style::default().fg(theme.dim)),
        Span::raw(format!(": {flow_name}")),
    ])];

    if let Some(err) = &state.transactions.quick_error {
        lines.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(theme.error),
        )));
    } else if state.transactions.quick_active {
        lines.push(Line::from(Span::styled(
            "Formato: 12.50 bar  |  +1000 stipendio  |  r 5.20 amazon  |  #tag opzionale",
            Style::default().fg(theme.dim),
        )));
        if !state.transactions.recent_categories.is_empty() {
            let recents = state
                .transactions
                .recent_categories
                .iter()
                .map(|cat| format!("#{cat}"))
                .collect::<Vec<_>>()
                .join(" ");
            lines.push(Line::from(Span::styled(
                format!("Recenti: {recents}"),
                Style::default().fg(theme.dim),
            )));
        }
        let recent_wallets = recent_wallet_names(state);
        if !recent_wallets.is_empty() {
            let list = recent_wallets.join(" • ");
            lines.push(Line::from(Span::styled(
                format!("Wallet recenti: {list}"),
                Style::default().fg(theme.dim),
            )));
        }
        let recent_flows = recent_flow_names(state);
        if !recent_flows.is_empty() {
            let list = recent_flows.join(" • ");
            lines.push(Line::from(Span::styled(
                format!("Flow recenti: {list}"),
                Style::default().fg(theme.dim),
            )));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title("Quick add (a)");
    let widget = Paragraph::new(lines).block(block);
    frame.render_widget(widget, area);
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

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Kind", Style::default().fg(theme.dim)),
            Span::raw(format!(": {}", kind_label(header.kind))),
            Span::raw("   "),
            Span::styled("Voided", Style::default().fg(theme.dim)),
            Span::raw(format!(": {voided}")),
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
            let amount = Money::new(leg.amount_minor).format(currency);
            let label = match leg.target {
                LegTarget::Wallet { .. } => "Wallet",
                LegTarget::Flow { .. } => "Flow",
            };
            ListItem::new(Line::from(format!("{label}: {name}  {amount}")))
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

fn kind_label(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Income => "▲ Income",
        TransactionKind::Expense => "▼ Expense",
        TransactionKind::Refund => "↩ Refund",
        TransactionKind::TransferWallet => "⇄ Transfer",
        TransactionKind::TransferFlow => "⇄ Transfer",
    }
}

fn badge_label(kind: TransactionKind, voided: bool) -> String {
    let mut badge = String::new();
    if matches!(
        kind,
        TransactionKind::TransferWallet | TransactionKind::TransferFlow
    ) {
        badge.push_str("[TR]");
    }
    if voided {
        badge.push_str("[VOID]");
    }
    if badge.is_empty() {
        "-".to_string()
    } else {
        badge
    }
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
    if let Some(kinds) = state.transactions.filter_kinds.as_ref() {
        if !kinds.is_empty() {
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
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("Filters: {}", parts.join(" • ")))
    }
}
