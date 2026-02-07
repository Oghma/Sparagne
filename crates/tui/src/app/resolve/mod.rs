//! Entity resolution and lookup utilities.
//!
//! This module provides functions for resolving entity IDs to names, finding
//! matching entities by name query, extracting entities from transactions, and
//! determining default entities for operations.

mod categories;
mod defaults;
mod flows;
mod transactions;
mod wallets;

pub(crate) use categories::resolve_category_matches;
pub(crate) use defaults::{default_wallet_flow, default_wallet_flow_names};
pub(crate) use flows::{ordered_active_flows, resolve_flow_matches, resolve_flow_name};
pub(crate) use transactions::{
    extract_flow_transfer, extract_wallet_flow, extract_wallet_transfer,
};
pub(crate) use wallets::{resolve_wallet_matches, resolve_wallet_name};
