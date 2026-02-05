pub(crate) mod common;
pub mod components;
pub mod forms;
pub mod keymap;
pub mod screens;

mod overlays;
mod shell;
mod terminal;
mod theme;

use ratatui::Frame;

use crate::app::AppState;

pub use terminal::{AppTerminal as Terminal, restore_terminal, setup_terminal};
pub use theme::Theme;

pub fn render(frame: &mut Frame<'_>, state: &AppState) {
    let area = frame.area();
    let theme = Theme::default();
    frame.render_widget(
        ratatui::widgets::Block::default()
            .style(ratatui::style::Style::default().bg(theme.background)),
        area,
    );
    match state.screen {
        crate::app::Screen::Login => screens::login::render(frame, area, state),
        crate::app::Screen::Home => {
            shell::render_shell(frame, area, state);
            overlays::render_overlays(frame, area, state);
        }
    }
}
