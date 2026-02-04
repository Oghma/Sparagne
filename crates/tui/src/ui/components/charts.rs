use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
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

// ============================================================================
// Braille Sparklines - High Resolution (8x)
// ============================================================================
//
// Braille characters use a 2x4 dot matrix providing 8 dots per character.
// This gives much higher resolution than standard bar characters.
//
// Dot positions in a Braille cell:
//   ┌───┬───┐
//   │ 1 │ 4 │  <- row 0: 0x01, 0x08
//   │ 2 │ 5 │  <- row 1: 0x02, 0x10
//   │ 3 │ 6 │  <- row 2: 0x04, 0x20
//   │ 7 │ 8 │  <- row 3: 0x40, 0x80
//   └───┴───┘
//
// Base character: U+2800 (⠀ - blank braille)

const BRAILLE_BASE: u32 = 0x2800;

/// Left column dot bits (top to bottom): rows 0, 1, 2, 3
const BRAILLE_LEFT: [u8; 4] = [0x01, 0x02, 0x04, 0x40];

/// Right column dot bits (top to bottom): rows 0, 1, 2, 3
const BRAILLE_RIGHT: [u8; 4] = [0x08, 0x10, 0x20, 0x80];

/// Creates a Braille sparkline with filled area (like an area chart).
///
/// Instead of just the top dot, fills all dots from bottom up to the value.
#[must_use]
pub fn braille_sparkline_filled(values: &[u64]) -> String {
    if values.is_empty() {
        return String::new();
    }

    let max = *values.iter().max().unwrap_or(&1);
    if max == 0 {
        let char_count = values.len().div_ceil(2);
        return "⠀".repeat(char_count);
    }

    let mut result = String::new();

    for chunk in values.chunks(2) {
        let left_val = chunk[0];
        let right_val = chunk.get(1).copied().unwrap_or(0);

        let left_height = normalize_to_4(left_val, max);
        let right_height = normalize_to_4(right_val, max);

        let braille_char = braille_column_pair_filled(left_height, right_height);
        result.push(braille_char);
    }

    result
}

/// Normalizes a value to a 0-4 range for Braille dots.
fn normalize_to_4(value: u64, max: u64) -> u8 {
    if max == 0 {
        return 0;
    }
    // Scale to 0-4 (4 rows of dots)
    let normalized = (value as f64 / max as f64) * 4.0;
    (normalized.round() as u8).min(4)
}

/// Creates a Braille character for a pair of column values (filled/area style).
///
/// Lights all dots from bottom up to the height level.
fn braille_column_pair_filled(left_height: u8, right_height: u8) -> char {
    let mut dots: u8 = 0;

    // Left column: fill from bottom (row 3) up to the height
    for row in (4 - left_height)..4 {
        dots |= BRAILLE_LEFT[row as usize];
    }

    // Right column: same logic
    for row in (4 - right_height)..4 {
        dots |= BRAILLE_RIGHT[row as usize];
    }

    char::from_u32(BRAILLE_BASE + dots as u32).unwrap_or('?')
}

/// Renders a Braille sparkline widget with optional min/max labels.
pub fn render_braille_sparkline(
    frame: &mut Frame<'_>,
    area: Rect,
    data: &[u64],
    theme: &Theme,
    show_minmax: bool,
) {
    if data.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }

    let sparkline_str = braille_sparkline_filled(data);

    let text = if show_minmax {
        let min = data.iter().copied().min().unwrap_or(0);
        let max = data.iter().copied().max().unwrap_or(0);
        format!("{sparkline_str} {min}-{max}")
    } else {
        sparkline_str
    };

    let paragraph = Paragraph::new(Span::styled(text, Style::default().fg(theme.accent)));
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod braille_tests {
    use super::*;

    #[test]
    fn test_braille_sparkline_filled_empty() {
        assert_eq!(braille_sparkline_filled(&[]), "");
    }

    #[test]
    fn test_braille_sparkline_filled_all_zeros() {
        let result = braille_sparkline_filled(&[0, 0, 0, 0]);
        assert_eq!(result.chars().count(), 2); // 4 values = 2 braille chars
        assert!(result.chars().all(|c| c == '⠀'));
    }

    #[test]
    fn test_braille_sparkline_filled_single_value() {
        let result = braille_sparkline_filled(&[100]);
        assert_eq!(result.chars().count(), 1);
        // Max value should fill all 4 rows in left column
        // Rows 0-3 filled: 0x01 | 0x02 | 0x04 | 0x40 = 0x47 -> U+2847
        assert_eq!(result, "⡇");
    }

    #[test]
    fn test_braille_sparkline_filled_pair() {
        let result = braille_sparkline_filled(&[100, 100]);
        assert_eq!(result.chars().count(), 1);
        // Both at max should fill all dots in both columns
        // Left: 0x47, Right: 0x88 | 0x10 | 0x20 | 0x80 = 0xB8
        // Combined: 0x47 | 0xB8 = 0xFF -> U+28FF
        assert_eq!(result, "⣿");
    }

    #[test]
    fn test_braille_sparkline_filled_ascending() {
        let result = braille_sparkline_filled(&[25, 50, 75, 100]);
        assert_eq!(result.chars().count(), 2);
        // Should show increasing fill from left to right
        assert!(!result.contains('⠀')); // No blank chars for non-zero data
    }

    #[test]
    fn test_normalize_to_4() {
        assert_eq!(normalize_to_4(0, 100), 0);
        assert_eq!(normalize_to_4(25, 100), 1);
        assert_eq!(normalize_to_4(50, 100), 2);
        assert_eq!(normalize_to_4(75, 100), 3);
        assert_eq!(normalize_to_4(100, 100), 4);
    }
}
