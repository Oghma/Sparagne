use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::{
    app::{AccountsTab, AppState, Section, SettingsTab},
    text::{Locale, TextKey, t},
    ui::theme::Theme,
};

pub(super) fn render_header(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    locale: Locale,
    theme: &Theme,
) {
    let section_name = match state.section {
        Section::Home => t(locale, TextKey::SectionHome).to_string(),
        Section::Transactions => t(locale, TextKey::SectionTransactions).to_string(),
        Section::Accounts => match state.accounts_tab {
            AccountsTab::Wallets => format!(
                "{} > {}",
                t(locale, TextKey::HintAccounts),
                t(locale, TextKey::HelpSourcesWallets)
            ),
            AccountsTab::Budget => format!(
                "{} > {}",
                t(locale, TextKey::HintAccounts),
                t(locale, TextKey::HelpEnvelopesFlows)
            ),
        },
        Section::Analytics => t(locale, TextKey::HintAnalytics).to_string(),
        Section::Settings => match state.settings_tab {
            SettingsTab::Categories => format!(
                "{} > {}",
                t(locale, TextKey::HintSettings),
                t(locale, TextKey::SectionCategories)
            ),
            SettingsTab::Vault => format!(
                "{} > {}",
                t(locale, TextKey::HintSettings),
                t(locale, TextKey::SectionVault)
            ),
            SettingsTab::Members => format!(
                "{} > {}",
                t(locale, TextKey::HintSettings),
                t(locale, TextKey::SectionMembers)
            ),
            SettingsTab::Preferences => format!(
                "{} > {}",
                t(locale, TextKey::HintSettings),
                t(locale, TextKey::SectionPreferences)
            ),
        },
    };

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("  {}  ", t(locale, TextKey::HelpTitle)),
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled("─ ", Style::default().fg(theme.border)),
            Span::styled(section_name, Style::default().fg(theme.accent)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    frame.render_widget(Paragraph::new(lines).block(block), area);
}
