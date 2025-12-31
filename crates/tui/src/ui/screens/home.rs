use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
};

use api_types::transaction::TransactionKind;
use engine::{Currency, Money};

use crate::{
    app::AppState,
    ui::{
        components::{
            card::{Card, StatCard},
            charts::{ascii_bar, mini_bar_chart},
            money::{inline_progress_bar, styled_amount},
        },
        theme::Theme,
    },
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();

    // Main layout: Quick stats, wallets/flows, recent transactions, quick actions
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // Quick stats
            Constraint::Length(10), // Wallets and Flows side by side
            Constraint::Min(5),     // Recent transactions
            Constraint::Length(3),  // Quick actions
        ])
        .split(area);

    render_quick_stats(frame, layout[0], state, &theme);
    render_wallets_flows(frame, layout[1], state, &theme);
    render_recent_transactions(frame, layout[2], state, &theme);
    render_quick_actions(frame, layout[3], state, &theme);
}

fn render_quick_stats(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);

    // Calculate totals
    let total_balance: i64 = state
        .snapshot
        .as_ref()
        .map(|snap| snap.wallets.iter().map(|w| w.balance_minor).sum())
        .unwrap_or(0);

    let (income, expenses) = state
        .stats
        .data
        .as_ref()
        .map(|s| (s.total_income_minor, s.total_expenses_minor))
        .unwrap_or((0, 0));

    // Split into three columns
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(area);

    // Total Balance
    let trend = mini_bar_chart(&state.stats.sparkline);
    let mut total_card = StatCard::new(
        "Total Balance",
        Money::new(total_balance).format(currency),
        theme,
    );
    if !trend.is_empty() {
        total_card = total_card.subtitle(trend);
    }
    total_card.render(frame, cols[0]);

    // This Month Income
    let income_ratio = if income > 0 {
        Some((income, income)) // Full bar for income reference
    } else {
        None
    };
    render_stat_card(
        frame,
        cols[1],
        "This Month Income",
        format!("+{}", Money::new(income).format(currency)),
        Style::default().fg(theme.positive),
        income_ratio,
        theme,
    );

    // This Month Expenses
    let expense_ratio = if income > 0 {
        Some((expenses, income)) // Expenses relative to income
    } else {
        None
    };
    render_stat_card(
        frame,
        cols[2],
        "This Month Expenses",
        format!("-{}", Money::new(expenses).format(currency)),
        Style::default().fg(theme.negative),
        expense_ratio,
        theme,
    );
}

fn render_stat_card(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    value: String,
    value_style: Style,
    ratio: Option<(i64, i64)>,
    theme: &Theme,
) {
    let card = Card::new(title, theme).focused(true);
    let inner = card.inner(area);
    let mut lines = vec![Line::from(Span::styled(
        value,
        value_style.add_modifier(Modifier::BOLD),
    ))];

    if let Some((current, max)) = ratio {
        let bar_width = (inner.width as usize).saturating_sub(8);
        let bar = inline_progress_bar(current, Some(max), bar_width.min(20));
        lines.push(Line::from(Span::styled(
            bar,
            Style::default().fg(theme.dim),
        )));
    }

    card.render_with(frame, area, Paragraph::new(lines));
}

fn render_wallets_flows(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    render_wallets_panel(frame, cols[0], state, theme);
    render_flows_panel(frame, cols[1], state, theme);
}

fn render_wallets_panel(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);

    let card = Card::new("Wallets", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let items: Vec<ListItem> = state
        .snapshot
        .as_ref()
        .map(|snap| {
            snap.wallets
                .iter()
                .filter(|w| !w.archived)
                .take(inner.height as usize)
                .map(|wallet| {
                    let balance = styled_amount(wallet.balance_minor, currency, theme);
                    let name = Span::styled(&wallet.name, Style::default().fg(theme.text));

                    ListItem::new(Line::from(vec![name, Span::raw("  "), balance]))
                })
                .collect()
        })
        .unwrap_or_default();

    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("No wallets", Style::default().fg(theme.dim))),
            inner,
        );
    } else {
        frame.render_widget(List::new(items), inner);
    }
}

fn render_flows_panel(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);

    let card = Card::new("Flows", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let items: Vec<ListItem> = state
        .snapshot
        .as_ref()
        .map(|snap| {
            let max_balance = snap
                .flows
                .iter()
                .map(|flow| flow.balance_minor.saturating_abs() as u64)
                .max()
                .unwrap_or(0);

            snap.flows
                .iter()
                .filter(|f| !f.archived)
                .take(inner.height as usize)
                .map(|flow| {
                    let balance_str = Money::new(flow.balance_minor).format(currency);
                    let name_style = if flow.is_unallocated {
                        Style::default().fg(theme.dim)
                    } else {
                        Style::default().fg(theme.text)
                    };

                    // For now, show a simple bar (we don't have cap info in FlowView)
                    // TODO: Add cap info to FlowView API to show proper progress
                    let bar_width = 10;
                    let bar = ascii_bar(
                        flow.balance_minor.saturating_abs() as u64,
                        max_balance,
                        bar_width,
                    );

                    let balance_color = if flow.balance_minor >= 0 {
                        theme.positive
                    } else {
                        theme.negative
                    };

                    ListItem::new(Line::from(vec![
                        Span::styled(&flow.name, name_style),
                        Span::raw("  "),
                        Span::styled(balance_str, Style::default().fg(balance_color)),
                        Span::raw(" "),
                        Span::styled(bar, Style::default().fg(theme.dim)),
                    ]))
                })
                .collect()
        })
        .unwrap_or_default();

    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("No flows", Style::default().fg(theme.dim))),
            inner,
        );
    } else {
        frame.render_widget(List::new(items), inner);
    }
}

fn render_recent_transactions(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);

    let card = Card::new("Recent Transactions", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let items: Vec<ListItem> = state
        .transactions
        .items
        .iter()
        .take(inner.height as usize)
        .map(|tx| {
            let date = tx.occurred_at.format("%d %b").to_string();
            let kind = kind_label(tx.kind);
            let amount = styled_amount(
                if tx.kind == TransactionKind::Expense {
                    -tx.amount_minor.abs()
                } else {
                    tx.amount_minor
                },
                currency,
                theme,
            );

            let note = tx.note.as_deref().unwrap_or("");
            let category = tx
                .category
                .as_ref()
                .map(|c| format!("#{c} "))
                .unwrap_or_default();

            let kind_color = match tx.kind {
                TransactionKind::Income => theme.positive,
                TransactionKind::Expense => theme.negative,
                TransactionKind::Refund => theme.warning,
                TransactionKind::TransferWallet | TransactionKind::TransferFlow => theme.dim,
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{date:<6}"), Style::default().fg(theme.dim)),
                Span::styled(format!("{kind:<10}"), Style::default().fg(kind_color)),
                amount,
                Span::raw("  "),
                Span::styled(category, Style::default().fg(theme.accent)),
                Span::styled(note, Style::default().fg(theme.text_muted)),
            ]))
        })
        .collect();

    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No recent transactions",
                Style::default().fg(theme.dim),
            )),
            inner,
        );
    } else {
        frame.render_widget(List::new(items), inner);
    }
}

fn render_quick_actions(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let _ = state; // Unused for now

    let card = Card::new("Quick Actions", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let actions = Line::from(vec![
        Span::styled("[a]", Style::default().fg(theme.accent)),
        Span::raw(" Add expense   "),
        Span::styled("[i]", Style::default().fg(theme.accent)),
        Span::raw(" Add income   "),
        Span::styled("[t]", Style::default().fg(theme.accent)),
        Span::raw(" Go to transactions   "),
        Span::styled("[r]", Style::default().fg(theme.accent)),
        Span::raw(" Refresh"),
    ]);

    frame.render_widget(Paragraph::new(actions), inner);
}

fn get_currency(state: &AppState) -> Currency {
    state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur)
}

fn map_currency(currency: &api_types::Currency) -> Currency {
    match currency {
        api_types::Currency::Eur => Currency::Eur,
    }
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
