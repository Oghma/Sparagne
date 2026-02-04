//! Tab bar rendering and tab routing for the stats screen.

use ratatui::{Frame, layout::Rect};

use crate::{
    app::{AppState, StatsTab},
    ui::{
        components::tab_bar::{self, TabBarItem},
        theme::Theme,
    },
};

use super::{cash_flow, net_worth, spending};

/// Render the stats tab bar.
pub fn render_tab_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let items = [
        TabBarItem::new("1 Cash Flow"),
        TabBarItem::new("2 Spending"),
        TabBarItem::new("3 Net Worth"),
    ];
    tab_bar::render(frame, area, &items, state.stats.tab.index(), theme);
}

/// Render the content for the currently selected tab.
pub fn render_tab_content(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    match state.stats.tab {
        StatsTab::CashFlow => cash_flow::render(frame, area, state, theme),
        StatsTab::Spending => spending::render(frame, area, state, theme),
        StatsTab::NetWorth => net_worth::render(frame, area, state, theme),
    }
}
