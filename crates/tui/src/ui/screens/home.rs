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
            charts::{PieSlice, ascii_bar, mini_bar_chart, render_pie_chart},
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

    let Some(snapshot) = state.snapshot.as_ref() else {
        frame.render_widget(
            Paragraph::new(Span::styled("No wallets", Style::default().fg(theme.dim))),
            inner,
        );
        return;
    };

    let palette = pie_palette(theme);
    let entries: Vec<(String, i64, ratatui::style::Color)> = snapshot
        .wallets
        .iter()
        .filter(|wallet| !wallet.archived)
        .enumerate()
        .map(|(idx, wallet)| {
            let color = palette[idx % palette.len()];
            (wallet.name.clone(), wallet.balance_minor, color)
        })
        .collect();

    let show_pie = should_show_pie(inner);
    let (pie_area, list_area) = if show_pie {
        let pie_width = pie_width(inner.width);
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(pie_width), Constraint::Min(0)])
            .split(inner);
        (Some(columns[0]), columns[1])
    } else {
        (None, inner)
    };

    if let Some(pie_area) = pie_area {
        let slices: Vec<PieSlice> = entries
            .iter()
            .filter(|(_, balance, _)| *balance != 0)
            .map(|(_, balance, color)| PieSlice {
                value: balance.unsigned_abs(),
                color: *color,
            })
            .collect();
        render_pie_chart(frame, pie_area, "", &slices, theme);
    }

    let items: Vec<ListItem> = entries
        .iter()
        .take(list_area.height as usize)
        .map(|(name, balance, color)| {
            let balance = styled_amount(*balance, currency, theme);
            let marker = Span::styled("●", Style::default().fg(*color));
            let name = Span::styled(name.as_str(), Style::default().fg(theme.text));

            ListItem::new(Line::from(vec![
                marker,
                Span::raw(" "),
                name,
                Span::raw("  "),
                balance,
            ]))
        })
        .collect();

    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("No wallets", Style::default().fg(theme.dim))),
            list_area,
        );
    } else {
        frame.render_widget(List::new(items), list_area);
    }
}

fn render_flows_panel(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);

    let card = Card::new("Flows", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let Some(snapshot) = state.snapshot.as_ref() else {
        frame.render_widget(
            Paragraph::new(Span::styled("No flows", Style::default().fg(theme.dim))),
            inner,
        );
        return;
    };

    let palette = pie_palette(theme);
    let entries: Vec<(String, i64, bool, ratatui::style::Color)> = snapshot
        .flows
        .iter()
        .filter(|flow| !flow.archived)
        .enumerate()
        .map(|(idx, flow)| {
            let color = palette[idx % palette.len()];
            (
                flow.name.clone(),
                flow.balance_minor,
                flow.is_unallocated,
                color,
            )
        })
        .collect();

    let show_pie = should_show_pie(inner);
    let (pie_area, list_area) = if show_pie {
        let pie_width = pie_width(inner.width);
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(pie_width), Constraint::Min(0)])
            .split(inner);
        (Some(columns[0]), columns[1])
    } else {
        (None, inner)
    };

    if let Some(pie_area) = pie_area {
        let slices: Vec<PieSlice> = entries
            .iter()
            .filter(|(_, balance, _, _)| *balance != 0)
            .map(|(_, balance, _, color)| PieSlice {
                value: balance.unsigned_abs(),
                color: *color,
            })
            .collect();
        render_pie_chart(frame, pie_area, "", &slices, theme);
    }

    let max_balance = entries
        .iter()
        .map(|(_, balance, _, _)| balance.unsigned_abs())
        .max()
        .unwrap_or(0);

    let items: Vec<ListItem> = entries
        .iter()
        .take(list_area.height as usize)
        .map(|(name, balance, is_unallocated, color)| {
            let balance_str = Money::new(*balance).format(currency);
            let name_style = if *is_unallocated {
                Style::default().fg(theme.dim)
            } else {
                Style::default().fg(theme.text)
            };

            // For now, show a simple bar (we don't have cap info in FlowView)
            // TODO: Add cap info to FlowView API to show proper progress
            let bar_width = 10;
            let bar = ascii_bar(balance.unsigned_abs(), max_balance, bar_width);

            let balance_color = if *balance >= 0 {
                theme.positive
            } else {
                theme.negative
            };
            let marker = Span::styled("●", Style::default().fg(*color));

            ListItem::new(Line::from(vec![
                marker,
                Span::raw(" "),
                Span::styled(name.as_str(), name_style),
                Span::raw("  "),
                Span::styled(balance_str, Style::default().fg(balance_color)),
                Span::raw(" "),
                Span::styled(bar, Style::default().fg(theme.dim)),
            ]))
        })
        .collect();

    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("No flows", Style::default().fg(theme.dim))),
            list_area,
        );
    } else {
        frame.render_widget(List::new(items), list_area);
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

fn pie_palette(theme: &Theme) -> [ratatui::style::Color; 6] {
    [
        theme.accent,
        theme.positive,
        theme.warning,
        theme.negative,
        theme.text,
        theme.text_muted,
    ]
}

fn should_show_pie(area: Rect) -> bool {
    let pie_width = pie_width(area.width);
    area.height >= 7 && area.width >= pie_width.saturating_add(14)
}

fn pie_width(total_width: u16) -> u16 {
    let width = total_width.saturating_mul(2) / 5;
    width.clamp(10, 18)
}
