use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use engine::{Currency, Money};

use crate::{
    app::AppState,
    ui::{
        components::{
            card::Card,
            charts::{
                BarStyle, ascii_bar_styled, compute_percentage, percentage_bar, render_bar_chart,
                render_inline_sparkline, render_sparkline as render_sparkline_card,
            },
            money::{
                flow_cap_gauge, styled_amount_bold, styled_amount_no_sign, styled_percentage_change,
            },
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

    // Main layout: Month summary, Sparkline, Category breakdown, Monthly trend
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9),  // Month summary with navigation
            Constraint::Length(6),  // Sparkline
            Constraint::Length(12), // Category breakdown
            Constraint::Min(6),     // Monthly trend chart
        ])
        .split(area);

    render_month_summary(frame, layout[0], state, &theme);
    render_sparkline(frame, layout[1], state, &theme);
    render_category_breakdown(frame, layout[2], state, &theme);
    render_monthly_trend(frame, layout[3], state, &theme);
}

fn render_month_summary(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let (year, month) = state.stats.current_month;
    let month_name = month_name(month);

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

    // Month navigation header
    let nav_line = Line::from(vec![
        Span::styled(
            format!("{month_name} {year}"),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("[Current]", Style::default().fg(theme.dim)),
    ]);
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
        "Income",
        income,
        income_pct,
        theme.positive,
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
        "Expenses",
        -expenses,
        expense_pct,
        theme.negative,
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
        styled_amount_bold(net, currency, theme),
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

fn render_stat_row(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    amount: i64,
    percentage: u16,
    color: ratatui::style::Color,
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
        Paragraph::new(Span::styled(label, Style::default().fg(theme.dim))),
        cols[0],
    );

    // Amount
    frame.render_widget(
        Paragraph::new(styled_amount_no_sign(amount, currency, theme)),
        cols[1],
    );

    let bar_width = cols[2].width.saturating_sub(4).max(1) as usize;
    let bar = percentage_bar(percentage, bar_width);
    frame.render_widget(
        Paragraph::new(Span::styled(bar, Style::default().fg(color))),
        cols[2],
    );
}

fn render_category_breakdown(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let card = Card::new("Category Breakdown", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let currency = get_currency(state);

    let breakdown = &state.stats.category_breakdown;

    if breakdown.is_empty() {
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

    let total: i64 = breakdown.iter().map(|(_, v)| *v).sum();

    let rows: Vec<Line> = breakdown
        .iter()
        .take(inner.height as usize)
        .map(|(category, amount)| {
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
                20,
                style,
            );

            Line::from(vec![
                Span::styled(
                    format!("{:<16}", truncate_string(category, 15)),
                    Style::default().fg(theme.text),
                ),
                Span::styled(
                    format!("{:>12}", Money::new(*amount).format(currency)),
                    Style::default().fg(theme.negative),
                ),
                Span::raw("  "),
                Span::styled(bar, Style::default().fg(theme.negative)),
                Span::styled(format!(" {pct:>3}%"), Style::default().fg(theme.dim)),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(rows), inner);
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

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let income_data: Vec<(&str, u64)> = income_trend
        .iter()
        .map(|(label, value)| (label.as_str(), (*value).max(0) as u64))
        .collect();
    let income_color = if income_data.is_empty() {
        theme.dim
    } else if income_data.last().map(|(_, v)| *v).unwrap_or(0)
        >= income_data.first().map(|(_, v)| *v).unwrap_or(0)
    {
        theme.positive
    } else {
        theme.warning
    };
    let income_title = Line::from(vec![
        Span::styled("Income (6m)", Style::default().fg(theme.text_muted)),
        Span::raw(" "),
        Span::styled(
            if income_color == theme.positive {
                "●"
            } else {
                "○"
            },
            Style::default().fg(income_color),
        ),
    ]);
    if income_data.is_empty() {
        let card = Card::new("Income (6m)", theme);
        let inner = card.inner(layout[0]);
        card.render_frame(frame, layout[0]);
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No income data yet.",
                Style::default().fg(theme.dim),
            ))
            .alignment(Alignment::Center),
            inner,
        );
    } else {
        let card = Card::new("Income (6m)", theme).focused(true);
        let inner = card.inner(layout[0]);
        card.render_frame(frame, layout[0]);
        frame.render_widget(Paragraph::new(income_title), Rect { height: 1, ..inner });
        let chart_area = Rect {
            y: inner.y + 1,
            height: inner.height.saturating_sub(1),
            ..inner
        };
        render_bar_chart(frame, chart_area, "", &income_data, theme);
    }

    let expense_data: Vec<(&str, u64)> = expense_trend
        .iter()
        .map(|(label, value)| (label.as_str(), (*value).max(0) as u64))
        .collect();
    let expense_color = if expense_data.is_empty() {
        theme.dim
    } else if expense_data.last().map(|(_, v)| *v).unwrap_or(0)
        >= expense_data.first().map(|(_, v)| *v).unwrap_or(0)
    {
        theme.warning
    } else {
        theme.positive
    };
    let expense_title = Line::from(vec![
        Span::styled("Expenses (6m)", Style::default().fg(theme.text_muted)),
        Span::raw(" "),
        Span::styled(
            if expense_color == theme.warning {
                "●"
            } else {
                "○"
            },
            Style::default().fg(expense_color),
        ),
    ]);
    if expense_data.is_empty() {
        let card = Card::new("Expenses (6m)", theme);
        let inner = card.inner(layout[1]);
        card.render_frame(frame, layout[1]);
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No expense data yet.",
                Style::default().fg(theme.dim),
            ))
            .alignment(Alignment::Center),
            inner,
        );
    } else {
        let card = Card::new("Expenses (6m)", theme).focused(true);
        let inner = card.inner(layout[1]);
        card.render_frame(frame, layout[1]);
        frame.render_widget(Paragraph::new(expense_title), Rect { height: 1, ..inner });
        let chart_area = Rect {
            y: inner.y + 1,
            height: inner.height.saturating_sub(1),
            ..inner
        };
        render_bar_chart(frame, chart_area, "", &expense_data, theme);
    }
}

fn render_sparkline(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    if state.stats.sparkline.is_empty() {
        let card = Card::new("Balance Trend (30d)", theme);
        let inner = card.inner(area);
        card.render_frame(frame, area);
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

    render_sparkline_card(
        frame,
        area,
        "Balance Trend (30d)",
        &state.stats.sparkline,
        theme,
    );
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
