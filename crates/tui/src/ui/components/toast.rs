use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};
use std::time::Instant;

use crate::{
    app::{ToastLevel, ToastState},
    ui::theme::Theme,
};

/// Status icons for toast notifications
const ICON_SUCCESS: &str = "✓";
const ICON_ERROR: &str = "✗";
const ICON_INFO: &str = "ℹ";
const ICON_UNDO: &str = "↺";

pub fn render(frame: &mut Frame<'_>, area: Rect, toast: Option<&ToastState>) {
    let Some(toast) = toast else {
        return;
    };
    let theme = Theme::default();

    // Get icon and color based on toast level
    let (icon, border_color, text_color) = match toast.level {
        ToastLevel::Info => (ICON_INFO, theme.info, theme.text),
        ToastLevel::Success => (ICON_SUCCESS, theme.positive, theme.positive),
        ToastLevel::Error => (ICON_ERROR, theme.negative, theme.negative),
        ToastLevel::Undo => (ICON_UNDO, theme.info, theme.text),
    };

    let undo_visual = if toast.level == ToastLevel::Undo {
        Some(render_undo_visual(toast, &theme))
    } else {
        None
    };

    // Calculate dimensions: icon + space + message + extras + padding
    let mut content_width = icon.len() + 1 + toast.message.len();
    if let Some(visual) = undo_visual.as_ref() {
        // "  [u] undo  5s  ███░░"
        content_width += 2 + "[u] undo  ".len() + visual.seconds.len() + 2 + visual.bar.len();
    }
    let width = (content_width + 4).min(area.width as usize) as u16;
    let height = 3u16;

    // Position bottom-right
    let x = area.x + area.width.saturating_sub(width + 1);
    let y = area
        .y
        .saturating_add(area.height.saturating_sub(height + 2));
    let rect = Rect {
        x,
        y,
        width,
        height,
    };

    // Build content with icon
    let mut spans = vec![
        Span::styled(icon, Style::default().fg(border_color)),
        Span::raw(" "),
        Span::styled(&toast.message, Style::default().fg(text_color)),
    ];

    if let Some(visual) = undo_visual {
        spans.push(Span::raw("  "));
        spans.push(Span::styled("[u]", Style::default().fg(theme.accent)));
        spans.push(Span::styled(
            " undo ",
            Style::default().fg(theme.text_muted),
        ));
        spans.push(Span::styled(
            visual.seconds,
            Style::default().fg(theme.info),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            visual.bar,
            Style::default().fg(visual.bar_color),
        ));
    }

    let content_line = Line::from(spans);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let content = Paragraph::new(content_line);
    frame.render_widget(content.block(block), rect);
}

struct UndoVisual {
    seconds: String,
    bar: String,
    bar_color: ratatui::style::Color,
}

fn render_undo_visual(toast: &ToastState, theme: &Theme) -> UndoVisual {
    let total = toast.expires_at.saturating_duration_since(toast.created_at);
    if total.is_zero() {
        return UndoVisual {
            seconds: "0s".to_string(),
            bar: "----------".to_string(),
            bar_color: theme.negative,
        };
    }
    let now = Instant::now();
    let remaining = toast.expires_at.saturating_duration_since(now);
    let remaining_ms = remaining.as_millis() as u64;
    let remaining_secs = remaining_ms.saturating_add(999) / 1000;

    let segments = 12usize;
    let ratio = (remaining.as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0);
    let filled = ((ratio * segments as f32).ceil() as usize).min(segments);
    let empty = segments.saturating_sub(filled);
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    let bar_color = if ratio > 0.66 {
        theme.info
    } else if ratio > 0.33 {
        theme.warning
    } else {
        theme.negative
    };

    UndoVisual {
        seconds: format!("{remaining_secs}s"),
        bar,
        bar_color,
    }
}
