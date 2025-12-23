use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::{app::Section, ui::theme::Theme};

/// Renders a horizontal tab bar for section navigation with underline indicator.
pub fn render_tabs(frame: &mut Frame<'_>, area: Rect, active: Section, theme: &Theme) {
    // Need 2 rows: one for labels, one for underline
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

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
    let mut underline_spans = Vec::new();
    spans.push(Span::raw("  ")); // Leading padding
    underline_spans.push(Span::raw("  "));

    for (i, section) in sections.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   ")); // Gap between tabs
            underline_spans.push(Span::raw("   "));
        }

        let label = section.label();
        let label_len = label.len();

        if *section == active {
            // Active tab: bold accent color
            spans.push(Span::styled(
                label,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
            // Underline for active tab
            underline_spans.push(Span::styled(
                "═".repeat(label_len),
                Style::default().fg(theme.accent),
            ));
        } else {
            // Inactive tab: muted
            spans.push(Span::styled(label, Style::default().fg(theme.text_muted)));
            // No underline for inactive
            underline_spans.push(Span::raw(" ".repeat(label_len)));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), rows[0]);
    frame.render_widget(Paragraph::new(Line::from(underline_spans)), rows[1]);
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
