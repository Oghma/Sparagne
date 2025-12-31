use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    symbols,
    widgets::{BarChart, Sparkline},
};

use crate::ui::{components::card::Card, theme::Theme};

/// Renders a horizontal bar chart with labeled bars.
///
/// This is a wrapper around ratatui's `BarChart` with consistent styling.
pub fn render_bar_chart(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    data: &[(&str, u64)],
    theme: &Theme,
) {
    let chart = BarChart::default()
        .data(data)
        .bar_width(3)
        .bar_gap(1)
        .bar_style(Style::default().fg(theme.accent))
        .value_style(Style::default().fg(theme.text).add_modifier(Modifier::BOLD))
        .label_style(Style::default().fg(theme.dim));

    if title.is_empty() {
        frame.render_widget(chart, area);
    } else {
        let card = Card::new(title, theme);
        let inner = card.inner(area);
        card.render_frame(frame, area);
        frame.render_widget(chart, inner);
    }
}

/// Renders a sparkline (mini line chart) for trend visualization.
///
/// Useful for showing trends in a compact space.
pub fn render_sparkline(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    data: &[u64],
    theme: &Theme,
) {
    let sparkline = Sparkline::default()
        .data(data)
        .style(Style::default().fg(theme.accent));

    if title.is_empty() {
        frame.render_widget(sparkline, area);
    } else {
        let card = Card::new(title, theme);
        let inner = card.inner(area);
        card.render_frame(frame, area);
        frame.render_widget(sparkline, inner);
    }
}

/// Renders an inline sparkline without borders (for embedding in other
/// widgets).
pub fn render_inline_sparkline(frame: &mut Frame<'_>, area: Rect, data: &[u64], theme: &Theme) {
    let sparkline = Sparkline::default()
        .data(data)
        .style(Style::default().fg(theme.accent));

    frame.render_widget(sparkline, area);
}

/// Creates a simple ASCII-based horizontal bar for inline use.
///
/// Returns a string like `████████░░░░░░░░░░░░` representing the ratio.
#[must_use]
pub fn ascii_bar(value: u64, max: u64, width: usize) -> String {
    if max == 0 {
        return "░".repeat(width);
    }

    let ratio = (value as f64 / max as f64).clamp(0.0, 1.0);
    let filled = ((ratio * width as f64) as usize).min(width);
    let empty = width.saturating_sub(filled);

    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

/// Creates a simple ASCII-based horizontal bar with different fill styles.
#[must_use]
pub fn ascii_bar_styled(value: u64, max: u64, width: usize, style: BarStyle) -> String {
    if max == 0 {
        return match style {
            BarStyle::Block => "░".repeat(width),
            BarStyle::Line => "─".repeat(width),
            BarStyle::Dot => "·".repeat(width),
        };
    }

    let ratio = (value as f64 / max as f64).clamp(0.0, 1.0);
    let filled = ((ratio * width as f64) as usize).min(width);
    let empty = width.saturating_sub(filled);

    match style {
        BarStyle::Block => format!("{}{}", "█".repeat(filled), "░".repeat(empty)),
        BarStyle::Line => format!("{}{}", "━".repeat(filled), "─".repeat(empty)),
        BarStyle::Dot => format!("{}{}", "●".repeat(filled), "○".repeat(empty)),
    }
}

/// Style options for ASCII bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarStyle {
    /// Block characters: █ and ░
    Block,
    /// Line characters: ━ and ─
    Line,
    /// Dot characters: ● and ○
    Dot,
}

/// Creates a mini bar chart representation as a string.
///
/// Returns something like `▁▂▃▅▇▅▃▂▁` for a series of values.
#[must_use]
pub fn mini_bar_chart(values: &[u64]) -> String {
    if values.is_empty() {
        return String::new();
    }

    let max = *values.iter().max().unwrap_or(&1);
    if max == 0 {
        return " ".repeat(values.len());
    }

    let bars = [
        symbols::bar::ONE_EIGHTH,
        symbols::bar::ONE_QUARTER,
        symbols::bar::THREE_EIGHTHS,
        symbols::bar::HALF,
        symbols::bar::FIVE_EIGHTHS,
        symbols::bar::THREE_QUARTERS,
        symbols::bar::SEVEN_EIGHTHS,
        symbols::bar::FULL,
    ];

    values
        .iter()
        .map(|&v| {
            if v == 0 {
                " "
            } else {
                let index = ((v as f64 / max as f64) * 7.0) as usize;
                bars[index.min(7)]
            }
        })
        .collect()
}

/// Creates a percentage bar with label.
///
/// Returns something like `████████░░ 80%`
#[must_use]
pub fn percentage_bar(percentage: u16, width: usize) -> String {
    let filled = ((percentage as usize * width) / 100).min(width);
    let empty = width.saturating_sub(filled);
    format!(
        "{}{} {:>3}%",
        "█".repeat(filled),
        "░".repeat(empty),
        percentage
    )
}

/// Computes the percentage of value relative to max.
#[must_use]
pub fn compute_percentage(value: i64, max: i64) -> u16 {
    if max == 0 {
        return 0;
    }
    ((value.abs() as f64 / max.abs() as f64) * 100.0).min(100.0) as u16
}
