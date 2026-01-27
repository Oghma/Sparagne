use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::{
    app::{ToastLevel, ToastState},
    ui::theme::Theme,
};

/// Status icons for toast notifications
const ICON_SUCCESS: &str = "✓";
const ICON_ERROR: &str = "✗";
const ICON_WARNING: &str = "⚠";
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

    let undo_bar = if toast.level == ToastLevel::Undo {
        Some(render_undo_bar(toast))
    } else {
        None
    };

    // Calculate dimensions: icon + space + message + extras + padding
    let mut content_width = icon.len() + 1 + toast.message.len();
    if let Some(bar) = undo_bar.as_ref() {
        content_width += 2 + "[u] undo ".len() + bar.len();
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

    if let Some(bar) = undo_bar {
        spans.push(Span::raw("  "));
        spans.push(Span::styled("[u]", Style::default().fg(theme.accent)));
        spans.push(Span::styled(
            " undo ",
            Style::default().fg(theme.text_muted),
        ));
        spans.push(Span::styled(bar, Style::default().fg(theme.info)));
    }

    let content_line = Line::from(spans);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let content = Paragraph::new(content_line);
    frame.render_widget(content.block(block), rect);
}

fn render_undo_bar(toast: &ToastState) -> String {
    let total = toast.expires_at.saturating_duration_since(toast.created_at);
    if total.is_zero() {
        return "----------".to_string();
    }
    let remaining = toast
        .expires_at
        .saturating_duration_since(std::time::Instant::now());
    let segments = 10usize;
    let ratio = remaining.as_secs_f32() / total.as_secs_f32();
    let filled = ((ratio * segments as f32).ceil() as usize).min(segments);
    let empty = segments.saturating_sub(filled);
    format!("{}{}", "#".repeat(filled), "-".repeat(empty))
}
