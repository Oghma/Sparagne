/// Transaction screen - modular implementation.
///
/// This module is organized into sub-modules for maintainability:
/// - `common`: Shared utilities and formatting helpers
/// - `header`: Header rendering (filters, search, hints)
/// - `list`: Transaction list with grouping
/// - `detail`: Transaction detail panel
/// - `form`: Transaction creation/edit form
/// - `filter`: Filter modal
/// - `quick_add`: Quick-add input bar
/// - `pickers`: Wallet/Flow/Transfer pickers
///
/// The main `render()` function routes to the appropriate view based on `TransactionsMode`.

mod common;
mod detail;
mod filter;
mod form;
mod header;
mod list;
mod pickers;
mod quick_add;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::{
    app::{AppState, TransactionsMode},
    ui::theme::Theme,
};

use detail::render_detail;
use filter::render_filter_overlay;
use form::render_form_overlay;
use header::render_header;
use list::render_list;
use pickers::{render_scope_picker, render_transfer_form, render_transfer_picker};

/// Main entry point for transaction screen rendering.
/// Routes to appropriate sub-views based on mode.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);

    render_header(frame, layout[0], state);
    match state.transactions.mode {
        TransactionsMode::List
        | TransactionsMode::PickWallet
        | TransactionsMode::PickFlow
        | TransactionsMode::TransferPicker
        | TransactionsMode::TransferWallet
        | TransactionsMode::TransferFlow
        | TransactionsMode::Filter
        | TransactionsMode::Form
        | TransactionsMode::Edit => {
            render_list(frame, layout[1], state, &theme);
            if matches!(
                state.transactions.mode,
                TransactionsMode::PickWallet | TransactionsMode::PickFlow
            ) {
                render_scope_picker(frame, layout[1], state, &theme);
            } else if state.transactions.mode == TransactionsMode::TransferPicker {
                render_transfer_picker(frame, layout[1], state, &theme);
            } else if matches!(
                state.transactions.mode,
                TransactionsMode::TransferWallet | TransactionsMode::TransferFlow
            ) {
                render_transfer_form(frame, layout[1], state, &theme);
            } else if matches!(
                state.transactions.mode,
                TransactionsMode::Form | TransactionsMode::Edit
            ) {
                render_form_overlay(frame, layout[1], state, &theme);
            } else if state.transactions.mode == TransactionsMode::Filter {
                render_filter_overlay(frame, layout[1], state, &theme);
            }
        }
        TransactionsMode::Detail => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(layout[1]);
            render_list(frame, columns[0], state, &theme);
            render_detail(frame, columns[1], state, &theme);
        }
    }
}
