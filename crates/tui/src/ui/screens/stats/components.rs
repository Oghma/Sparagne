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
    text::{TextKey, t},
    ui::{
        common::truncate,
        components::{
            card::StatCard,
            charts::{ascii_bar, compute_percentage},
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
        Paragraph::new(Span::styled(
            row.label,
            Style::default().fg(theme.text_muted),
        )),
        cols[0],
    );

    // Amount
    frame.render_widget(
        Paragraph::new(styled_amount_no_sign(row.amount, currency, theme)),
        cols[1],
    );

    let bar_width = cols[2].width.saturating_sub(8).max(1) as usize;
    let bar = ascii_bar(row.percentage as u64, 100, bar_width);
    let label = format!(" {:>3}%", row.percentage);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(bar, Style::default().fg(row.color)),
            Span::styled(label, Style::default().fg(theme.text_muted)),
        ])),
        cols[2],
    );
}

/// Render the three main stat cards (Income, Expenses, Net Balance).
///
/// Values reflect the currently selected month, not all-time totals.
pub fn render_stat_cards(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);

    let income = state.stats.current_month_income;
    let expenses = state.stats.current_month_expenses;
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

    let locale = state.locale;

    // Income StatCard with MoM trend
    let income_formatted = Money::new(income).format(currency);
    let income_subtitle = format_mom_subtitle(income_change, locale);
    StatCard::new(
        t(locale, TextKey::StatsTotalIncome),
        income_formatted,
        theme,
    )
    .subtitle(&income_subtitle)
    .render(frame, cols[0]);

    // Expenses StatCard with MoM trend
    let expenses_formatted = Money::new(-expenses).format(currency);
    let expense_subtitle = format_mom_subtitle(expense_change, locale);
    StatCard::new(
        t(locale, TextKey::StatsTotalExpenses),
        expenses_formatted,
        theme,
    )
    .subtitle(&expense_subtitle)
    .render(frame, cols[1]);

    // Net Balance StatCard with MoM trend
    let net_formatted = Money::new(net).format(currency);
    let net_subtitle = format_mom_subtitle(net_change, locale);
    StatCard::new(t(locale, TextKey::StatsNetBalance), net_formatted, theme)
        .subtitle(&net_subtitle)
        .render(frame, cols[2]);
}

/// Render the category list with amounts and bars.
pub fn render_category_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    use crate::ui::components::card::Card;

    let card = Card::new(t(state.locale, TextKey::StatsCategoryBreakdown), theme);
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
            let bar = ascii_bar(
                amount.saturating_abs() as u64,
                total.saturating_abs() as u64,
                15,
            );

            // Get category icon based on name pattern
            let icon = get_category_icon(category);

            // Alert indicator for high spending categories
            // Use ❗ (U+2757, char width 2) instead of ⚠️ (U+26A0+FE0F, char
            // width 1) to avoid ratatui width misalignment.
            let alert = if pct >= 40 {
                Span::styled(" ❗", Style::default().fg(theme.warning))
            } else {
                Span::raw("   ")
            };

            Line::from(vec![
                Span::styled(
                    format!("{}. ", idx + 1),
                    Style::default().fg(theme.text_muted),
                ),
                Span::styled(format!("{icon} "), Style::default().fg(theme.text_muted)),
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
                Span::styled(
                    format!(" {:>2}%", pct),
                    Style::default().fg(theme.text_muted),
                ),
                alert,
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(rows), inner);
}

/// Get an emoji icon for a category based on common patterns.
///
/// All returned emojis must have `UnicodeWidthChar::width() == 2` for the base
/// codepoint so that ratatui's per-character width calculation matches terminal
/// rendering. Avoid characters that need VS16 (U+FE0F) to reach width 2
/// (e.g. U+1F37D, U+1F6CD, U+2708, U+1F6E1) because ratatui processes chars
/// individually and would see width 1 for the base, causing a 1-cell offset.
pub fn get_category_icon(category: &str) -> &'static str {
    let lower = category.to_lowercase();
    if lower.contains("food")
        || lower.contains("grocer")
        || lower.contains("restaurant")
        || lower.contains("ristorante")
        || lower.contains("spesa")
        || lower.contains("alimentar")
        || lower.contains("cibo")
    {
        "🍴"
    } else if lower.contains("house")
        || lower.contains("rent")
        || lower.contains("home")
        || lower.contains("casa")
        || lower.contains("affitto")
    {
        "🏠"
    } else if lower.contains("transport")
        || lower.contains("trasport")
        || lower.contains("car")
        || lower.contains("gas")
        || lower.contains("benzina")
        || lower.contains("auto")
    {
        "🚗"
    } else if lower.contains("health")
        || lower.contains("medical")
        || lower.contains("medic")
        || lower.contains("farmac")
        || lower.contains("salute")
    {
        "🏥"
    } else if lower.contains("entertain")
        || lower.contains("fun")
        || lower.contains("divertiment")
        || lower.contains("cinema")
        || lower.contains("svago")
    {
        "🎬"
    } else if lower.contains("shop")
        || lower.contains("cloth")
        || lower.contains("abbigliam")
        || lower.contains("vestit")
        || lower.contains("acquist")
    {
        "🛒"
    } else if lower.contains("bill") || lower.contains("utilit") || lower.contains("bolletta") {
        "💡"
    } else if lower.contains("subscri")
        || lower.contains("streaming")
        || lower.contains("abbonament")
    {
        "📱"
    } else if lower.contains("travel") || lower.contains("vacanz") || lower.contains("viaggio") {
        "🛫"
    } else if lower.contains("educat")
        || lower.contains("school")
        || lower.contains("universit")
        || lower.contains("scuola")
    {
        "📚"
    } else if lower.contains("assicuraz") || lower.contains("insurance") {
        "🔒"
    } else {
        "📁"
    }
}

// `get_currency` is provided by `crate::ui::common`.
pub(crate) use crate::ui::common::get_currency;

// Re-export from app layer where the business logic now lives.
pub(crate) use crate::app::{calculate_net_change, percentage_change};

/// Format MoM subtitle with arrow indicator.
pub fn format_mom_subtitle(change: Option<f64>, locale: crate::text::Locale) -> String {
    match change {
        Some(pct) => {
            let arrow = if pct > 0.0 {
                "↑"
            } else if pct < 0.0 {
                "↓"
            } else {
                "→"
            };
            let sign = if pct > 0.0 { "+" } else { "" };
            format!("{arrow} {sign}{:.0}% {}", pct, t(locale, TextKey::StatsMoM))
        }
        None => t(locale, TextKey::StatsThisMonth).to_string(),
    }
}

/// Full month names indexed by month number (1-12).
const MONTH_NAMES: [&str; 13] = [
    "Unknown",
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Build a visual timeline showing recent months with the current one
/// highlighted.
pub fn build_month_timeline<'a>(year: i32, month: u32, theme: &Theme) -> Line<'a> {
    use crate::app::{month_label, offset_month};

    let mut spans = Vec::new();

    // Show 5 months: 2 before, current, 2 after
    let months_to_show = [
        offset_month(year, month, -2),
        offset_month(year, month, -1),
        (year, month),
        offset_month(year, month, 1),
        offset_month(year, month, 2),
    ];

    spans.push(Span::styled(
        "Month: ",
        Style::default().fg(theme.text_muted),
    ));
    spans.push(Span::styled(
        format!(
            "{} {}",
            MONTH_NAMES.get(month as usize).unwrap_or(&"Unknown"),
            year
        ),
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw("  "));

    // Navigation hint
    spans.push(Span::styled("[◀", Style::default().fg(theme.accent)));

    for (i, (y, m)) in months_to_show.iter().enumerate() {
        let short_name = month_label(*m);
        if i > 0 {
            spans.push(Span::raw("  "));
        }

        if (*y, *m) == (year, month) {
            // Current month - highlighted
            spans.push(Span::styled(
                format!("[{short_name}]*"),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                format!("[{short_name}]"),
                Style::default().fg(theme.text_muted),
            ));
        }
    }

    spans.push(Span::styled("▶]", Style::default().fg(theme.accent)));

    Line::from(spans)
}

/// Get status badge based on trend.
pub fn trend_status_badge(
    change: Option<f64>,
    positive_is_good: bool,
    theme: &Theme,
) -> (&'static str, ratatui::style::Color) {
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
        None => ("N/A", theme.text_muted),
    }
}

/// Build a consolidated trend line with sparkline, amount, and status.
pub fn build_trend_line<'a>(
    label: &str,
    change: Option<f64>,
    amount: i64,
    status: (&'static str, ratatui::style::Color),
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

    let amount_color = if amount >= 0 {
        theme.positive
    } else {
        theme.negative
    };

    Line::from(vec![
        Span::styled(format!("{label:<12}"), Style::default().fg(theme.text)),
        Span::styled(
            format!("({arrow} {change_str}) "),
            Style::default().fg(theme.text_muted),
        ),
        Span::styled(
            format!("{:>12}", Money::new(amount).format(currency)),
            Style::default().fg(amount_color),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Status: {status_label}"),
            Style::default().fg(status_color),
        ),
    ])
}
