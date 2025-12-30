use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{app::Section, ui::theme::Theme};

/// Renders a horizontal tab bar for section navigation.
pub fn render_tabs(frame: &mut Frame<'_>, area: Rect, active: Section, theme: &Theme) {
    let sections = [
        Section::Home,
        Section::Transactions,
        Section::Wallets,
        Section::Flows,
        Section::Vault,
        Section::Stats,
    ];

    // Build the tab labels
    let mut spans = Vec::new();
    spans.push(Span::raw(" ")); // Leading padding

    for (i, section) in sections.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  ")); // Gap between tabs
        }

        let label = section.label();
        if *section == active {
            spans.push(Span::styled("[", Style::default().fg(theme.accent)));
            spans.push(Span::styled(
                label,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled("]", Style::default().fg(theme.accent)));
        } else {
            spans.push(Span::styled(label, Style::default().fg(theme.text_muted)));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Returns the shortcut hint for tab navigation.
pub fn tab_shortcuts(theme: &Theme) -> Vec<Span<'static>> {
    vec![
        Span::styled("h", Style::default().fg(theme.accent)),
        Span::raw("/"),
        Span::styled("t", Style::default().fg(theme.accent)),
        Span::raw("/"),
        Span::styled("w", Style::default().fg(theme.accent)),
        Span::raw("/"),
        Span::styled("f", Style::default().fg(theme.accent)),
        Span::raw("/"),
        Span::styled("v", Style::default().fg(theme.accent)),
        Span::raw("/"),
        Span::styled("s", Style::default().fg(theme.accent)),
        Span::raw(" nav"),
    ]
}
