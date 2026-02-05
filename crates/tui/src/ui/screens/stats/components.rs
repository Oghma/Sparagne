//! Shared components for stats rendering: stat cards, stat rows, helpers.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use engine::{Currency, Money};

use crate::{
    app::AppState,
    ui::{
        common::truncate,
        components::{
            card::StatCard,
            charts::{BarStyle, ascii_bar_styled, compute_percentage, percentage_bar},
        },
        theme::Theme,
    },
};

/// Data for a single stat row with label, amount, percentage, and color.
pub struct StatRow<'a> {
    pub label: &'a str,
    pub amount: i64,
    pub percentage: u16,
    pub color: ratatui::style::Color,
}

/// Render a stat row with label, amount, and percentage bar.
pub fn render_stat_row(
    frame: &mut Frame<'_>,
    area: Rect,
    row: StatRow<'_>,
    currency: Currency,
    theme: &Theme,
) {
    use crate::ui::components::money::styled_amount_no_sign;

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

/// Render the three main stat cards (Income, Expenses, Net Balance).
pub fn render_stat_cards(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
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

/// Render the category list with amounts and bars.
pub fn render_category_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    use crate::ui::components::card::Card;

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
                    format!("{:<14}", truncate(category, 13)),
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

/// Get an emoji icon for a category based on common patterns.
pub fn get_category_icon(category: &str) -> &'static str {
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

// `get_currency` is provided by `crate::ui::common`.
pub(crate) use crate::ui::common::get_currency;

/// Calculate percentage change between last two values in a series.
pub fn percentage_change(series: &[(String, i64)]) -> Option<f64> {
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

/// Calculate net change from income and expense trends.
pub fn calculate_net_change(income_trend: &[(String, i64)], expense_trend: &[(String, i64)]) -> Option<f64> {
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

/// Format MoM subtitle with arrow indicator.
pub fn format_mom_subtitle(change: Option<f64>, _positive_is_good: bool) -> String {
    match change {
        Some(pct) => {
            let arrow = if pct > 0.0 { "↑" } else if pct < 0.0 { "↓" } else { "→" };
            let sign = if pct > 0.0 { "+" } else { "" };
            format!("{arrow} {sign}{:.0}% MoM", pct)
        }
        None => "This month".to_string(),
    }
}

/// Get full month name.
pub fn month_name(month: u32) -> &'static str {
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

/// Get short month name (3 letters).
pub fn month_short_name(month: u32) -> &'static str {
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

/// Calculate year/month with offset.
pub fn offset_month(year: i32, month: u32, offset: i32) -> (i32, u32) {
    let total_months = (year * 12) as i32 + (month as i32 - 1) + offset;
    let new_year = total_months / 12;
    let new_month = (total_months % 12 + 12) % 12 + 1;
    (new_year, new_month as u32)
}


/// Build a visual timeline showing recent months with the current one highlighted.
pub fn build_month_timeline<'a>(year: i32, month: u32, theme: &Theme) -> Line<'a> {
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

/// Get status badge based on trend.
pub fn trend_status_badge(change: Option<f64>, positive_is_good: bool, theme: &Theme) -> (&'static str, ratatui::style::Color) {
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

/// Build a consolidated trend line with sparkline, amount, and status.
pub fn build_trend_line<'a>(
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
