use ratatui::{
    layout::{Constraint, Layout, Rect},
    prelude::Direction,
};

pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);

    horizontal[1]
}

pub mod bulk_category_dialog;
pub mod card;
pub mod charts;
pub mod command_palette;
pub mod confirm_dialog;
pub mod error_dialog;
pub mod global_search;
pub mod grouping_dialog;
pub mod help_overlay;
pub mod hints;
pub mod input_dialog;
pub mod loading;
pub mod money;
pub(crate) mod recent_transactions;
pub mod tab_bar;
pub mod tabs;
pub mod toast;
