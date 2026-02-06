//! Wallet/Flow/Transfer pickers rendering.
//!
//! Displays:
//! - Wallet scope picker (for filtering transactions by wallet)
//! - Flow scope picker (for filtering transactions by flow)
//! - Transfer type picker (wallet vs flow transfer)
//! - Transfer form overlay (from/to/amount/note/when)

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{AppState, TransactionsMode, TransferField},
    ui::{common::render_label_value_field, components::centered_rect, theme::Theme},
};

/// Renders the wallet/flow scope picker overlay
pub fn render_scope_picker(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
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

/// Renders the transfer type picker (wallet vs flow transfer)
pub fn render_transfer_picker(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
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

/// Renders the transfer form overlay (wallet or flow transfer)
pub fn render_transfer_form(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
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
        render_label_value_field("From", from, transfer.focus == TransferField::From, theme),
        render_label_value_field("To", to, transfer.focus == TransferField::To, theme),
        render_label_value_field(
            "Amount",
            transfer.amount.value(),
            transfer.focus == TransferField::Amount,
            theme,
        ),
        render_label_value_field(
            "Note",
            transfer.note.value(),
            transfer.focus == TransferField::Note,
            theme,
        ),
        render_label_value_field(
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
            Style::default().fg(theme.text_muted),
        )),
    ];

    if let Some(err) = transfer.error.as_ref() {
        lines.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(theme.negative),
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
