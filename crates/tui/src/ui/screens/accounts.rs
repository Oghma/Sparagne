use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::Clear,
};

use crate::{
    app::{AccountsTab, AppState, EntityListMode},
    ui::{
        components::centered_rect,
        screens::{
            flows::{self, render_flow_detail},
            wallets::{self, render_wallet_detail},
        },
        theme::Theme,
    },
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let split =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);

    let wallets_focused = state.accounts_tab == AccountsTab::Wallets;
    wallets::render(frame, split[0], state, theme, wallets_focused);
    flows::render(frame, split[1], state, theme, !wallets_focused);

    // Detail popup overlays (rendered on top of full area)
    if state.wallets.mode == EntityListMode::Detail {
        let popup = centered_rect(70, 80, area);
        frame.render_widget(Clear, popup);
        render_wallet_detail(frame, popup, state, theme);
    }
    if state.flows.mode == EntityListMode::Detail {
        let popup = centered_rect(70, 80, area);
        frame.render_widget(Clear, popup);
        render_flow_detail(frame, popup, state, theme);
    }
}
