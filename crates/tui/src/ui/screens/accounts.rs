use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::{
    app::{AccountsTab, AppState},
    ui::{
        components::{card::Card, tab_bar, tab_bar::TabBarItem},
        screens,
        theme::Theme,
    },
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);

    render_tab_bar(frame, layout[0], state, &theme);

    match state.accounts_tab {
        AccountsTab::Sources => screens::wallets::render(frame, layout[1], state),
        AccountsTab::Envelopes => screens::flows::render(frame, layout[1], state),
        AccountsTab::Goals => render_goals_placeholder(frame, layout[1], &theme),
    }
}

fn render_tab_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let card = Card::new("Accounts", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let items = [
        TabBarItem {
            label: "💰 Sources",
        },
        TabBarItem {
            label: "📦 Envelopes",
        },
        TabBarItem {
            label: "🎯 Goals"
        },
    ];

    tab_bar::render(frame, inner, &items, state.accounts_tab.index(), theme);
}

fn render_goals_placeholder(frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
    let card = Card::new("Goals", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let lines = vec![
        Line::from(Span::styled(
            "Goals view is coming soon.",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Use Sources for wallets and Envelopes for budgets.",
            Style::default().fg(theme.text_muted),
        )),
    ];

    let paragraph = Paragraph::new(lines)
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}
