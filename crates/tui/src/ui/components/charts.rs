use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{BarChart, Paragraph, Sparkline, Wrap},
};
use std::f64::consts::{FRAC_PI_2, TAU};

use crate::ui::{components::card::Card, theme::Theme};

#[derive(Debug, Clone, Copy)]
pub struct PieSlice {
    pub value: u64,
    pub color: Color,
}

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

/// Renders a compact pie chart for proportional data.
pub fn render_pie_chart(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    slices: &[PieSlice],
    theme: &Theme,
) {
    let total: u64 = slices.iter().map(|slice| slice.value).sum();

    let (container, inner) = if title.is_empty() {
        (None, area)
    } else {
        let card = Card::new(title, theme);
        let inner = card.inner(area);
        (Some(card), inner)
    };

    if let Some(card) = container.as_ref() {
        card.render_frame(frame, area);
    }

    if total == 0 || inner.width < 4 || inner.height < 3 {
        let empty = Paragraph::new(Span::styled("No data", Style::default().fg(theme.dim)))
            .alignment(Alignment::Center);
        frame.render_widget(empty, inner);
        return;
    }

    let lines = pie_lines(inner, slices, total);
    let pie = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
    frame.render_widget(pie, inner);
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

fn pie_lines(area: Rect, slices: &[PieSlice], total: u64) -> Vec<Line<'static>> {
    let width = area.width as usize;
    let height = area.height as usize;
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let x_scale = 0.55;
    let cx = (width as f64 - 1.0) / 2.0;
    let cy = (height as f64 - 1.0) / 2.0;
    let max_x = cx * x_scale;
    let max_y = cy;
    let radius = max_x.min(max_y);

    let mut lines = Vec::with_capacity(height);
    for y in 0..height {
        let mut spans = Vec::new();
        let mut current_color: Option<Color> = None;
        let mut buffer = String::new();

        for x in 0..width {
            let dx = (x as f64 - cx) * x_scale;
            let dy = y as f64 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let (ch, color) = if dist <= radius {
                let mut angle = dy.atan2(dx) + FRAC_PI_2;
                if angle < 0.0 {
                    angle += TAU;
                }
                ("●", Some(pie_color(angle, slices, total)))
            } else {
                (" ", None)
            };

            if color != current_color {
                if !buffer.is_empty() {
                    spans.push(Span::styled(
                        buffer.clone(),
                        Style::default().fg(current_color.unwrap_or(Color::Reset)),
                    ));
                    buffer.clear();
                }
                current_color = color;
            }
            buffer.push_str(ch);
        }

        if !buffer.is_empty() {
            spans.push(Span::styled(
                buffer,
                Style::default().fg(current_color.unwrap_or(Color::Reset)),
            ));
        }

        lines.push(Line::from(spans));
    }

    lines
}

fn pie_color(angle: f64, slices: &[PieSlice], total: u64) -> Color {
    let mut acc = 0.0;
    for slice in slices {
        let ratio = slice.value as f64 / total as f64;
        acc += ratio * TAU;
        if angle <= acc {
            return slice.color;
        }
    }
    slices
        .last()
        .map(|slice| slice.color)
        .unwrap_or(Color::Reset)
}
