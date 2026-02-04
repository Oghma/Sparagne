use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use engine::{Currency, Money};

use crate::{
    app::{AppState, StatsTab},
    ui::{
        components::{
            card::{Card, StatCard},
            charts::{
                BarStyle, PieSlice, ascii_bar_styled, compute_percentage, percentage_bar,
                render_bar_chart, render_braille_sparkline, render_inline_sparkline,
                render_pie_chart,
            },
            money::{
                flow_cap_gauge, styled_amount_bold_emoji, styled_amount_no_sign,
                styled_percentage_change,
            },
            tab_bar::{self, TabBarItem},
        },
        theme::Theme,
    },
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();

    // Show error state if stats loading failed
    if let Some(error) = &state.stats.error {
        let card = Card::new("Stats", &theme);
        let inner = card.inner(area);
        card.render_frame(frame, area);

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(error.as_str(), Style::default().fg(theme.error)),
                Span::raw(" Press "),
                Span::styled("r", Style::default().fg(theme.accent)),
                Span::raw(" to refresh."),
            ]))
            .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    // Show empty state if no data
    if state.stats.data.is_none() {
        let card = Card::new("Stats", &theme);
        let inner = card.inner(area);
        card.render_frame(frame, area);

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("No data. Press "),
                Span::styled("r", Style::default().fg(theme.accent)),
                Span::raw(" to refresh."),
            ]))
            .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    // Layout: tab bar + tab content
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    render_tab_bar(frame, layout[0], state, &theme);
    render_tab_content(frame, layout[1], state, &theme);
}

fn render_tab_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let items = [
        TabBarItem::new("1 Cash Flow"),
        TabBarItem::new("2 Spending"),
        TabBarItem::new("3 Net Worth"),
    ];
    tab_bar::render(frame, area, &items, state.stats.tab.index(), theme);
}

fn render_tab_content(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    match state.stats.tab {
        StatsTab::CashFlow => render_cash_flow_tab(frame, area, state, theme),
        StatsTab::Spending => render_spending_tab(frame, area, state, theme),
        StatsTab::NetWorth => render_net_worth_tab(frame, area, state, theme),
    }
}

fn render_cash_flow_tab(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // StatCards row
            Constraint::Length(9),  // Month summary with navigation
            Constraint::Length(6),  // Sparkline
            Constraint::Length(12), // Category breakdown
            Constraint::Min(6),     // Monthly trend chart
        ])
        .split(area);

    render_stat_cards(frame, layout[0], state, theme);
    render_month_summary(frame, layout[1], state, theme);
    render_sparkline(frame, layout[2], state, theme);
    render_category_breakdown(frame, layout[3], state, theme);
    render_monthly_trend(frame, layout[4], state, theme);
}

fn render_spending_tab(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),  // Sparkline for expense trend
            Constraint::Length(12), // Category breakdown
            Constraint::Min(6),     // Trend chart
        ])
        .split(area);

    render_expense_sparkline(frame, layout[0], state, theme);
    render_category_breakdown(frame, layout[1], state, theme);
    render_monthly_trend(frame, layout[2], state, theme);
}

fn render_net_worth_tab(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9), // Summary
            Constraint::Length(6), // Sparkline
            Constraint::Min(6),    // Trend
        ])
        .split(area);

    render_month_summary(frame, layout[0], state, theme);
    render_sparkline(frame, layout[1], state, theme);
    render_monthly_trend(frame, layout[2], state, theme);
}

fn render_stat_cards(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);

    let (income, expenses, _balance) = state
        .stats
        .data
        .as_ref()
        .map(|s| {
            (
                s.total_income_minor,
                s.total_expenses_minor,
                s.balance_minor,
            )
        })
        .unwrap_or((0, 0, 0));

    let net = income - expenses;

    // Calculate MoM changes
    let income_change = percentage_change(&state.stats.monthly_income);
    let expense_change = percentage_change(&state.stats.monthly_trend);
    let net_change = calculate_net_change(&state.stats.monthly_income, &state.stats.monthly_trend);

    // Split into 3 columns for the stat cards
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(area);

    // Income StatCard with MoM trend
    let income_formatted = Money::new(income).format(currency);
    let income_subtitle = format_mom_subtitle(income_change, true);
    StatCard::new("Total Income", income_formatted, theme)
        .subtitle(&income_subtitle)
        .render(frame, cols[0]);

    // Expenses StatCard with MoM trend
    let expenses_formatted = Money::new(-expenses).format(currency);
    let expense_subtitle = format_mom_subtitle(expense_change, false);
    StatCard::new("Total Expenses", expenses_formatted, theme)
        .subtitle(&expense_subtitle)
        .render(frame, cols[1]);

    // Net Balance StatCard with MoM trend
    let net_formatted = Money::new(net).format(currency);
    let net_subtitle = format_mom_subtitle(net_change, true);
    StatCard::new("Net Balance", net_formatted, theme)
        .subtitle(&net_subtitle)
        .render(frame, cols[2]);
}

/// Calculate net change from income and expense trends
fn calculate_net_change(income_trend: &[(String, i64)], expense_trend: &[(String, i64)]) -> Option<f64> {
    if income_trend.len() < 2 || expense_trend.len() < 2 {
        return None;
    }
    let prev_income = income_trend[income_trend.len() - 2].1;
    let curr_income = income_trend[income_trend.len() - 1].1;
    let prev_expense = expense_trend[expense_trend.len() - 2].1;
    let curr_expense = expense_trend[expense_trend.len() - 1].1;

    let prev_net = prev_income - prev_expense;
    let curr_net = curr_income - curr_expense;

    if prev_net == 0 {
        return None;
    }
    Some(((curr_net - prev_net) as f64 / prev_net.abs() as f64) * 100.0)
}

/// Format MoM subtitle with arrow indicator
fn format_mom_subtitle(change: Option<f64>, _positive_is_good: bool) -> String {
    match change {
        Some(pct) => {
            let arrow = if pct > 0.0 { "↑" } else if pct < 0.0 { "↓" } else { "→" };
            let sign = if pct > 0.0 { "+" } else { "" };
            format!("{arrow} {sign}{:.0}% MoM", pct)
        }
        None => "This month".to_string(),
    }
}

fn render_month_summary(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let (year, month) = state.stats.current_month;

    let card = Card::new("Month Summary", theme).focused(true);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let currency = get_currency(state);

    let (income, expenses, balance) = state
        .stats
        .data
        .as_ref()
        .map(|s| {
            (
                s.total_income_minor,
                s.total_expenses_minor,
                s.balance_minor,
            )
        })
        .unwrap_or((0, 0, 0));

    let net = income - expenses;

    // Layout: header with navigation, then stats
    let inner_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Navigation header
            Constraint::Length(1), // Inline trend + MoM
            Constraint::Min(0),    // Stats content
        ])
        .split(inner);

    // Month navigation timeline
    let nav_line = build_month_timeline(year, month, theme);
    frame.render_widget(Paragraph::new(nav_line), inner_layout[0]);

    let change_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(inner_layout[1]);

    if !state.stats.sparkline.is_empty() {
        render_inline_sparkline(frame, change_layout[0], &state.stats.sparkline, theme);
    } else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No trend data yet",
                Style::default().fg(theme.dim),
            )),
            change_layout[0],
        );
    }

    let income_change = percentage_change(&state.stats.monthly_income);
    let expense_change = percentage_change(&state.stats.monthly_trend);
    let change_line = Line::from(vec![
        Span::styled("MoM", Style::default().fg(theme.dim)),
        Span::raw(" "),
        Span::styled("Inc", Style::default().fg(theme.text_muted)),
        Span::raw(" "),
        income_change
            .map(|value| styled_percentage_change(value, theme))
            .unwrap_or_else(|| Span::styled("n/a", Style::default().fg(theme.dim))),
        Span::raw("  "),
        Span::styled("Exp", Style::default().fg(theme.text_muted)),
        Span::raw(" "),
        expense_change
            .map(|value| styled_percentage_change(value, theme))
            .unwrap_or_else(|| Span::styled("n/a", Style::default().fg(theme.dim))),
    ]);
    frame.render_widget(Paragraph::new(change_line), change_layout[1]);

    // Stats content
    let stats_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Income
            Constraint::Length(1), // Expenses
            Constraint::Length(1), // Gauge
            Constraint::Length(1), // Divider
            Constraint::Length(1), // Net
            Constraint::Length(1), // Total Balance
        ])
        .split(inner_layout[2]);

    // Income row with ASCII bar
    let income_pct = compute_percentage(income, income);
    render_stat_row(
        frame,
        stats_layout[0],
        StatRow {
            label: "Income",
            amount: income,
            percentage: income_pct,
            color: theme.positive,
        },
        currency,
        theme,
    );

    // Expenses row with ASCII bar (relative to income)
    let expense_pct = if income == 0 && expenses > 0 {
        100
    } else {
        compute_percentage(expenses, income)
    };
    render_stat_row(
        frame,
        stats_layout[1],
        StatRow {
            label: "Expenses",
            amount: -expenses,
            percentage: expense_pct,
            color: theme.negative,
        },
        currency,
        theme,
    );

    if let Some(gauge) = flow_cap_gauge(expenses, Some(income), "Expense/Income", theme) {
        frame.render_widget(gauge, stats_layout[2]);
    } else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No income to compare",
                Style::default().fg(theme.dim),
            )),
            stats_layout[2],
        );
    }

    // Divider
    let divider = "─".repeat(stats_layout[3].width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(divider, Style::default().fg(theme.border))),
        stats_layout[3],
    );

    // Net row
    let net_line = Line::from(vec![
        Span::styled("Net         ", Style::default().fg(theme.dim)),
        styled_amount_bold_emoji(net, currency, theme, state.emoji_mode),
    ]);
    frame.render_widget(Paragraph::new(net_line), stats_layout[4]);

    // Total Balance row
    let balance_line = Line::from(vec![
        Span::styled("Balance     ", Style::default().fg(theme.dim)),
        Span::styled(
            Money::new(balance).format(currency),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(balance_line), stats_layout[5]);

    // Show error if any
    if let Some(err) = state.stats.error.as_ref() {
        let error_area = Rect {
            y: inner.y + inner.height.saturating_sub(1),
            height: 1,
            ..inner
        };
        frame.render_widget(
            Paragraph::new(Span::styled(err.as_str(), Style::default().fg(theme.error))),
            error_area,
        );
    }
}

struct StatRow<'a> {
    label: &'a str,
    amount: i64,
    percentage: u16,
    color: ratatui::style::Color,
}

fn render_stat_row(
    frame: &mut Frame<'_>,
    area: Rect,
    row: StatRow<'_>,
    currency: Currency,
    theme: &Theme,
) {
    // Split: label, amount, bar
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(12), // Label
            Constraint::Length(16), // Amount
            Constraint::Min(10),    // Bar
        ])
        .split(area);

    // Label
    frame.render_widget(
        Paragraph::new(Span::styled(row.label, Style::default().fg(theme.dim))),
        cols[0],
    );

    // Amount
    frame.render_widget(
        Paragraph::new(styled_amount_no_sign(row.amount, currency, theme)),
        cols[1],
    );

    let bar_width = cols[2].width.saturating_sub(4).max(1) as usize;
    let bar = percentage_bar(row.percentage, bar_width);
    frame.render_widget(
        Paragraph::new(Span::styled(bar, Style::default().fg(row.color))),
        cols[2],
    );
}

fn render_category_breakdown(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let breakdown = &state.stats.category_breakdown;

    if breakdown.is_empty() {
        let card = Card::new("Category Breakdown", theme);
        let inner = card.inner(area);
        card.render_frame(frame, area);
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No expense data for category breakdown",
                Style::default().fg(theme.dim),
            ))
            .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    // Split area: pie chart on the left, list on the right
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(30)])
        .split(area);

    // Render pie chart on the left
    render_category_pie_chart(frame, cols[0], breakdown, theme);

    // Render category list on the right
    render_category_list(frame, cols[1], state, theme);
}

fn render_category_pie_chart(
    frame: &mut Frame<'_>,
    area: Rect,
    breakdown: &[(String, i64)],
    theme: &Theme,
) {
    // Define colors for pie slices (cycling through theme-compatible colors)
    let colors = [
        theme.negative,   // Red for largest
        theme.warning,    // Orange/Yellow
        theme.accent,     // Blue/Accent
        theme.positive,   // Green
        theme.text_muted, // Muted
        theme.dim,        // Dim for smaller slices
    ];

    let slices: Vec<PieSlice> = breakdown
        .iter()
        .enumerate()
        .map(|(i, (_, amount))| PieSlice {
            value: amount.unsigned_abs(),
            color: colors[i % colors.len()],
        })
        .collect();

    render_pie_chart(frame, area, "Distribution", &slices, theme);
}

fn render_category_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let card = Card::new("Category Breakdown", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let currency = get_currency(state);
    let breakdown = &state.stats.category_breakdown;
    let total: i64 = breakdown.iter().map(|(_, v)| *v).sum();

    let rows: Vec<Line> = breakdown
        .iter()
        .enumerate()
        .take(inner.height as usize)
        .map(|(idx, (category, amount))| {
            let pct = compute_percentage(*amount, total);
            let style = if pct >= 75 {
                BarStyle::Block
            } else if pct >= 25 {
                BarStyle::Line
            } else {
                BarStyle::Dot
            };
            let bar = ascii_bar_styled(
                amount.saturating_abs() as u64,
                total.saturating_abs() as u64,
                15,
                style,
            );

            // Get category icon based on name pattern
            let icon = get_category_icon(category);

            // Alert indicator for high spending categories
            let alert = if pct >= 40 {
                Span::styled(" ⚠️", Style::default().fg(theme.warning))
            } else {
                Span::raw("  ")
            };

            Line::from(vec![
                Span::styled(
                    format!("{}. ", idx + 1),
                    Style::default().fg(theme.dim),
                ),
                Span::styled(
                    format!("{icon} "),
                    Style::default().fg(theme.text_muted),
                ),
                Span::styled(
                    format!("{:<14}", truncate_string(category, 13)),
                    Style::default().fg(theme.text),
                ),
                Span::styled(
                    format!("{:>10}", Money::new(*amount).format(currency)),
                    Style::default().fg(theme.negative),
                ),
                Span::raw(" "),
                Span::styled(bar, Style::default().fg(theme.negative)),
                Span::styled(format!(" {:>2}%", pct), Style::default().fg(theme.dim)),
                alert,
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(rows), inner);
}

/// Get an emoji icon for a category based on common patterns
fn get_category_icon(category: &str) -> &'static str {
    let lower = category.to_lowercase();
    if lower.contains("food") || lower.contains("grocer") || lower.contains("restaurant") || lower.contains("cibo") {
        "🍽️"
    } else if lower.contains("house") || lower.contains("rent") || lower.contains("home") || lower.contains("casa") {
        "🏠"
    } else if lower.contains("transport") || lower.contains("car") || lower.contains("gas") || lower.contains("auto") {
        "🚗"
    } else if lower.contains("health") || lower.contains("medical") || lower.contains("salute") {
        "🏥"
    } else if lower.contains("entertain") || lower.contains("fun") || lower.contains("svago") {
        "🎬"
    } else if lower.contains("shop") || lower.contains("cloth") || lower.contains("acquist") {
        "🛍️"
    } else if lower.contains("bill") || lower.contains("utilit") || lower.contains("bolletta") {
        "💡"
    } else if lower.contains("subscri") || lower.contains("abbonament") {
        "📱"
    } else if lower.contains("travel") || lower.contains("viaggio") {
        "✈️"
    } else if lower.contains("educat") || lower.contains("school") || lower.contains("scuola") {
        "📚"
    } else {
        "📁"
    }
}

fn render_monthly_trend(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let expense_trend = &state.stats.monthly_trend;
    let income_trend = &state.stats.monthly_income;

    if expense_trend.is_empty() && income_trend.is_empty() {
        let card = Card::new("Monthly Trend", theme);
        let inner = card.inner(area);
        card.render_frame(frame, area);
        frame.render_widget(
            Paragraph::new(Span::styled(
                "Monthly trend data not available. Press 'r' to refresh stats.",
                Style::default().fg(theme.dim),
            ))
            .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    // Consolidated trend view with status badges
    let card = Card::new("Financial Trends (6 months)", theme).focused(true);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let currency = get_currency(state);

    // Calculate trends and status
    let income_change = percentage_change(income_trend);
    let expense_change = percentage_change(expense_trend);
    let net_change = calculate_net_change(income_trend, expense_trend);

    // Get current values
    let current_income = income_trend.last().map(|(_, v)| *v).unwrap_or(0);
    let current_expense = expense_trend.last().map(|(_, v)| *v).unwrap_or(0);
    let current_net = current_income - current_expense;

    let inner_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Income row
            Constraint::Length(1), // Expense row
            Constraint::Length(1), // Net savings row
            Constraint::Min(0),    // Charts
        ])
        .split(inner);

    // Income trend row
    let income_status = trend_status_badge(income_change, true, theme);
    let income_line = build_trend_line(
        "Income",
        income_change,
        current_income,
        income_status,
        income_trend,
        currency,
        theme,
    );
    frame.render_widget(Paragraph::new(income_line), inner_layout[0]);

    // Expense trend row
    let expense_status = trend_status_badge(expense_change, false, theme);
    let expense_line = build_trend_line(
        "Expenses",
        expense_change,
        -current_expense,
        expense_status,
        expense_trend,
        currency,
        theme,
    );
    frame.render_widget(Paragraph::new(expense_line), inner_layout[1]);

    // Net savings row
    let net_status = trend_status_badge(net_change, true, theme);
    let net_sparkline: Vec<(String, i64)> = income_trend
        .iter()
        .zip(expense_trend.iter())
        .map(|((label, inc), (_, exp))| (label.clone(), inc - exp))
        .collect();
    let net_line = build_trend_line(
        "Net Savings",
        net_change,
        current_net,
        net_status,
        &net_sparkline,
        currency,
        theme,
    );
    frame.render_widget(Paragraph::new(net_line), inner_layout[2]);

    // Render mini bar charts side by side
    let chart_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner_layout[3]);

    let income_data: Vec<(&str, u64)> = income_trend
        .iter()
        .map(|(label, value)| (label.as_str(), (*value).max(0) as u64))
        .collect();
    let expense_data: Vec<(&str, u64)> = expense_trend
        .iter()
        .map(|(label, value)| (label.as_str(), (*value).max(0) as u64))
        .collect();

    if !income_data.is_empty() {
        render_bar_chart(frame, chart_layout[0], "Income", &income_data, theme);
    }
    if !expense_data.is_empty() {
        render_bar_chart(frame, chart_layout[1], "Expenses", &expense_data, theme);
    }
}

/// Build a consolidated trend line with sparkline, amount, and status
fn build_trend_line<'a>(
    label: &str,
    change: Option<f64>,
    amount: i64,
    status: (&'static str, ratatui::style::Color),
    _data: &[(String, i64)],
    currency: Currency,
    theme: &Theme,
) -> Line<'a> {
    let (status_label, status_color) = status;
    let arrow = match change {
        Some(pct) if pct > 5.0 => "↑",
        Some(pct) if pct < -5.0 => "↓",
        _ => "→",
    };
    let change_str = change
        .map(|pct| format!("{:+.0}%", pct))
        .unwrap_or_else(|| "n/a".to_string());

    let amount_color = if amount >= 0 { theme.positive } else { theme.negative };

    Line::from(vec![
        Span::styled(format!("{label:<12}"), Style::default().fg(theme.text)),
        Span::styled(format!("({arrow} {change_str}) "), Style::default().fg(theme.text_muted)),
        Span::styled(
            format!("{:>12}", Money::new(amount).format(currency)),
            Style::default().fg(amount_color),
        ),
        Span::raw("  "),
        Span::styled(format!("Status: {status_label}"), Style::default().fg(status_color)),
    ])
}

/// Get status badge based on trend
fn trend_status_badge(change: Option<f64>, positive_is_good: bool, theme: &Theme) -> (&'static str, ratatui::style::Color) {
    match change {
        Some(pct) if pct > 10.0 => {
            if positive_is_good {
                ("EXCELLENT", theme.positive)
            } else {
                ("CAUTION", theme.warning)
            }
        }
        Some(pct) if pct > 0.0 => {
            if positive_is_good {
                ("GOOD", theme.positive)
            } else {
                ("RISING", theme.warning)
            }
        }
        Some(pct) if pct > -10.0 => ("STABLE", theme.text_muted),
        Some(_) => {
            if positive_is_good {
                ("DECLINING", theme.warning)
            } else {
                ("GOOD", theme.positive)
            }
        }
        None => ("N/A", theme.dim),
    }
}

fn render_sparkline(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let card = Card::new("Balance Trend (30d)", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    if state.stats.sparkline.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No data. Press 'r' to refresh stats.",
                Style::default().fg(theme.dim),
            ))
            .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    render_braille_sparkline(frame, inner, &state.stats.sparkline, theme, true);
}

fn render_expense_sparkline(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let card = Card::new("Expense Trend (6m)", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    // Convert monthly trend data (last 6 months) to sparkline format
    let expense_data: Vec<u64> = state
        .stats
        .monthly_trend
        .iter()
        .map(|(_, value)| (*value).max(0) as u64)
        .collect();

    if expense_data.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No expense trend data. Press 'r' to refresh.",
                Style::default().fg(theme.dim),
            ))
            .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    render_braille_sparkline(frame, inner, &expense_data, theme, true);
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

fn percentage_change(series: &[(String, i64)]) -> Option<f64> {
    if series.len() < 2 {
        return None;
    }
    let (_, prev) = series[series.len() - 2];
    let (_, current) = series[series.len() - 1];
    if prev == 0 {
        return None;
    }
    Some(((current - prev) as f64 / prev.abs() as f64) * 100.0)
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "Unknown",
    }
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}

/// Build a visual timeline showing recent months with the current one highlighted
fn build_month_timeline<'a>(year: i32, month: u32, theme: &Theme) -> Line<'a> {
    let mut spans = Vec::new();

    // Show 5 months: 2 before, current, 2 after
    let months_to_show = [
        offset_month(year, month, -2),
        offset_month(year, month, -1),
        (year, month),
        offset_month(year, month, 1),
        offset_month(year, month, 2),
    ];

    spans.push(Span::styled("Month: ", Style::default().fg(theme.text_muted)));
    spans.push(Span::styled(
        format!("{} {}", month_name(month), year),
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw("  "));

    // Navigation hint
    spans.push(Span::styled("[◀", Style::default().fg(theme.accent)));

    for (i, (y, m)) in months_to_show.iter().enumerate() {
        let short_name = month_short_name(*m);
        if i > 0 {
            spans.push(Span::raw("  "));
        }

        if (*y, *m) == (year, month) {
            // Current month - highlighted
            spans.push(Span::styled(
                format!("[{short_name}]*"),
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                format!("[{short_name}]"),
                Style::default().fg(theme.dim),
            ));
        }
    }

    spans.push(Span::styled("▶]", Style::default().fg(theme.accent)));

    Line::from(spans)
}

/// Get short month name (3 letters)
fn month_short_name(month: u32) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    }
}

/// Calculate year/month with offset
fn offset_month(year: i32, month: u32, offset: i32) -> (i32, u32) {
    let total_months = (year * 12) as i32 + (month as i32 - 1) + offset;
    let new_year = total_months / 12;
    let new_month = (total_months % 12 + 12) % 12 + 1;
    (new_year, new_month as u32)
}
