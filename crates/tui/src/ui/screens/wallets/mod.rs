//! Wallets screen - modular implementation.
//!
//! This module is organized into sub-modules for maintainability:
//! - `list`: Wallet list view
//! - `detail`: Wallet detail panel
//! - `form`: Wallet creation/rename form
//!
//! The main `render()` function renders the list and any overlay dialogs.

pub(crate) mod detail;
mod form;
mod list;

use ratatui::{Frame, layout::Rect};

use crate::{
    app::{AppState, EntityListMode},
    ui::theme::Theme,
};

use form::render_rename_dialog;
use list::render_list;

pub(crate) use detail::render_detail as render_wallet_detail;

/// Main entry point for wallet screen rendering.
/// Renders the list (with inline create form) plus rename dialog overlay.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme, focused: bool) {
    render_list(frame, area, state, theme, focused);

    if state.wallets.mode == EntityListMode::Rename {
        render_rename_dialog(frame, area, state, theme);
    }
}
