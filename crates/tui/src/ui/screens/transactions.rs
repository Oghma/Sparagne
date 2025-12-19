use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use api_types::transaction::TransactionKind;
use engine::{Currency, Money};

use crate::{app::AppState, ui::theme::Theme};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    render_header(frame, layout[0], state);
    render_list(frame, layout[1], state, &theme);
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

            let text = format!("{date}  {kind:<14} {amount:<14} {category}{note}");
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
