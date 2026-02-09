mod create;
mod defaults;
mod footer;
mod list;
mod view;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::{
    app::{AppState, VaultMode},
    ui::theme::Theme,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(area);

    match state.vault_ui.mode {
        VaultMode::View => view::render(frame, layout[0], state, theme),
        VaultMode::Create => create::render(frame, layout[0], state, theme),
        VaultMode::Defaults => defaults::render(frame, layout[0], state, theme),
        VaultMode::Select => list::render(frame, layout[0], state, theme),
    }

    footer::render_footer(frame, layout[1], state, theme);
}
