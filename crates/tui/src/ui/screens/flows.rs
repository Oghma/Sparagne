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
    app::{AppState, FlowFormField, FlowModeChoice, FlowsMode},
    ui::theme::Theme,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    render_header(frame, layout[0], state, &theme);

    match state.flows.mode {
        FlowsMode::Detail => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(layout[1]);
            render_list(frame, columns[0], state, &theme);
            render_detail(frame, columns[1], state, &theme);
        }
        FlowsMode::Create | FlowsMode::Rename | FlowsMode::List => {
            render_list(frame, layout[1], state, &theme)
        }
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let mode = match state.flows.mode {
        FlowsMode::List => "List",
        FlowsMode::Detail => "Detail",
        FlowsMode::Create => "Create",
        FlowsMode::Rename => "Rename",
    };
    let mut line = vec![
        Span::styled("Mode", Style::default().fg(theme.dim)),
        Span::raw(format!(": {mode}")),
    ];
    if let Some(err) = state.flows.error.as_ref() {
        line.push(Span::raw("   "));
        line.push(Span::styled(err.as_str(), Style::default().fg(theme.error)));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title("Flows");
    frame.render_widget(Paragraph::new(Line::from(line)).block(block), area);
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let show_form = matches!(state.flows.mode, FlowsMode::Create | FlowsMode::Rename);
    let (form_area, list_area) = if show_form {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(7), Constraint::Min(0)])
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
            snap.flows
                .iter()
                .map(|flow| {
                    let balance = Money::new(flow.balance_minor).format(currency);
                    let archived = if flow.archived { " archived" } else { "" };
                    let marker = if flow.is_unallocated { " [Unallocated]" } else { "" };
                    let text = format!("{}{}  {balance}{archived}", flow.name, marker);
                    ListItem::new(Line::from(text))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(Vec::new);

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    if items.is_empty() {
        let empty_msg = Paragraph::new(Line::from(vec![
            Span::raw("No flows. Press "),
            Span::styled("c", Style::default().fg(theme.accent)),
            Span::raw(" to create one."),
        ]))
        .alignment(Alignment::Center)
        .block(list_block);
        frame.render_widget(empty_msg, list_area);
        return;
    }

    let mut list_state = ListState::default();
    list_state.select(Some(state.flows.selected));

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
    let form = &state.flows.form;
    let is_rename = state.flows.mode == FlowsMode::Rename;

    let mut lines = Vec::new();
    lines.push(render_field(
        "Name",
        form.name.as_str(),
        form.focus == FlowFormField::Name,
        theme,
    ));
    if !is_rename {
        lines.push(render_field(
            "Mode",
            form.mode.label(),
            form.focus == FlowFormField::Mode,
            theme,
        ));
        lines.push(render_field(
            "Cap",
            if matches!(form.mode, FlowModeChoice::Unlimited) {
                "-"
            } else {
                form.cap.as_str()
            },
            form.focus == FlowFormField::Cap,
            theme,
        ));
        lines.push(render_field(
            "Opening",
            form.opening.as_str(),
            form.focus == FlowFormField::Opening,
            theme,
        ));
    }

    lines.push(Line::from(Span::styled(
        if is_rename {
            "Enter: rename • Tab: next • Esc: cancel"
        } else {
            "Enter: create • Tab: next • M: mode • Esc: cancel"
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
        .title(if is_rename { "Rename Flow" } else { "New Flow" })
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
    let Some(detail_id) = state.flows.detail.flow_id else {
        render_empty(frame, area, theme, "Nessun flow selezionato.");
        return;
    };
    let Some(flow) = snapshot.flows.iter().find(|flow| flow.id == detail_id) else {
        render_empty(frame, area, theme, "Flow non trovato.");
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

    let balance = Money::new(flow.balance_minor).format(currency);
    let archived = if flow.archived { "YES" } else { "NO" };
    let unallocated = if flow.is_unallocated { "YES" } else { "NO" };

    let mut header_lines = vec![
        Line::from(vec![
            Span::styled("Flow", Style::default().fg(theme.dim)),
            Span::raw(format!(": {}", flow.name)),
        ]),
        Line::from(vec![
            Span::styled("Balance", Style::default().fg(theme.dim)),
            Span::raw(format!(": {balance}")),
        ]),
        Line::from(vec![
            Span::styled("Archived", Style::default().fg(theme.dim)),
            Span::raw(format!(": {archived}")),
            Span::raw("   "),
            Span::styled("Unallocated", Style::default().fg(theme.dim)),
            Span::raw(format!(": {unallocated}")),
        ]),
    ];

    if let Some(detail) = state.flows.detail.detail.as_ref() {
        if let Some(line) = cap_progress_line(detail, currency, theme) {
            header_lines.push(line);
        }
    }
    let header_block = Block::default()
        .title("Flow Detail")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(header_lines).block(header_block), layout[0]);

    if let Some(err) = state.flows.detail.error.as_ref() {
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
        .flows
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
        .title("Flow Detail")
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

fn kind_label(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Income => "▲ Income",
        TransactionKind::Expense => "▼ Expense",
        TransactionKind::Refund => "↩ Refund",
        TransactionKind::TransferWallet => "⇄ Transfer",
        TransactionKind::TransferFlow => "⇄ Transfer",
    }
}

fn map_currency(currency: &api_types::Currency) -> Currency {
    match currency {
        api_types::Currency::Eur => Currency::Eur,
    }
}

fn cap_progress_line(
    detail: &engine::CashFlow,
    currency: Currency,
    theme: &Theme,
) -> Option<Line<'static>> {
    let cap = detail.max_balance?;
    if cap <= 0 {
        return None;
    }

    let (label, current) = if let Some(income_total_minor) = detail.income_balance {
        ("Income cap", income_total_minor)
    } else {
        ("Net cap", detail.balance)
    };

    let current = current.max(0);
    let ratio = (current.min(cap) * 20) / cap;
    let filled = ratio as usize;
    let empty = 20usize.saturating_sub(filled);
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    let current_fmt = Money::new(current).format(currency);
    let cap_fmt = Money::new(cap).format(currency);

    Some(Line::from(vec![
        Span::styled(label, Style::default().fg(theme.dim)),
        Span::raw(format!(": {current_fmt} / {cap_fmt}  ")),
        Span::styled(bar, Style::default().fg(theme.accent)),
    ]))
}
