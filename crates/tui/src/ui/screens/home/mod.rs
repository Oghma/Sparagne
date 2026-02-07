//! Home screen - modular implementation.
//!
//! This module is organized into sub-modules for maintainability:
//! - `common`: Shared utilities (currency helpers, text truncation)
//! - `stats_bar`: Stats bar rendering (net worth, income, expenses)
//! - `quick_balances`: Quick balances card
//! - `activity_feed`: Activity feed with transactions and alerts
//!
//! The main `render()` function routes to appropriate layout based on terminal
//! width.

mod activity_feed;
mod common;
mod quick_balances;
mod stats_bar;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::{app::AppState, ui::theme::Theme};

use activity_feed::render_activity_feed;
use quick_balances::render_quick_balances;
use stats_bar::{render_stats_bar, render_stats_bar_compact};

/// Main entry point for home screen rendering.
/// Adapts layout based on terminal width.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    // Main layout based on terminal width
    if area.width >= 100 {
        render_large_layout(frame, area, state, theme);
    } else if area.width >= 80 {
        render_medium_layout(frame, area, state, theme);
    } else {
        render_small_layout(frame, area, state, theme);
    }
}

fn render_large_layout(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    // Stats bar (6 rows) + Main content below
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(8)])
        .split(area);

    render_stats_bar(frame, main_layout[0], state, theme);

    // Main content: Quick Balances (30%) | Activity Feed (70%)
    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(main_layout[1]);

    render_quick_balances(frame, content_layout[0], state, theme);
    render_activity_feed(frame, content_layout[1], state, theme);
}

fn render_medium_layout(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    // Stats bar (5 rows) + Quick Balances (8 rows) + Activity Feed (rest)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(8),
            Constraint::Min(6),
        ])
        .split(area);

    render_stats_bar(frame, layout[0], state, theme);
    render_quick_balances(frame, layout[1], state, theme);
    render_activity_feed(frame, layout[2], state, theme);
}

fn render_small_layout(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    // Compact stats (1 row) + Quick Balances (6 rows) + Activity Feed (rest)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(6),
            Constraint::Min(4),
        ])
        .split(area);

    render_stats_bar_compact(frame, layout[0], state, theme);
    render_quick_balances(frame, layout[1], state, theme);
    render_activity_feed(frame, layout[2], state, theme);
}
