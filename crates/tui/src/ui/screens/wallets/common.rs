//! Shared utilities for wallet rendering.
//!
//! Re-exports consolidated helpers from [`crate::ui::common`] so that sibling
//! modules can continue using `super::common::X` imports.

pub(crate) use crate::ui::common::{
    ICON_EXPENSE, ICON_INCOME, ICON_REFUND, ICON_TRANSFER, map_currency, progress_bar,
};
