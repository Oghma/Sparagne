use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use api_types::transaction::TransactionKind;
use engine::{Currency, Money};

use crate::{
    app::{AppState, WalletFormField, WalletsMode},
    ui::theme::Theme,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    render_header(frame, layout[0], state, &theme);

    match state.wallets.mode {
        WalletsMode::Detail => render_detail(frame, layout[1], state, &theme),
        WalletsMode::Create | WalletsMode::Rename | WalletsMode::List => {
            render_list(frame, layout[1], state, &theme)
        }
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let mode = match state.wallets.mode {
        WalletsMode::List => "List",
        WalletsMode::Detail => "Detail",
        WalletsMode::Create => "Create",
        WalletsMode::Rename => "Rename",
    };
    let mut line = vec![
        Span::styled("Mode", Style::default().fg(theme.dim)),
        Span::raw(format!(": {mode}")),
    ];
    if let Some(err) = state.wallets.error.as_ref() {
        line.push(Span::raw("   "));
        line.push(Span::styled(err.as_str(), Style::default().fg(theme.error)));
    }

    let block = Block::default().borders(Borders::ALL).title("Wallets");
    frame.render_widget(Paragraph::new(Line::from(line)).block(block), area);
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let show_form = matches!(state.wallets.mode, WalletsMode::Create | WalletsMode::Rename);
    let (form_area, list_area) = if show_form {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(6), Constraint::Min(0)])
            .split(area);
        (Some(layout[0]), layout[1])
    } else {
        (None, area)
    };

    if let Some(form_area) = form_area {
        render_form(frame, form_area, state, theme);
    }
    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur);

    let items = state
        .snapshot
        .as_ref()
        .map(|snap| {
            snap.wallets
                .iter()
                .map(|wallet| {
                    let balance = Money::new(wallet.balance_minor).format(currency);
                    let archived = if wallet.archived { " archived" } else { "" };
                    let text = format!("{}  {balance}{archived}", wallet.name);
                    ListItem::new(Line::from(text))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(Vec::new);

    let mut list_state = ListState::default();
    if !items.is_empty() {
        list_state.select(Some(state.wallets.selected));
    }

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("» ");
    frame.render_stateful_widget(list, list_area, &mut list_state);
}

fn render_form(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let form = &state.wallets.form;
    let is_rename = state.wallets.mode == WalletsMode::Rename;

    let mut lines = Vec::new();
    lines.push(render_field(
        "Name",
        form.name.as_str(),
        form.focus == WalletFormField::Name,
        theme,
    ));
    if !is_rename {
        lines.push(render_field(
            "Opening",
            form.opening.as_str(),
            form.focus == WalletFormField::Opening,
            theme,
        ));
    }

    lines.push(Line::from(Span::styled(
        if is_rename {
            "Enter: rename • Tab: next • Esc: cancel"
        } else {
            "Enter: create • Tab: next • Esc: cancel"
        },
        Style::default().fg(theme.dim),
    )));

    if let Some(err) = form.error.as_ref() {
        lines.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(theme.error),
        )));
    }

    let block = Block::default()
        .title(if is_rename { "Rename Wallet" } else { "New Wallet" })
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(snapshot) = state.snapshot.as_ref() else {
        render_empty(frame, area, theme, "Snapshot non disponibile.");
        return;
    };
    let Some(detail_id) = state.wallets.detail.wallet_id else {
        render_empty(frame, area, theme, "Nessun wallet selezionato.");
        return;
    };
    let Some(wallet) = snapshot.wallets.iter().find(|wallet| wallet.id == detail_id) else {
        render_empty(frame, area, theme, "Wallet non trovato.");
        return;
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(0)])
        .split(area);

    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur);

    let balance = Money::new(wallet.balance_minor).format(currency);
    let archived = if wallet.archived { "YES" } else { "NO" };

    let header_lines = vec![
        Line::from(vec![
            Span::styled("Wallet", Style::default().fg(theme.dim)),
            Span::raw(format!(": {}", wallet.name)),
        ]),
        Line::from(vec![
            Span::styled("Balance", Style::default().fg(theme.dim)),
            Span::raw(format!(": {balance}")),
        ]),
        Line::from(vec![
            Span::styled("Archived", Style::default().fg(theme.dim)),
            Span::raw(format!(": {archived}")),
        ]),
    ];
    let header_block = Block::default()
        .title("Wallet Detail")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(header_lines).block(header_block), layout[0]);

    if let Some(err) = state.wallets.detail.error.as_ref() {
        let block = Block::default()
            .title("Recent Transactions")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.error));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                err.as_str(),
                Style::default().fg(theme.error),
            )))
            .alignment(Alignment::Center)
            .block(block),
            layout[1],
        );
        return;
    }

    let items = state
        .wallets
        .detail
        .transactions
        .iter()
        .map(|tx| {
            let when = tx.occurred_at.format("%d %b %H:%M").to_string();
            let amount = Money::new(tx.amount_minor).format(currency);
            let note = tx.note.as_deref().unwrap_or("");
            let kind = kind_label(tx.kind);
            let text = format!("{when}  {kind:<12} {amount:<12} {note}");
            ListItem::new(Line::from(text))
        })
        .collect::<Vec<_>>();

    let list = List::new(items).block(
        Block::default()
            .title("Recent Transactions")
            .borders(Borders::ALL),
    );
    frame.render_widget(list, layout[1]);
}

fn render_field(label: &str, value: &str, focused: bool, theme: &Theme) -> Line<'static> {
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
        Span::styled(format!("{label:<10}"), label_style),
        Span::raw(" "),
        Span::styled(value.to_string(), value_style),
    ])
}

fn render_empty(frame: &mut Frame<'_>, area: Rect, theme: &Theme, message: &str) {
    let block = Block::default()
        .title("Wallet Detail")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(
        Paragraph::new(Line::from(message))
            .alignment(Alignment::Center)
            .block(block),
        area,
    );
}

fn kind_label(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Income => "Income",
        TransactionKind::Expense => "Expense",
        TransactionKind::Refund => "Refund",
        TransactionKind::TransferWallet => "Transfer W",
        TransactionKind::TransferFlow => "Transfer F",
    }
}

fn map_currency(currency: &api_types::Currency) -> Currency {
    match currency {
        api_types::Currency::Eur => Currency::Eur,
    }
}
