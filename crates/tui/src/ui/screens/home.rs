use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
};

use api_types::transaction::TransactionKind;
use chrono::{Datelike, Local, NaiveDate};
use engine::{Currency, Money};

use crate::{
    app::{AppState, FlowAlertSeverity, HomeFeedItem, home_feed_items},
    ui::{
        components::{
            card::Card,
            charts::mini_bar_chart,
            money::{styled_amount, styled_percentage_change},
        },
        theme::Theme,
    },
};

/// Transaction type icons
const ICON_INCOME: &str = "▲";
const ICON_EXPENSE: &str = "▼";
const ICON_REFUND: &str = "↩";
const ICON_TRANSFER: &str = "⇄";

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();

    // Main layout based on terminal width
    if area.width >= 100 {
        render_large_layout(frame, area, state, &theme);
    } else if area.width >= 80 {
        render_medium_layout(frame, area, state, &theme);
    } else {
        render_small_layout(frame, area, state, &theme);
    }
}

fn render_large_layout(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    // Main layout: Top row, middle row, recent transactions
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),  // Balance + This Month
            Constraint::Length(10), // Wallets + Budgets/Goals
            Constraint::Min(5),     // Recent transactions
        ])
        .split(area);

    // Top row: Balance (left) + This Month (right)
    let top_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(layout[0]);

    render_balance_card(frame, top_row[0], state, theme);
    render_this_month_card(frame, top_row[1], state, theme);

    // Middle row: Wallets (left) + Budgets/Goals (right)
    let middle_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(layout[1]);

    render_wallets_card(frame, middle_row[0], state, theme);
    render_budgets_goals_card(frame, middle_row[1], state, theme);

    // Bottom: Recent transactions
    render_recent_transactions(frame, layout[2], state, theme);
}

fn render_medium_layout(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Balance (compact)
            Constraint::Length(4), // This Month (compact)
            Constraint::Length(8), // Wallets + Budgets side by side
            Constraint::Min(5),    // Recent transactions
        ])
        .split(area);

    render_balance_card_compact(frame, layout[0], state, theme);
    render_this_month_card_compact(frame, layout[1], state, theme);

    let middle_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[2]);

    render_wallets_card(frame, middle_row[0], state, theme);
    render_budgets_goals_card(frame, middle_row[1], state, theme);

    render_recent_transactions(frame, layout[3], state, theme);
}

fn render_small_layout(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Balance (minimal)
            Constraint::Length(3), // This Month (minimal)
            Constraint::Length(5), // Wallets
            Constraint::Min(4),    // Recent transactions
        ])
        .split(area);

    render_balance_card_minimal(frame, layout[0], state, theme);
    render_this_month_card_minimal(frame, layout[1], state, theme);
    render_wallets_card_minimal(frame, layout[2], state, theme);
    render_recent_transactions_minimal(frame, layout[3], state, theme);
}

// === Balance Card ===

fn render_balance_card(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);
    let card = Card::new("Balance", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let total_balance: i64 = state
        .snapshot
        .as_ref()
        .map(|snap| snap.wallets.iter().map(|w| w.balance_minor).sum())
        .unwrap_or(0);

    // Calculate month change from stats
    let month_change = state
        .stats
        .data
        .as_ref()
        .map(|s| s.total_income_minor - s.total_expenses_minor)
        .unwrap_or(0);

    // Build sparkline
    let trend = mini_bar_chart(&state.stats.sparkline);

    // Center the content
    let balance_str = Money::new(total_balance).format(currency);
    let change_str = if month_change >= 0 {
        format!(
            "▲ +{} this month",
            Money::new(month_change).format(currency)
        )
    } else {
        format!("▼ {} this month", Money::new(month_change).format(currency))
    };

    let change_color = if month_change >= 0 {
        theme.positive
    } else {
        theme.negative
    };

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            balance_str,
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(change_str, Style::default().fg(change_color))),
    ];

    if !trend.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(trend, Style::default().fg(theme.info)),
            Span::styled("  last 30 days", Style::default().fg(theme.text_muted)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
}

fn render_balance_card_compact(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);
    let card = Card::new("Balance", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let total_balance: i64 = state
        .snapshot
        .as_ref()
        .map(|snap| snap.wallets.iter().map(|w| w.balance_minor).sum())
        .unwrap_or(0);

    let month_change = state
        .stats
        .data
        .as_ref()
        .map(|s| s.total_income_minor - s.total_expenses_minor)
        .unwrap_or(0);

    let trend = mini_bar_chart(&state.stats.sparkline);

    let balance_str = Money::new(total_balance).format(currency);
    let (arrow, change_color) = if month_change >= 0 {
        ("▲", theme.positive)
    } else {
        ("▼", theme.negative)
    };

    let line = Line::from(vec![
        Span::styled(
            format!("{balance_str}  "),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "{arrow} {}",
                Money::new(month_change.abs()).format(currency)
            ),
            Style::default().fg(change_color),
        ),
        Span::raw("  "),
        Span::styled(trend, Style::default().fg(theme.info)),
    ]);

    frame.render_widget(Paragraph::new(line), inner);
}

fn render_balance_card_minimal(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);

    let total_balance: i64 = state
        .snapshot
        .as_ref()
        .map(|snap| snap.wallets.iter().map(|w| w.balance_minor).sum())
        .unwrap_or(0);

    let month_change = state
        .stats
        .data
        .as_ref()
        .map(|s| s.total_income_minor - s.total_expenses_minor)
        .unwrap_or(0);

    let (arrow, change_color) = if month_change >= 0 {
        ("▲", theme.positive)
    } else {
        ("▼", theme.negative)
    };

    let line = Line::from(vec![
        Span::styled("Balance: ", Style::default().fg(theme.text_muted)),
        Span::styled(
            Money::new(total_balance).format(currency),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                "{arrow} {}",
                Money::new(month_change.abs()).format(currency)
            ),
            Style::default().fg(change_color),
        ),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

// === This Month Card ===

fn render_this_month_card(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);
    let now = Local::now();
    let month_name = now.format("%B %Y").to_string();

    let card = Card::new(&month_name, theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let (income, expenses) = state
        .stats
        .data
        .as_ref()
        .map(|s| (s.total_income_minor, s.total_expenses_minor))
        .unwrap_or((0, 0));

    let net = income - expenses;

    // Progress bar width
    let bar_width = 14;
    let max_for_bar = income.max(expenses).max(1);

    let income_bar = progress_bar(income, max_for_bar, bar_width);
    let expense_bar = progress_bar(expenses, max_for_bar, bar_width);

    let income_pct = if max_for_bar > 0 {
        (income as f64 / max_for_bar as f64 * 100.0) as u16
    } else {
        0
    };
    let expense_pct = if max_for_bar > 0 {
        (expenses as f64 / max_for_bar as f64 * 100.0) as u16
    } else {
        0
    };

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Income     ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("{:<10}", Money::new(income).format(currency)),
                Style::default().fg(theme.positive),
            ),
            Span::styled(income_bar, Style::default().fg(theme.positive)),
            Span::styled(
                format!(" {:>3}%", income_pct),
                Style::default().fg(theme.text_muted),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Expenses   ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("{:<10}", Money::new(expenses).format(currency)),
                Style::default().fg(theme.negative),
            ),
            Span::styled(expense_bar, Style::default().fg(theme.negative)),
            Span::styled(
                format!(" {:>3}%", expense_pct),
                Style::default().fg(theme.text_muted),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Net        ", Style::default().fg(theme.text_muted)),
            styled_amount(net, currency, theme),
            Span::raw("     "),
            styled_percentage_change(
                if expenses > 0 {
                    (net as f64 / expenses as f64) * 100.0
                } else {
                    0.0
                },
                theme,
            ),
        ]),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_this_month_card_compact(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    theme: &Theme,
) {
    let currency = get_currency(state);
    let now = Local::now();
    let month_name = now.format("%b %Y").to_string();

    let card = Card::new(&month_name, theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let (income, expenses) = state
        .stats
        .data
        .as_ref()
        .map(|s| (s.total_income_minor, s.total_expenses_minor))
        .unwrap_or((0, 0));

    let net = income - expenses;
    let bar_width = 8;
    let max_for_bar = income.max(expenses).max(1);

    let line = Line::from(vec![
        Span::styled("Income ", Style::default().fg(theme.text_muted)),
        Span::styled(
            Money::new(income).format(currency),
            Style::default().fg(theme.positive),
        ),
        Span::raw(" "),
        Span::styled(
            progress_bar(income, max_for_bar, bar_width),
            Style::default().fg(theme.positive),
        ),
        Span::raw("   "),
        Span::styled("Expenses ", Style::default().fg(theme.text_muted)),
        Span::styled(
            Money::new(expenses).format(currency),
            Style::default().fg(theme.negative),
        ),
        Span::raw(" "),
        Span::styled(
            progress_bar(expenses, max_for_bar, bar_width),
            Style::default().fg(theme.negative),
        ),
        Span::raw("   "),
        Span::styled("Net ", Style::default().fg(theme.text_muted)),
        styled_amount(net, currency, theme),
    ]);

    frame.render_widget(Paragraph::new(line), inner);
}

fn render_this_month_card_minimal(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    theme: &Theme,
) {
    let currency = get_currency(state);
    let now = Local::now();
    let month_name = now.format("%b %Y").to_string();

    let (income, expenses) = state
        .stats
        .data
        .as_ref()
        .map(|s| (s.total_income_minor, s.total_expenses_minor))
        .unwrap_or((0, 0));

    let line = Line::from(vec![
        Span::styled(
            format!("{month_name}: "),
            Style::default().fg(theme.text_muted),
        ),
        Span::styled("Income ", Style::default().fg(theme.text_muted)),
        Span::styled(
            Money::new(income).format(currency),
            Style::default().fg(theme.positive),
        ),
        Span::raw(" | "),
        Span::styled("Expenses ", Style::default().fg(theme.text_muted)),
        Span::styled(
            Money::new(expenses).format(currency),
            Style::default().fg(theme.negative),
        ),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

// === Wallets Card ===

fn render_wallets_card(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);
    let card = Card::new("Wallets", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let Some(snapshot) = state.snapshot.as_ref() else {
        render_empty_state(
            frame,
            inner,
            "No wallets yet",
            "[w] to create your first wallet",
            theme,
        );
        return;
    };

    let wallets: Vec<_> = snapshot
        .wallets
        .iter()
        .filter(|w| !w.archived)
        .take(inner.height.saturating_sub(1) as usize)
        .collect();

    if wallets.is_empty() {
        render_empty_state(
            frame,
            inner,
            "No wallets yet",
            "[w] to create your first wallet",
            theme,
        );
        return;
    }

    let mut items: Vec<ListItem> = wallets
        .iter()
        .map(|wallet| {
            let emoji = "💰";
            let balance_color = if wallet.balance_minor >= 0 {
                theme.positive
            } else {
                theme.negative
            };

            ListItem::new(Line::from(vec![
                Span::raw(format!("  {emoji} ")),
                Span::styled(
                    format!("{:<14}", wallet.name),
                    Style::default().fg(theme.text),
                ),
                Span::styled(
                    format!("{:>12}", Money::new(wallet.balance_minor).format(currency)),
                    Style::default().fg(balance_color),
                ),
            ]))
        })
        .collect();

    // Add footer action
    if inner.height as usize > items.len() + 1 {
        items.push(ListItem::new(Line::from("")));
        items.push(ListItem::new(Line::from(vec![
            Span::styled("  [", Style::default().fg(theme.text_muted)),
            Span::styled("w", Style::default().fg(theme.accent)),
            Span::styled("] Manage wallets", Style::default().fg(theme.text_muted)),
        ])));
    }

    frame.render_widget(List::new(items), inner);
}

fn render_wallets_card_minimal(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);

    let Some(snapshot) = state.snapshot.as_ref() else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No wallets",
                Style::default().fg(theme.text_muted),
            )),
            area,
        );
        return;
    };

    let wallets: Vec<_> = snapshot
        .wallets
        .iter()
        .filter(|w| !w.archived)
        .take(area.height as usize)
        .collect();

    let items: Vec<ListItem> = wallets
        .iter()
        .map(|wallet| {
            let emoji = "💰";
            let balance_color = if wallet.balance_minor >= 0 {
                theme.positive
            } else {
                theme.negative
            };

            ListItem::new(Line::from(vec![
                Span::raw(format!("  {emoji} ")),
                Span::styled(&wallet.name, Style::default().fg(theme.text)),
                Span::raw("  "),
                Span::styled(
                    Money::new(wallet.balance_minor).format(currency),
                    Style::default().fg(balance_color),
                ),
            ]))
        })
        .collect();

    frame.render_widget(List::new(items), area);
}

// === Budgets & Goals Card ===

fn render_budgets_goals_card(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);
    let card = Card::new("Budgets & Goals", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let Some(snapshot) = state.snapshot.as_ref() else {
        render_empty_state(
            frame,
            inner,
            "No budgets set up yet",
            "[f] to create a budget",
            theme,
        );
        return;
    };

    let flows: Vec<_> = snapshot
        .flows
        .iter()
        .filter(|f| !f.archived && !f.is_unallocated)
        .collect();

    if flows.is_empty() {
        render_empty_state(
            frame,
            inner,
            "No budgets set up yet",
            "[f] to create a budget",
            theme,
        );
        return;
    }

    // Separate budgets (expenses) from goals (savings)
    // For now, treat all as budgets since we don't have goal distinction in API
    let bar_width = 10;

    let max_balance = flows
        .iter()
        .map(|f| f.balance_minor.unsigned_abs())
        .max()
        .unwrap_or(1);

    let items: Vec<ListItem> = flows
        .iter()
        .take(inner.height as usize)
        .map(|flow| {
            let emoji = "📦";
            let balance = flow.balance_minor;
            let bar = progress_bar(balance.unsigned_abs() as i64, max_balance as i64, bar_width);

            let balance_color = if balance >= 0 {
                theme.positive
            } else {
                theme.negative
            };

            ListItem::new(Line::from(vec![
                Span::raw(format!("  {emoji} ")),
                Span::styled(
                    format!("{:<12}", truncate(&flow.name, 12)),
                    Style::default().fg(theme.text),
                ),
                Span::styled(
                    format!("{:>10}", Money::new(balance).format(currency)),
                    Style::default().fg(balance_color),
                ),
                Span::raw("  "),
                Span::styled(bar, Style::default().fg(theme.accent)),
            ]))
        })
        .collect();

    frame.render_widget(List::new(items), inner);
}

// === Recent Transactions Card ===

fn render_recent_transactions(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);
    let card = Card::new("Recent Transactions", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let feed_items = home_feed_items(state);
    if feed_items.is_empty() {
        render_empty_state(
            frame,
            inner,
            "No transactions yet",
            "[n] to add your first transaction",
            theme,
        );
        return;
    }

    let today = Local::now().date_naive();
    let yesterday = today - chrono::Duration::days(1);

    let mut items: Vec<ListItem> = Vec::new();
    let mut last_date: Option<NaiveDate> = None;
    let mut in_transactions = false;
    let mut selected_row = None;
    let has_alerts = feed_items
        .iter()
        .any(|item| matches!(item, HomeFeedItem::FlowAlert(_)));

    if has_alerts {
        items.push(ListItem::new(Line::from(Span::styled(
            "  Alerts",
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ))));
    }

    for (feed_idx, item) in feed_items.iter().enumerate() {
        match item {
            HomeFeedItem::FlowAlert(alert) => {
                if feed_idx == state.home_feed_selected {
                    selected_row = Some(items.len());
                }
                items.push(flow_alert_item(alert, currency, theme));
            }
            HomeFeedItem::Transaction { index } => {
                let Some(tx) = state.transactions.items.get(*index) else {
                    continue;
                };
                if !in_transactions {
                    if has_alerts && !items.is_empty() {
                        items.push(ListItem::new(Line::from("")));
                    }
                    items.push(ListItem::new(Line::from(Span::styled(
                        "  Transactions",
                        Style::default()
                            .fg(theme.text_muted)
                            .add_modifier(Modifier::BOLD),
                    ))));
                    in_transactions = true;
                }

                let tx_date = tx.occurred_at.date_naive();
                if last_date != Some(tx_date) {
                    if last_date.is_some() {
                        items.push(ListItem::new(Line::from("")));
                    }
                    let date_label = format_date_label(tx_date, today, yesterday);
                    items.push(ListItem::new(Line::from(Span::styled(
                        format!("  {date_label}"),
                        Style::default()
                            .fg(theme.text_muted)
                            .add_modifier(Modifier::BOLD),
                    ))));
                    last_date = Some(tx_date);
                }

                let (icon, icon_color) = match tx.kind {
                    TransactionKind::Income => (ICON_INCOME, theme.income),
                    TransactionKind::Expense => (ICON_EXPENSE, theme.expense),
                    TransactionKind::Refund => (ICON_REFUND, theme.refund),
                    TransactionKind::TransferWallet | TransactionKind::TransferFlow => {
                        (ICON_TRANSFER, theme.transfer)
                    }
                };

                let amount = if tx.kind == TransactionKind::Expense {
                    -tx.amount_minor.abs()
                } else {
                    tx.amount_minor
                };

                let note = tx.note.as_deref().unwrap_or("");
                let category = tx
                    .category
                    .as_ref()
                    .map(|c| format!("#{c}"))
                    .unwrap_or_default();
                let time = tx.occurred_at.format("%H:%M").to_string();

                if feed_idx == state.home_feed_selected {
                    selected_row = Some(items.len());
                }

                items.push(ListItem::new(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(icon, Style::default().fg(icon_color)),
                    Span::raw(" "),
                    styled_amount(amount, currency, theme),
                    Span::raw("  "),
                    Span::styled(
                        format!("{:<24}", truncate(note, 24)),
                        Style::default().fg(theme.text),
                    ),
                    Span::styled(
                        format!("{:<12}", truncate(&category, 12)),
                        Style::default().fg(theme.accent),
                    ),
                    Span::styled(time, Style::default().fg(theme.text_muted)),
                ])));
            }
        }
    }

    // Add footer
    if inner.height as usize > items.len() + 1 {
        items.push(ListItem::new(Line::from("")));
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("[", Style::default().fg(theme.text_muted)),
            Span::styled("t", Style::default().fg(theme.accent)),
            Span::styled("] View all →", Style::default().fg(theme.text_muted)),
        ])));
    }

    let mut list_state = ratatui::widgets::ListState::default();
    if let Some(row) = selected_row {
        list_state.select(Some(row));
    }

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("» ");

    frame.render_stateful_widget(list, inner, &mut list_state);
}

fn render_recent_transactions_minimal(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    theme: &Theme,
) {
    let currency = get_currency(state);

    let feed_items = home_feed_items(state);
    if feed_items.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No transactions",
                Style::default().fg(theme.text_muted),
            )),
            area,
        );
        return;
    }

    let mut selected_row = None;
    let items: Vec<ListItem> = feed_items
        .iter()
        .enumerate()
        .take(area.height as usize)
        .filter_map(|(feed_idx, item)| match item {
            HomeFeedItem::FlowAlert(alert) => {
                if feed_idx == state.home_feed_selected {
                    selected_row = Some(feed_idx);
                }
                Some(flow_alert_item(alert, currency, theme))
            }
            HomeFeedItem::Transaction { index } => {
                let tx = state.transactions.items.get(*index)?;
                let (icon, icon_color) = match tx.kind {
                    TransactionKind::Income => (ICON_INCOME, theme.income),
                    TransactionKind::Expense => (ICON_EXPENSE, theme.expense),
                    TransactionKind::Refund => (ICON_REFUND, theme.refund),
                    TransactionKind::TransferWallet | TransactionKind::TransferFlow => {
                        (ICON_TRANSFER, theme.transfer)
                    }
                };

                let amount = if tx.kind == TransactionKind::Expense {
                    -tx.amount_minor.abs()
                } else {
                    tx.amount_minor
                };
                let note = tx.note.as_deref().unwrap_or("");

                if feed_idx == state.home_feed_selected {
                    selected_row = Some(feed_idx);
                }

                Some(ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(icon, Style::default().fg(icon_color)),
                    Span::raw(" "),
                    styled_amount(amount, currency, theme),
                    Span::raw(" "),
                    Span::styled(truncate(note, 20), Style::default().fg(theme.text)),
                ])))
            }
        })
        .collect();

    let mut list_state = ratatui::widgets::ListState::default();
    if let Some(row) = selected_row {
        list_state.select(Some(row));
    }
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("» ");

    frame.render_stateful_widget(list, area, &mut list_state);
}

// === Helper Functions ===

fn flow_alert_item(
    alert: &crate::app::FlowAlertItem,
    currency: Currency,
    theme: &Theme,
) -> ListItem<'static> {
    let (icon, color) = match alert.severity {
        FlowAlertSeverity::Critical => ("‼", theme.negative),
        FlowAlertSeverity::Warning => ("⚠", theme.warning),
    };
    let balance = Money::new(alert.balance_minor).format(currency);
    let label = match alert.severity {
        FlowAlertSeverity::Critical => "deficit".to_string(),
        FlowAlertSeverity::Warning => {
            let threshold = Money::new(alert.threshold_minor).format(currency);
            format!("≤ {threshold}")
        }
    };

    ListItem::new(Line::from(vec![
        Span::raw("    "),
        Span::styled(
            icon,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{:<18}", truncate(alert.name.as_str(), 18)),
            Style::default().fg(theme.text),
        ),
        Span::styled(
            format!("{:<14}", truncate(label.as_str(), 14)),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(balance, Style::default().fg(color)),
    ]))
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

fn progress_bar(value: i64, max: i64, width: usize) -> String {
    if max == 0 {
        return "░".repeat(width);
    }

    let ratio = (value.unsigned_abs() as f64 / max.unsigned_abs() as f64).clamp(0.0, 1.0);
    let filled = ((ratio * width as f64) as usize).min(width);
    let empty = width.saturating_sub(filled);

    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_len - 1).collect::<String>())
    }
}

fn format_date_label(date: NaiveDate, today: NaiveDate, yesterday: NaiveDate) -> String {
    if date == today {
        "Today".to_string()
    } else if date == yesterday {
        "Yesterday".to_string()
    } else if date.year() == today.year() {
        date.format("%d %b").to_string()
    } else {
        date.format("%d %b %Y").to_string()
    }
}

fn render_empty_state(frame: &mut Frame<'_>, area: Rect, message: &str, hint: &str, theme: &Theme) {
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(message, Style::default().fg(theme.text_muted))),
        Line::from(""),
        Line::from(Span::styled(hint, Style::default().fg(theme.text_muted))),
    ];

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}
