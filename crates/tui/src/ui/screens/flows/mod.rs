//! Flows screen - modular implementation.
//!
//! This module is organized into sub-modules for maintainability:
//! - `common`: Shared utilities (currency mapping)
//! - `list`: Flow list rendering with items and stats header
//! - `detail`: Flow detail panel (right side)
//! - `form`: Create flow form overlay
//! - `dialogs`: Rename dialog and other modals
//!
//! The main `render()` function routes to the appropriate view based on `EntityListMode`.

mod common;
mod detail;
mod dialogs;
mod form;
mod list;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::{app::AppState, ui::theme::Theme};

use detail::render_detail;
use dialogs::render_rename_dialog;
use list::render_list;

use crate::app::EntityListMode;

/// Main entry point for flows screen rendering.
/// Routes to appropriate sub-views based on mode.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {

    match state.flows.mode {
        EntityListMode::Detail => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_list(frame, columns[0], state, theme);
            render_detail(frame, columns[1], state, theme);
        }
        EntityListMode::Create | EntityListMode::Rename | EntityListMode::List => {
            render_list(frame, area, state, theme)
        }
    }

    if state.flows.mode == EntityListMode::Rename {
        render_rename_dialog(frame, area, state, theme);
    }
}
