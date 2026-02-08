//! Cash flow tab rendering: stat cards, month summary, sparklines, category
//! breakdown, trends.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use engine::Money;

use crate::{
    app::AppState,
    text::{TextKey, t},
    ui::{
        components::{
            card::Card,
            charts::{
                PieSlice, compute_percentage, render_bar_chart, render_braille_sparkline,
                render_pie_chart,
            },
            money::{flow_cap_gauge, styled_amount_bold_emoji, styled_percentage_change},
        },
        theme::Theme,
    },
};

use super::components::{
    StatRow, build_month_timeline, build_trend_line, calculate_net_change, get_currency,
    percentage_change, render_category_list, render_stat_cards, render_stat_row,
    trend_status_badge,
};

/// Renders an optional percentage change, falling back to "N/A" when absent.
fn change_span(
    change: Option<f64>,
    locale: crate::text::Locale,
    theme: &Theme,
) -> Span<'static> {
    change
        .map(|value| styled_percentage_change(value, theme))
        .unwrap_or_else(|| {
            Span::styled(
                t(locale, TextKey::StatsNa),
                Style::default().fg(theme.text_muted),
            )
        })
}

/// Render the cash flow tab.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
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

/// Render the month summary card with navigation, stats, and gauges.
pub fn render_month_summary(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let (year, month) = state.stats.current_month;
    let locale = state.locale;

    let card = Card::new(t(locale, TextKey::StatsMonthSummary), theme).focused(true);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let currency = get_currency(state);

    let income = state.stats.current_month_income;
    let expenses = state.stats.current_month_expenses;
    let balance = state
        .stats
        .data
        .as_ref()
        .map(|s| s.balance_minor)
        .unwrap_or(0);
    let net = income - expenses;

    // Layout: header with navigation, then MoM, then stats
    let inner_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Navigation header
            Constraint::Length(1), // MoM change line (full width)
            Constraint::Min(0),    // Stats content
        ])
        .split(inner);

    // Month navigation timeline
    let nav_line = build_month_timeline(year, month, theme);
    frame.render_widget(Paragraph::new(nav_line), inner_layout[0]);

    // MoM change line at full width (no inline sparkline)
    let income_change = percentage_change(&state.stats.monthly_income);
    let expense_change = percentage_change(&state.stats.monthly_trend);
    let muted = |key| Span::styled(t(locale, key), Style::default().fg(theme.text_muted));
    let change_line = Line::from(vec![
        muted(TextKey::StatsMoM),
        Span::raw(" "),
        muted(TextKey::StatsInc),
        Span::raw(" "),
        change_span(income_change, locale, theme),
        Span::raw("  "),
        muted(TextKey::StatsExp),
        Span::raw(" "),
        change_span(expense_change, locale, theme),
    ]);
    frame.render_widget(Paragraph::new(change_line), inner_layout[1]);

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
            label: t(locale, TextKey::StatsIncome),
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
            label: t(locale, TextKey::StatsExpenses),
            amount: -expenses,
            percentage: expense_pct,
            color: theme.negative,
        },
        currency,
        theme,
    );

    if let Some(gauge) = flow_cap_gauge(
        expenses,
        Some(income),
        t(locale, TextKey::StatsExpenseOverIncome),
        theme,
    ) {
        frame.render_widget(gauge, stats_layout[2]);
    } else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                t(locale, TextKey::StatsNoIncomeToCompare),
                Style::default().fg(theme.text_muted),
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
    let net_label = format!("{:<12}", t(locale, TextKey::StatsNet));
    let net_line = Line::from(vec![
        Span::styled(net_label, Style::default().fg(theme.text_muted)),
        styled_amount_bold_emoji(net, currency, theme, state.emoji_mode),
    ]);
    frame.render_widget(Paragraph::new(net_line), stats_layout[4]);

    // Total Balance row (all-time vault balance)
    let balance_label = format!("{:<12}", t(locale, TextKey::StatsBalance));
    let balance_line = Line::from(vec![
        Span::styled(balance_label, Style::default().fg(theme.text_muted)),
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
            Paragraph::new(Span::styled(
                err.as_str(),
                Style::default().fg(theme.negative),
            )),
            error_area,
        );
    }
}

/// Render the category breakdown with pie chart and list.
pub fn render_category_breakdown(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    theme: &Theme,
) {
    let locale = state.locale;
    let breakdown = &state.stats.category_breakdown;

    if breakdown.is_empty() {
        let card = Card::new(t(locale, TextKey::StatsCategoryBreakdown), theme);
        let inner = card.inner(area);
        card.render_frame(frame, area);
        frame.render_widget(
            Paragraph::new(Span::styled(
                t(locale, TextKey::StatsNoCategoryData),
                Style::default().fg(theme.text_muted),
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
    render_category_pie_chart(frame, cols[0], breakdown, locale, theme);

    // Render category list on the right
    render_category_list(frame, cols[1], state, theme);
}

/// Render a pie chart for category distribution.
fn render_category_pie_chart(
    frame: &mut Frame<'_>,
    area: Rect,
    breakdown: &[(String, i64)],
    locale: crate::text::Locale,
    theme: &Theme,
) {
    // Define colors for pie slices (cycling through theme-compatible colors)
    let colors = [
        theme.negative,   // Red for largest
        theme.warning,    // Orange/Yellow
        theme.accent,     // Blue/Accent
        theme.positive,   // Green
        theme.text_muted, // Muted
        theme.text_muted, // Dim for smaller slices
    ];

    let slices: Vec<PieSlice> = breakdown
        .iter()
        .enumerate()
        .map(|(i, (_, amount))| PieSlice {
            value: amount.unsigned_abs(),
            color: colors[i % colors.len()],
        })
        .collect();

    render_pie_chart(
        frame,
        area,
        t(locale, TextKey::StatsDistribution),
        &slices,
        theme,
    );
}

/// Render the balance sparkline.
pub fn render_sparkline(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let locale = state.locale;
    let card = Card::new(t(locale, TextKey::StatsBalanceTrend), theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    if state.stats.sparkline.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(super::refresh_hint_spans(locale, theme)))
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let currency = get_currency(state);
    let range_label = format!(
        "{}–{}",
        Money::new(state.stats.sparkline_min).format(currency),
        Money::new(state.stats.sparkline_max).format(currency),
    );
    render_braille_sparkline(
        frame,
        inner,
        &state.stats.sparkline,
        theme,
        Some(&range_label),
    );
}

/// Render the monthly trend charts with income/expense comparison.
pub fn render_monthly_trend(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let locale = state.locale;
    let expense_trend = &state.stats.monthly_trend;
    let income_trend = &state.stats.monthly_income;

    if expense_trend.is_empty() && income_trend.is_empty() {
        let card = Card::new(t(locale, TextKey::StatsMonthlyTrend), theme);
        let inner = card.inner(area);
        card.render_frame(frame, area);
        frame.render_widget(
            Paragraph::new(Span::styled(
                t(locale, TextKey::StatsMonthlyTrendNoData),
                Style::default().fg(theme.text_muted),
            ))
            .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    // Consolidated trend view with status badges
    let card = Card::new(t(locale, TextKey::StatsFinancialTrends), theme).focused(true);
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
        t(locale, TextKey::StatsIncome),
        income_change,
        current_income,
        income_status,
        currency,
        theme,
    );
    frame.render_widget(Paragraph::new(income_line), inner_layout[0]);

    // Expense trend row
    let expense_status = trend_status_badge(expense_change, false, theme);
    let expense_line = build_trend_line(
        t(locale, TextKey::StatsExpenses),
        expense_change,
        -current_expense,
        expense_status,
        currency,
        theme,
    );
    frame.render_widget(Paragraph::new(expense_line), inner_layout[1]);

    // Net savings row
    let net_status = trend_status_badge(net_change, true, theme);
    let net_line = build_trend_line(
        t(locale, TextKey::StatsNetSavings),
        net_change,
        current_net,
        net_status,
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
        render_bar_chart(
            frame,
            chart_layout[0],
            t(locale, TextKey::StatsIncome),
            &income_data,
            theme.positive,
            theme,
        );
    }
    if !expense_data.is_empty() {
        render_bar_chart(
            frame,
            chart_layout[1],
            t(locale, TextKey::StatsExpenses),
            &expense_data,
            theme.negative,
            theme,
        );
    }
}
