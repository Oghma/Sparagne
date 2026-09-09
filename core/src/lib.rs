//! Sparagne v2 core.
//!
//! Domain rules, a per-vault command log and a SQLite projection of it.
//! Every write goes through [`Core::execute`] as a [`Command`]; state tables
//! are derived and can be rebuilt with [`replay`].
//!
//! Design rules:
//! - amounts are `i64` minor units, one currency per vault;
//! - the command id is the idempotency key, and every entity created by a
//!   command gets an id derived from the command id (equal to it, or a UUID v5
//!   of it), so replaying the log is deterministic;
//! - domain checks (flow caps, non-negativity) run on create and update, never
//!   on void.

#![forbid(unsafe_code)]

mod category;
mod currency;
mod error;
mod flow;
mod money;

pub use category::{normalize_category_display, normalize_category_key, validate_category_name};
pub use currency::Currency;
pub use error::DomainError;
pub use flow::{Flow, FlowMode, UNALLOCATED_NAME};
pub use money::Money;

/// Result alias used across the crate.
pub type Result<T> = std::result::Result<T, DomainError>;
