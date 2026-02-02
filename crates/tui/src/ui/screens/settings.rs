use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::{
    app::{AppState, SettingsTab},
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
        .constraints([
            Constraint::Length(4), // Tab bar
            Constraint::Length(1), // Spacer
            Constraint::Min(0),    // Content
        ])
        .split(area);

    render_tab_bar(frame, layout[0], state, &theme);

    match state.settings_tab {
        SettingsTab::Categories => screens::categories::render(frame, layout[2], state),
        SettingsTab::Vault => screens::vault::render(frame, layout[2], state),
        SettingsTab::Members => screens::members::render(frame, layout[2], state),
    }
}

fn render_tab_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let card = Card::new("Settings", theme);
    let inner = inset(card.inner(area), 1, 0);
    card.render_frame(frame, area);

    let items = [
        TabBarItem {
            label: "1 Categories",
        },
        TabBarItem { label: "2 Vault" },
        TabBarItem { label: "3 Members" },
    ];

    tab_bar::render(frame, inner, &items, state.settings_tab.index(), theme);
}

fn inset(area: Rect, horizontal: u16, vertical: u16) -> Rect {
    let x = area.x.saturating_add(horizontal);
    let y = area.y.saturating_add(vertical);
    let width = area.width.saturating_sub(horizontal.saturating_mul(2));
    let height = area.height.saturating_sub(vertical.saturating_mul(2));
    Rect {
        x,
        y,
        width,
        height,
    }
}
