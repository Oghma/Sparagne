use engine::{Currency, Money};
use ratatui::{
    style::{Modifier, Style},
    text::Span,
    widgets::{Gauge, LineGauge},
};

use crate::ui::theme::Theme;

/// Creates a styled span for a money amount with semantic coloring.
///
/// - Positive amounts: green with `+` prefix
/// - Negative amounts: red (no prefix, negative sign shown)
/// - Zero: neutral text color
#[must_use]
pub fn styled_amount(amount: i64, currency: Currency, theme: &Theme) -> Span<'static> {
    let money = Money::new(amount);
    let formatted = money.format(currency);

    let (color, prefix) = if amount > 0 {
        (theme.positive, "+")
    } else if amount < 0 {
        (theme.negative, "")
    } else {
        (theme.text, "")
    };

    Span::styled(format!("{prefix}{formatted}"), Style::default().fg(color))
}

/// Creates a styled span for a money amount without the +/- prefix.
/// Used when the context already makes the sign clear (e.g., "Income: €1,234").
#[must_use]
pub fn styled_amount_no_sign(amount: i64, currency: Currency, theme: &Theme) -> Span<'static> {
    let money = Money::new(amount.abs());
    let formatted = money.format(currency);

    let color = if amount > 0 {
        theme.positive
    } else if amount < 0 {
        theme.negative
    } else {
        theme.text
    };

    Span::styled(formatted, Style::default().fg(color))
}

/// Creates a styled span with bold modifier for emphasis (e.g., totals).
#[must_use]
pub fn styled_amount_bold(amount: i64, currency: Currency, theme: &Theme) -> Span<'static> {
    let money = Money::new(amount);
    let formatted = money.format(currency);

    let (color, prefix) = if amount > 0 {
        (theme.positive, "+")
    } else if amount < 0 {
        (theme.negative, "")
    } else {
        (theme.text, "")
    };

    Span::styled(
        format!("{prefix}{formatted}"),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}

/// Creates a progress gauge for flow cap usage.
///
/// Returns `None` if the flow has no cap (unlimited).
#[must_use]
pub fn flow_cap_gauge(
    current: i64,
    cap: Option<i64>,
    label: &str,
    theme: &Theme,
) -> Option<Gauge<'static>> {
    let cap_value = cap?;
    if cap_value <= 0 {
        return None;
    }

    let ratio = (current as f64 / cap_value as f64).clamp(0.0, 1.0);
    let percentage = (ratio * 100.0) as u16;

    // Color based on usage: green < 70%, warning 70-90%, red > 90%
    let gauge_color = if ratio < 0.7 {
        theme.positive
    } else if ratio < 0.9 {
        theme.warning
    } else {
        theme.negative
    };

    Some(
        Gauge::default()
            .gauge_style(Style::default().fg(gauge_color))
            .percent(percentage)
            .label(label.to_string()),
    )
}

/// Creates a line gauge for compact progress display.
///
/// Returns `None` if the flow has no cap (unlimited).
#[must_use]
pub fn flow_cap_line_gauge(
    current: i64,
    cap: Option<i64>,
    theme: &Theme,
) -> Option<LineGauge<'static>> {
    let cap_value = cap?;
    if cap_value <= 0 {
        return None;
    }

    let ratio = (current as f64 / cap_value as f64).clamp(0.0, 1.0);

    // Color based on usage
    let gauge_color = if ratio < 0.7 {
        theme.positive
    } else if ratio < 0.9 {
        theme.warning
    } else {
        theme.negative
    };

    Some(
        LineGauge::default()
            .filled_style(Style::default().fg(gauge_color))
            .line_set(ratatui::symbols::line::THICK)
            .ratio(ratio),
    )
}

/// Creates a simple text-based progress bar for inline use.
///
/// Returns a string like `████████░░ 80%` or `━━━━━━━━━━` for unlimited.
#[must_use]
pub fn inline_progress_bar(current: i64, cap: Option<i64>, width: usize) -> String {
    match cap {
        Some(cap_value) if cap_value > 0 => {
            let ratio = (current as f64 / cap_value as f64).clamp(0.0, 1.0);
            let filled = ((ratio * width as f64) as usize).min(width);
            let empty = width.saturating_sub(filled);
            let percentage = (ratio * 100.0) as u16;

            format!(
                "{}{} {}%",
                "█".repeat(filled),
                "░".repeat(empty),
                percentage
            )
        }
        _ => "━".repeat(width),
    }
}

/// Creates a styled inline progress bar with appropriate coloring.
#[must_use]
pub fn styled_progress_bar(
    current: i64,
    cap: Option<i64>,
    width: usize,
    theme: &Theme,
) -> Span<'static> {
    let bar = inline_progress_bar(current, cap, width);

    let color = match cap {
        Some(cap_value) if cap_value > 0 => {
            let ratio = current as f64 / cap_value as f64;
            if ratio < 0.7 {
                theme.positive
            } else if ratio < 0.9 {
                theme.warning
            } else {
                theme.negative
            }
        }
        _ => theme.dim,
    };

    Span::styled(bar, Style::default().fg(color))
}

/// Formats a percentage change with appropriate styling.
///
/// Returns something like `▲ +2.3%` (green) or `▼ -1.5%` (red).
#[must_use]
pub fn styled_percentage_change(change: f64, theme: &Theme) -> Span<'static> {
    let (arrow, color) = if change >= 0.0 {
        ("▲", theme.positive)
    } else {
        ("▼", theme.negative)
    };

    let sign = if change >= 0.0 { "+" } else { "" };
    Span::styled(
        format!("{arrow} {sign}{change:.1}%"),
        Style::default().fg(color),
    )
}
