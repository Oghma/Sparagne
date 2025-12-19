use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use api_types::transaction::{LegTarget, TransactionKind};
use engine::{Currency, Money};
use uuid::Uuid;

use crate::{
    app::{AppState, TransactionsMode},
    ui::{components::centered_rect, theme::Theme},
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    render_header(frame, layout[0], state);
    match state.transactions.mode {
        TransactionsMode::List | TransactionsMode::PickWallet | TransactionsMode::PickFlow => {
            render_list(frame, layout[1], state, &theme);
            if matches!(
                state.transactions.mode,
                TransactionsMode::PickWallet | TransactionsMode::PickFlow
            ) {
                render_scope_picker(frame, layout[1], state, &theme);
            }
        }
        TransactionsMode::Detail | TransactionsMode::Edit => {
            render_detail(frame, layout[1], state, &theme)
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
    let mut line = vec![
        Span::styled("Scope", Style::default().fg(theme.dim)),
        Span::raw(format!(": {scope}   ")),
        Span::styled("Voided", Style::default().fg(theme.dim)),
        Span::raw(format!(": {include_voided}   ")),
        Span::styled("Transfers", Style::default().fg(theme.dim)),
        Span::raw(format!(": {include_transfers}")),
    ];

    if let Some(err) = &state.transactions.error {
        line.push(Span::raw("   "));
        line.push(Span::styled(err.as_str(), Style::default().fg(theme.error)));
    }

    let block = Block::default().borders(Borders::ALL).title("Transactions");
    let content = Paragraph::new(Line::from(line)).block(block);
    frame.render_widget(content, area);
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    render_quick_add(frame, layout[0], state, theme);

    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur);

    let items = state
        .transactions
        .items
        .iter()
        .map(|tx| {
            let date = tx.occurred_at.format("%d %b %H:%M").to_string();
            let kind = kind_label(tx.kind);
            let amount = Money::new(tx.amount_minor).format(currency);
            let note = tx.note.as_deref().unwrap_or("");
            let category = tx
                .category
                .as_deref()
                .map(|c| format!("#{c} "))
                .unwrap_or_default();
            let voided = if tx.voided { " void" } else { "" };

            let text = format!("{date}  {kind:<14} {amount:<14} {category}{note}{voided}");
            ListItem::new(Line::from(text))
        })
        .collect::<Vec<_>>();

    let mut list_state = ListState::default();
    if !items.is_empty() {
        list_state.select(Some(state.transactions.selected));
    }

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL))
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
                let marker = if flow.is_unallocated { " [Unallocated]" } else { "" };
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

fn render_quick_add(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let (wallet_name, flow_name) = default_wallet_flow_names(state);
    let focus = if state.transactions.quick_active {
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
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
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Quick add (a)");
    let widget = Paragraph::new(lines).block(block);
    frame.render_widget(widget, area);
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(detail) = &state.transactions.detail else {
        let block = Block::default()
            .title("Transaction")
            .borders(Borders::ALL)
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

    if state.transactions.mode == TransactionsMode::Edit {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Edit", Style::default().fg(theme.accent)),
            Span::raw(": "),
            Span::raw(state.transactions.edit_input.as_str()),
        ]));
        if let Some(err) = &state.transactions.edit_error {
            lines.push(Line::from(Span::styled(
                err.as_str(),
                Style::default().fg(theme.error),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "Formato: importo [nota]",
                Style::default().fg(theme.dim),
            )));
        }
    }

    let header_block = Block::default()
        .title("Transaction Detail")
        .borders(Borders::ALL)
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
        .border_style(Style::default().fg(theme.accent));
    let list = List::new(legs).block(legs_block);
    frame.render_widget(list, layout[1]);
}

fn kind_label(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Income => "Income",
        TransactionKind::Expense => "Expense",
        TransactionKind::Refund => "Refund",
        TransactionKind::TransferWallet => "Transfer Wallet",
        TransactionKind::TransferFlow => "Transfer Flow",
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
