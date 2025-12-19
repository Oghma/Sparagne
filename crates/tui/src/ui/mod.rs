pub mod components;
pub mod keymap;
pub mod screens;

mod terminal;
mod theme;

use ratatui::Frame;

use crate::app::AppState;

pub use terminal::{AppTerminal as Terminal, restore_terminal, setup_terminal};

pub fn render(frame: &mut Frame<'_>, state: &AppState) {
    let area = frame.area();
    match state.screen {
        crate::app::Screen::Login => screens::login::render(frame, area, state),
        crate::app::Screen::Home => screens::home::render(frame, area, state),
    }
}
