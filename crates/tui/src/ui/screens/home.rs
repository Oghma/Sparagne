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
    text::{TextKey, t},
    ui::{
        components::{
            card::Card,
            charts::braille_sparkline_filled,
            money::{styled_amount_emoji, styled_percentage_change},
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
    // Stats bar (6 rows) + Main content below
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(8)])
        .split(area);

    render_stats_bar(frame, main_layout[0], state, theme);

    // Main content: Quick Balances (30%) | Activity Feed (70%)
    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(main_layout[1]);

    render_quick_balances(frame, content_layout[0], state, theme);
    render_activity_feed(frame, content_layout[1], state, theme);
}

fn render_medium_layout(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    // Stats bar (5 rows) + Quick Balances (8 rows) + Activity Feed (rest)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(8),
            Constraint::Min(6),
        ])
        .split(area);

    render_stats_bar(frame, layout[0], state, theme);
    render_quick_balances(frame, layout[1], state, theme);
    render_activity_feed(frame, layout[2], state, theme);
}

fn render_small_layout(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    // Compact stats (1 row) + Quick Balances (6 rows) + Activity Feed (rest)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(6),
            Constraint::Min(4),
        ])
        .split(area);

    render_stats_bar_compact(frame, layout[0], state, theme);
    render_quick_balances(frame, layout[1], state, theme);
    render_activity_feed(frame, layout[2], state, theme);
}

// === Stats Bar ===

fn render_stats_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);

    render_net_worth_card(frame, layout[0], state, theme);

    let (income, expenses) = state
        .stats
        .data
        .as_ref()
        .map(|s| (s.total_income_minor, s.total_expenses_minor))
        .unwrap_or((0, 0));

    render_stat_card(
        frame,
        layout[1],
        state,
        theme,
        TextKey::HomeCardIncome,
        ICON_INCOME,
        income,
        theme.income,
    );
    render_stat_card(
        frame,
        layout[2],
        state,
        theme,
        TextKey::HomeCardExpenses,
        ICON_EXPENSE,
        expenses,
        theme.expense,
    );
}

fn render_stats_bar_compact(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);

    let net_worth_minor: i64 = state
        .snapshot
        .as_ref()
        .map(|snap| {
            snap.wallets
                .iter()
                .filter(|w| !w.archived)
                .map(|w| w.balance_minor)
                .sum()
        })
        .unwrap_or(0);

    let (income, expenses) = state
        .stats
        .data
        .as_ref()
        .map(|s| (s.total_income_minor, s.total_expenses_minor))
        .unwrap_or((0, 0));

    let net_worth = Money::new(net_worth_minor).format(currency);
    let income_str = Money::new(income).format(currency);
    let expenses_str = Money::new(expenses).format(currency);

    let line = Line::from(vec![
        Span::styled("Net Worth: ", Style::default().fg(theme.text_muted)),
        Span::styled(
            net_worth,
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  │  ", Style::default().fg(theme.border)),
        Span::styled("In: ", Style::default().fg(theme.text_muted)),
        Span::styled(income_str, Style::default().fg(theme.income)),
        Span::styled("  │  ", Style::default().fg(theme.border)),
        Span::styled("Out: ", Style::default().fg(theme.text_muted)),
        Span::styled(expenses_str, Style::default().fg(theme.expense)),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

fn render_net_worth_card(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);
    let card = Card::new("Net Worth", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let net_worth_minor: i64 = state
        .snapshot
        .as_ref()
        .map(|snap| {
            snap.wallets
                .iter()
                .filter(|w| !w.archived)
                .map(|w| w.balance_minor)
                .sum()
        })
        .unwrap_or(0);

    let net_worth = Money::new(net_worth_minor).format(currency);

    let (income, expenses) = state
        .stats
        .data
        .as_ref()
        .map(|s| (s.total_income_minor, s.total_expenses_minor))
        .unwrap_or((0, 0));

    let net = income - expenses;
    let pct_change = if expenses > 0 {
        (net as f64 / expenses as f64) * 100.0
    } else {
        0.0
    };

    let trend = braille_sparkline_filled(&state.stats.sparkline);

    // Amount and percentage centered
    let centered_lines = vec![
        Line::from(Span::styled(
            net_worth,
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        )),
        Line::from(styled_percentage_change(pct_change, theme)),
    ];

    // Split inner area: centered content on top, sparkline at bottom
    if !trend.is_empty() && inner.height >= 3 {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Length(1)])
            .split(inner);

        frame.render_widget(
            Paragraph::new(centered_lines).alignment(Alignment::Center),
            layout[0],
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                trend,
                Style::default().fg(theme.info),
            ))),
            layout[1],
        );
    } else {
        frame.render_widget(
            Paragraph::new(centered_lines).alignment(Alignment::Center),
            inner,
        );
    }
}

fn render_stat_card(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    title: TextKey,
    icon: &str,
    amount: i64,
    color: ratatui::style::Color,
) {
    let currency = get_currency(state);
    let card = Card::new(t(state.locale, title), theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let amount_str = Money::new(amount).format(currency);

    let lines = vec![
        Line::from(vec![
            Span::styled(icon, Style::default().fg(color)),
            Span::raw(" "),
            Span::styled(
                amount_str,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            t(state.locale, TextKey::HomeCardThisMonth),
            Style::default().fg(theme.text_muted),
        )),
    ];

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
}

// === Quick Balances ===

fn render_quick_balances(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);
    let card = Card::new("Quick Balances", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let Some(snapshot) = state.snapshot.as_ref() else {
        render_empty_state(
            frame,
            inner,
            "No data yet",
            "[n] to add your first transaction",
            theme,
        );
        return;
    };

    let mut lines: Vec<Line> = Vec::new();

    // Wallets section
    let mut wallets: Vec<_> = snapshot.wallets.iter().filter(|w| !w.archived).collect();
    wallets.sort_by(|a, b| b.balance_minor.cmp(&a.balance_minor));

    if !wallets.is_empty() {
        lines.push(Line::from(Span::styled(
            "Wallets",
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::BOLD),
        )));

        // Calculate how much space we have
        let available_height = inner.height as usize;
        let max_wallets = (available_height / 2).max(2).min(wallets.len());

        for wallet in wallets.iter().take(max_wallets) {
            let balance_color = if wallet.balance_minor >= 0 {
                theme.positive
            } else {
                theme.negative
            };
            lines.push(Line::from(vec![
                Span::raw("  💰 "),
                Span::styled(
                    format!("{:<12}", truncate(&wallet.name, 12)),
                    Style::default().fg(theme.text),
                ),
                Span::styled(
                    Money::new(wallet.balance_minor).format(currency),
                    Style::default().fg(balance_color),
                ),
            ]));
        }
    }

    // Budgets section (flows that are not archived and not "Unallocated")
    let flows: Vec<_> = snapshot
        .flows
        .iter()
        .filter(|f| !f.archived && !f.is_unallocated)
        .collect();

    if !flows.is_empty() {
        let remaining = inner.height as usize - lines.len();
        if remaining > 2 {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Budgets",
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::BOLD),
            )));

            let max_flows = (remaining - 3).max(1).min(flows.len());

            for flow in flows.iter().take(max_flows) {
                let balance_color = if flow.balance_minor >= 0 {
                    theme.positive
                } else {
                    theme.negative
                };
                lines.push(Line::from(vec![
                    Span::raw("  📦 "),
                    Span::styled(
                        format!("{:<12}", truncate(&flow.name, 12)),
                        Style::default().fg(theme.text),
                    ),
                    Span::styled(
                        Money::new(flow.balance_minor).format(currency),
                        Style::default().fg(balance_color),
                    ),
                ]));
            }
        }
    }

    lines.truncate(inner.height as usize);
    frame.render_widget(Paragraph::new(lines), inner);
}

// === Activity Feed ===

fn render_activity_feed(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);
    let card = Card::new("Activity Feed", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let feed_items = home_feed_items(state);
    if feed_items.is_empty() {
        render_empty_state(
            frame,
            inner,
            "No activity yet",
            "[n] to add your first transaction",
            theme,
        );
        return;
    }

    let today = Local::now().date_naive();
    let yesterday = today - chrono::Duration::days(1);

    let mut items: Vec<ListItem> = Vec::new();
    let mut last_date: Option<NaiveDate> = None;
    let mut selected_row = None;
    let insight = home_insight(state, currency);
    let mut insight_inserted = false;

    let note_width = (inner.width as usize).saturating_sub(30).clamp(14, 32);
    let cat_width = 12usize.min(inner.width as usize);
    let show_time = inner.width >= 50;
    let show_meta = inner.width >= 70;

    for (feed_idx, item) in feed_items.iter().enumerate() {
        match item {
            HomeFeedItem::FlowAlert(alert) => {
                if feed_idx == state.home_feed_selected {
                    selected_row = Some(items.len());
                }
                let balance = Money::new(alert.balance_minor).format(currency);
                let label = match alert.severity {
                    FlowAlertSeverity::Critical => "deficit".to_string(),
                    FlowAlertSeverity::Warning => {
                        let threshold = Money::new(alert.threshold_minor).format(currency);
                        format!("≤ {threshold}")
                    }
                };
                items.push(ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled("⚠", Style::default().fg(theme.warning)),
                    Span::raw(" "),
                    Span::styled(
                        "Alert: ".to_string(),
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{} ", truncate(&alert.name, 18)),
                        Style::default().fg(theme.text),
                    ),
                    Span::styled(
                        format!("{label} ({balance})"),
                        Style::default().fg(theme.warning),
                    ),
                ])));
            }
            HomeFeedItem::Transaction { index } => {
                let Some(tx) = state.transactions.items.get(*index) else {
                    continue;
                };
                let tx_date = tx.occurred_at.date_naive();
                if last_date != Some(tx_date) {
                    let date_label = format_date_label(tx_date, today, yesterday);
                    items.push(ListItem::new(Line::from(Span::styled(
                        format!("  {date_label}"),
                        Style::default()
                            .fg(theme.text_muted)
                            .add_modifier(Modifier::BOLD),
                    ))));
                    last_date = Some(tx_date);
                    if !insight_inserted && let Some(insight_text) = insight.as_ref() {
                        items.push(ListItem::new(Line::from(Span::styled(
                            insight_text.clone(),
                            Style::default().fg(theme.info),
                        ))));
                        insight_inserted = true;
                    }
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

                let note = tx.note.as_deref().unwrap_or("-");
                let category = tx.category.as_deref();
                let time = tx.occurred_at.format("%H:%M").to_string();

                if feed_idx == state.home_feed_selected {
                    selected_row = Some(items.len());
                }

                // Layout: icon → amount → time → note → [category]
                let mut line = vec![
                    Span::raw("  "),
                    Span::styled(icon, Style::default().fg(icon_color)),
                    Span::raw(" "),
                    styled_amount_emoji(amount, currency, theme, state.emoji_mode),
                ];

                if show_time {
                    line.push(Span::raw("  "));
                    line.push(Span::styled(time, Style::default().fg(theme.text_muted)));
                }

                line.push(Span::raw("  "));
                line.push(Span::styled(
                    format!("{:<note_width$}", truncate(note, note_width)),
                    Style::default().fg(theme.text),
                ));

                if show_meta {
                    if let Some(category) = category {
                        line.push(Span::raw("  "));
                        line.push(Span::styled(
                            format!("🏷{}", truncate(category, cat_width)),
                            Style::default().fg(theme.accent),
                        ));
                    }
                }

                items.push(ListItem::new(Line::from(line)));
            }
        }
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

// === Helper Functions ===

fn home_insight(state: &AppState, currency: Currency) -> Option<String> {
    let stats = state.stats.data.as_ref()?;
    let income = stats.total_income_minor;
    let expenses = stats.total_expenses_minor;
    if income == 0 && expenses == 0 {
        return None;
    }
    let net = income - expenses;
    if net > 0 {
        let pct = if income > 0 {
            (net as f64 / income as f64) * 100.0
        } else {
            0.0
        };
        Some(format!(
            "  💡 Insight: You saved {} ({pct:.1}% of income) this month",
            Money::new(net).format(currency)
        ))
    } else if net < 0 {
        Some(format!(
            "  💡 Insight: Expenses exceed income by {} this month",
            Money::new(net.abs()).format(currency)
        ))
    } else {
        None
    }
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
