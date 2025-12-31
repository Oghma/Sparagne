use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use api_types::transaction::TransactionKind;
use engine::{Currency, Money};

use crate::{
    app::{AppState, WalletFormField, WalletsMode, wallets_visible_indices},
    ui::theme::Theme,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    render_header(frame, layout[0], state, &theme);

    match state.wallets.mode {
        WalletsMode::Detail => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(layout[1]);
            render_list(frame, columns[0], state, &theme);
            render_detail(frame, columns[1], state, &theme);
        }
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
    let search_query = state.wallets.search_query.trim();
    if !search_query.is_empty() || state.wallets.search_active {
        line.push(Span::raw("   "));
        line.push(Span::styled("Search", Style::default().fg(theme.dim)));
        line.push(Span::raw(": "));
        let shown = if search_query.is_empty() {
            "…"
        } else {
            search_query
        };
        let mut style = Style::default().fg(theme.text);
        if state.wallets.search_active {
            style = style.fg(theme.accent).add_modifier(Modifier::BOLD);
        }
        line.push(Span::styled(shown.to_string(), style));
    }
    line.push(Span::raw("   "));
    line.push(Span::styled(
        "Ctrl+F: search",
        Style::default().fg(theme.dim),
    ));
    if let Some(err) = state.wallets.error.as_ref() {
        line.push(Span::raw("   "));
        line.push(Span::styled(err.as_str(), Style::default().fg(theme.error)));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title("Wallets");
    frame.render_widget(Paragraph::new(Line::from(line)).block(block), area);
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let show_form = matches!(
        state.wallets.mode,
        WalletsMode::Create | WalletsMode::Rename
    );
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
    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let Some(snapshot) = state.snapshot.as_ref() else {
        let empty_msg = Paragraph::new(Line::from("Snapshot non disponibile."))
            .alignment(Alignment::Center)
            .block(list_block);
        frame.render_widget(empty_msg, list_area);
        return;
    };

    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur);

    let visible = wallets_visible_indices(state);
    let items = visible
        .iter()
        .filter_map(|idx| snapshot.wallets.get(*idx))
        .map(|wallet| {
            let name_style = if wallet.archived {
                Style::default().fg(theme.dim)
            } else {
                Style::default().fg(theme.text)
            };
            let mut spans = vec![
                Span::styled(wallet.name.clone(), name_style),
                Span::raw("  "),
                balance_span(wallet.balance_minor, currency, theme),
            ];
            if wallet.archived {
                spans.push(Span::raw(" "));
                spans.push(status_chip("ARCHIVED", theme.warning));
            }
            ListItem::new(Line::from(spans))
        })
        .collect::<Vec<_>>();

    if items.is_empty() {
        let query = state.wallets.search_query.trim();
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
        } else if snapshot.wallets.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("No wallets. Press "),
                Span::styled("c", Style::default().fg(theme.accent)),
                Span::raw(" to create one."),
            ]));
        }
        let empty_msg = Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(list_block);
        frame.render_widget(empty_msg, list_area);
        return;
    }

    let mut list_state = ListState::default();
    list_state.select(Some(state.wallets.selected));

    let list = List::new(items)
        .block(list_block)
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
        .title(if is_rename {
            "Rename Wallet"
        } else {
            "New Wallet"
        })
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
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
    let Some(wallet) = snapshot
        .wallets
        .iter()
        .find(|wallet| wallet.id == detail_id)
    else {
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

    let header_lines = vec![
        Line::from(vec![
            Span::styled("Wallet", Style::default().fg(theme.dim)),
            Span::raw(format!(": {}", wallet.name)),
        ]),
        Line::from(vec![
            Span::styled("Balance", Style::default().fg(theme.dim)),
            Span::raw(": "),
            balance_span(wallet.balance_minor, currency, theme),
        ]),
        Line::from(vec![
            Span::styled("Status", Style::default().fg(theme.dim)),
            Span::raw(": "),
            if wallet.archived {
                status_chip("ARCHIVED", theme.warning)
            } else {
                status_chip("ACTIVE", theme.text_muted)
            },
        ]),
    ];
    let header_block = Block::default()
        .title("Wallet Detail")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(header_lines).block(header_block), layout[0]);

    if let Some(err) = state.wallets.detail.error.as_ref() {
        let block = Block::default()
            .title("Recent Transactions")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
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
            let note = tx.note.as_deref().unwrap_or("");
            let line = Line::from(vec![
                Span::styled(when, Style::default().fg(theme.dim)),
                Span::raw(" "),
                kind_chip(tx.kind, theme),
                Span::raw(" "),
                signed_amount_span(tx.amount_minor, currency, theme),
                Span::raw(" "),
                Span::raw(note),
            ]);
            ListItem::new(line)
        })
        .collect::<Vec<_>>();

    let list = List::new(items).block(
        Block::default()
            .title("Recent Transactions")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border)),
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
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(
        Paragraph::new(Line::from(message))
            .alignment(Alignment::Center)
            .block(block),
        area,
    );
}

fn status_chip(label: &str, color: ratatui::style::Color) -> Span<'static> {
    Span::styled(
        format!("[{label}]"),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}

fn balance_span(amount_minor: i64, currency: Currency, theme: &Theme) -> Span<'static> {
    signed_amount_span(amount_minor, currency, theme)
}

fn signed_amount_span(amount_minor: i64, currency: Currency, theme: &Theme) -> Span<'static> {
    let amount = Money::new(amount_minor).format(currency);
    let color = if amount_minor < 0 {
        theme.negative
    } else if amount_minor > 0 {
        theme.positive
    } else {
        theme.dim
    };
    Span::styled(amount, Style::default().fg(color))
}

fn kind_chip(kind: TransactionKind, theme: &Theme) -> Span<'static> {
    let (label, color) = match kind {
        TransactionKind::Income => ("INC", theme.positive),
        TransactionKind::Expense => ("EXP", theme.negative),
        TransactionKind::Refund => ("REF", theme.accent),
        TransactionKind::TransferWallet | TransactionKind::TransferFlow => ("TR", theme.text),
    };
    Span::styled(
        format!("[{label}]"),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}

fn map_currency(currency: &api_types::Currency) -> Currency {
    match currency {
        api_types::Currency::Eur => Currency::Eur,
    }
}
