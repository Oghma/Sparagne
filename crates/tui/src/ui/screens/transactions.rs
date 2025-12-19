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
    ui::theme::Theme,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    render_header(frame, layout[0], state);
    match state.transactions.mode {
        TransactionsMode::List => render_list(frame, layout[1], state, &theme),
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

    let mut line = vec![
        Span::styled("Scope", Style::default().fg(theme.dim)),
        Span::raw(": All   "),
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

    frame.render_stateful_widget(list, area, &mut list_state);
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
