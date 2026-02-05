//! Shared utilities for wallet rendering.

use engine::Currency;

/// Transaction type icons
pub const ICON_INCOME: &str = "▲";
pub const ICON_EXPENSE: &str = "▼";
pub const ICON_REFUND: &str = "↩";
pub const ICON_TRANSFER: &str = "⇄";

/// Renders a progress bar with filled and empty blocks.
pub fn progress_bar(value: i64, max: i64, width: usize) -> String {
    if max == 0 {
        return "░".repeat(width);
    }

    let ratio = (value.unsigned_abs() as f64 / max.unsigned_abs() as f64).clamp(0.0, 1.0);
    let filled = ((ratio * width as f64) as usize).min(width);
    let empty = width.saturating_sub(filled);

    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

/// Maps API currency type to engine currency type.
pub fn map_currency(currency: &api_types::Currency) -> Currency {
    match currency {
        api_types::Currency::Eur => Currency::Eur,
    }
}
