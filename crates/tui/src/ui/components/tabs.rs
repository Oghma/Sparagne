use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::{Section, SettingsTab},
    text::{Locale, TextKey, t},
    ui::theme::Theme,
};

/// Renders a horizontal tab bar for section navigation.
pub fn render_tabs(
    frame: &mut Frame<'_>,
    area: Rect,
    active: Section,
    settings_tab: Option<SettingsTab>,
    locale: Locale,
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

        let label = section_label(*section, locale);
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
                Section::Settings => settings_tab.map(|tab| settings_tab_label(tab, locale)),
                _ => None,
            };
            if let Some(sub) = sub_label {
                spans.push(Span::styled(" > ", Style::default().fg(theme.text_muted)));
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

fn section_label(section: Section, locale: Locale) -> &'static str {
    match section {
        Section::Home => t(locale, TextKey::SectionHome),
        Section::Transactions => t(locale, TextKey::SectionTransactions),
        Section::Accounts => t(locale, TextKey::SectionAccounts),
        Section::Analytics => t(locale, TextKey::SectionAnalytics),
        Section::Settings => t(locale, TextKey::SectionSettings),
    }
}

fn settings_tab_label(tab: SettingsTab, locale: Locale) -> &'static str {
    match tab {
        SettingsTab::Categories => t(locale, TextKey::SectionCategories),
        SettingsTab::Vault => t(locale, TextKey::SectionVault),
        SettingsTab::Members => t(locale, TextKey::SectionMembers),
        SettingsTab::Preferences => t(locale, TextKey::SectionPreferences),
    }
}
