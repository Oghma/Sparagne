use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use crate::{app::ToastLevel, app::ToastState, ui::theme::Theme};

pub fn render(frame: &mut Frame<'_>, area: Rect, toast: Option<&ToastState>) {
    let Some(toast) = toast else {
        return;
    };
    let theme = Theme::default();
    let width = (toast.message.len() + 4).min(area.width as usize) as u16;
    let height = 3u16;
    let x = area.x + area.width.saturating_sub(width);
    let y = area
        .y
        .saturating_add(area.height.saturating_sub(height + 1));
    let rect = Rect { x, y, width, height };

    let style = match toast.level {
        ToastLevel::Info => Style::default().fg(theme.text),
        ToastLevel::Success => Style::default().fg(theme.positive),
        ToastLevel::Error => Style::default().fg(theme.error),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style);
    let content = Paragraph::new(Line::from(toast.message.as_str())).style(style);
    frame.render_widget(content.block(block), rect);
}
