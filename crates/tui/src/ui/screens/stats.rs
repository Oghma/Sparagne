use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{BarChart, Block, BorderType, Borders, Gauge, Paragraph, Sparkline},
};

use engine::{Currency, Money};

use crate::{
    app::AppState,
    ui::{components::money::styled_amount_bold, theme::Theme},
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();

    // Show error state if stats loading failed
    if let Some(error) = &state.stats.error {
        let block = Block::default()
            .title(" Stats ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border));
        let inner = block.inner(area);
        frame.render_widget(block, area);

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
        let block = Block::default()
            .title(" Stats ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border));
        let inner = block.inner(area);
        frame.render_widget(block, area);

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

    let block = Block::default()
        .title(Span::styled(
            " Month Summary ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(area);
    frame.render_widget(block, area);

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
            Constraint::Length(1), // Spacer
            Constraint::Min(0),    // Stats content
        ])
        .split(inner);

    // Month navigation header
    let nav_line = Line::from(vec![
        Span::styled(
            format!("{month_name} {year}"),
            Style::default()
                .fg(theme.text)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("[Current]", Style::default().fg(theme.dim)),
    ]);
    frame.render_widget(Paragraph::new(nav_line), inner_layout[0]);

    // Stats content
    let stats_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Income
            Constraint::Length(1), // Expenses
            Constraint::Length(1), // Divider
            Constraint::Length(1), // Net
            Constraint::Length(1), // Total Balance
        ])
        .split(inner_layout[2]);

    // Income row with gauge
    let income_pct = if income > 0 { 100 } else { 0 };
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

    // Expenses row with gauge (relative to income)
    let expense_pct = if income > 0 {
        ((expenses as f64 / income as f64) * 100.0).min(100.0) as u16
    } else if expenses > 0 {
        100
    } else {
        0
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

    // Divider
    let divider = "─".repeat(stats_layout[2].width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(divider, Style::default().fg(theme.border))),
        stats_layout[2],
    );

    // Net row
    let net_line = Line::from(vec![
        Span::styled("Net         ", Style::default().fg(theme.dim)),
        styled_amount_bold(net, currency, theme),
    ]);
    frame.render_widget(Paragraph::new(net_line), stats_layout[3]);

    // Total Balance row
    let balance_line = Line::from(vec![
        Span::styled("Balance     ", Style::default().fg(theme.dim)),
        Span::styled(
            Money::new(balance).format(currency),
            Style::default()
                .fg(theme.text)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(balance_line), stats_layout[4]);

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
    // Split: label, amount, gauge
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(12), // Label
            Constraint::Length(16), // Amount
            Constraint::Min(10),    // Gauge
            Constraint::Length(5),  // Percentage
        ])
        .split(area);

    // Label
    frame.render_widget(
        Paragraph::new(Span::styled(label, Style::default().fg(theme.dim))),
        cols[0],
    );

    // Amount
    let sign = if amount > 0 { "+" } else { "" };
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("{sign}{}", Money::new(amount.abs()).format(currency)),
            Style::default().fg(color),
        )),
        cols[1],
    );

    // Gauge (simple bar)
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(color))
        .percent(percentage)
        .label("");
    frame.render_widget(gauge, cols[2]);

    // Percentage
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("{percentage:>3}%"),
            Style::default().fg(theme.dim),
        ))
        .alignment(Alignment::Right),
        cols[3],
    );
}

fn render_category_breakdown(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let block = Block::default()
        .title(Span::styled(
            " Category Breakdown ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(area);
    frame.render_widget(block, area);

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
            let pct = if total > 0 {
                (*amount as f64 / total as f64 * 100.0) as u16
            } else {
                0
            };

            let bar_width = 20;
            let filled = ((pct as usize * bar_width) / 100).min(bar_width);
            let empty = bar_width.saturating_sub(filled);
            let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

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
    let block = Block::default()
        .title(Span::styled(
            " Monthly Trend ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let expense_trend = &state.stats.monthly_trend;
    let income_trend = &state.stats.monthly_income;

    if expense_trend.is_empty() && income_trend.is_empty() {
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
        .constraints([Constraint::Length(6), Constraint::Min(0)])
        .split(inner);

    let income_data: Vec<(&str, u64)> = income_trend
        .iter()
        .map(|(label, value)| (label.as_str(), (*value).max(0) as u64))
        .collect();
    let income_chart = BarChart::default()
        .data(&income_data)
        .bar_width(5)
        .bar_gap(2)
        .bar_style(Style::default().fg(theme.positive))
        .value_style(Style::default().fg(theme.dim).add_modifier(Modifier::BOLD))
        .label_style(Style::default().fg(theme.dim))
        .max(income_data.iter().map(|(_, v)| *v).max().unwrap_or(1));
    frame.render_widget(income_chart, layout[0]);

    let expense_data: Vec<(&str, u64)> = expense_trend
        .iter()
        .map(|(label, value)| (label.as_str(), (*value).max(0) as u64))
        .collect();
    let expense_chart = BarChart::default()
        .data(&expense_data)
        .bar_width(5)
        .bar_gap(2)
        .bar_style(Style::default().fg(theme.negative))
        .value_style(Style::default().fg(theme.dim).add_modifier(Modifier::BOLD))
        .label_style(Style::default().fg(theme.dim))
        .max(expense_data.iter().map(|(_, v)| *v).max().unwrap_or(1));
    frame.render_widget(expense_chart, layout[1]);
}

fn render_sparkline(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let block = Block::default()
        .title(Span::styled(
            " Balance Trend (30d) ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));
    let inner = block.inner(area);
    frame.render_widget(block, area);

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

    let sparkline = Sparkline::default()
        .data(&state.stats.sparkline)
        .style(Style::default().fg(theme.accent));
    frame.render_widget(sparkline, inner);
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
