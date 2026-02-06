//! Wallets screen - modular implementation.
//!
//! This module is organized into sub-modules for maintainability:
//! - `common`: Shared utilities (progress bar, currency mapping)
//! - `list`: Wallet list view
//! - `detail`: Wallet detail panel
//! - `form`: Wallet creation/rename form
//!
//! The main `render()` function routes to the appropriate view based on `EntityListMode`.

mod common;
mod detail;
mod form;
mod list;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::{
    app::{AppState, EntityListMode},
    ui::theme::Theme,
};

use detail::render_detail;
use form::render_rename_dialog;
use list::render_list;

/// Main entry point for wallet screen rendering.
/// Routes to appropriate sub-views based on mode.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {

    match state.wallets.mode {
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

    if state.wallets.mode == EntityListMode::Rename {
        render_rename_dialog(frame, area, state, theme);
    }
}
