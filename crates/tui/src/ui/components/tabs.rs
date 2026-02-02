use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::{AccountsTab, Section, SettingsTab},
    ui::theme::Theme,
};

/// Renders a horizontal tab bar for section navigation.
pub fn render_tabs(
    frame: &mut Frame<'_>,
    area: Rect,
    active: Section,
    accounts_tab: Option<AccountsTab>,
    settings_tab: Option<SettingsTab>,
    theme: &Theme,
) {
    let sections = [
        Section::Home,
        Section::Transactions,
        Section::Accounts,
        Section::Analytics,
        Section::Settings,
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

            // Add breadcrumb for sub-tab
            let sub_label = match *section {
                Section::Accounts => accounts_tab.map(|t| t.label()),
                Section::Settings => settings_tab.map(|t| t.label()),
                _ => None,
            };
            if let Some(sub) = sub_label {
                spans.push(Span::styled(" > ", Style::default().fg(theme.dim)));
                spans.push(Span::styled(
                    sub,
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ));
            }

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
        Span::styled("a", Style::default().fg(theme.accent)),
        Span::raw("/"),
        Span::styled("y", Style::default().fg(theme.accent)),
        Span::raw("/"),
        Span::styled("s", Style::default().fg(theme.accent)),
        Span::raw(" nav"),
    ]
}
