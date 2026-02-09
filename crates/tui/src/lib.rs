//! Sparagne TUI — terminal interface for personal finance tracking.

pub mod app;
pub mod config;

pub use error::Result;

mod client;
mod error;
mod local_state;
mod quick_add;
mod text;
mod ui;
mod validation;
