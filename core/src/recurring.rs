//! Recurring templates: schedule types and period arithmetic.
//!
//! A template fires on a sequence of *period dates* computed from its
//! [`Schedule`]. Nothing is ever materialized automatically: the app lists the
//! due periods with `Core::pending_recurring` and the user executes or skips
//! each one with a command.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Rhythm of a recurring template.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "unit", rename_all = "snake_case")]
pub enum Frequency {
    Daily,
    /// `weekday` is ISO: Monday = 1 ... Sunday = 7.
    Weekly {
        weekday: u8,
    },
    /// `day` 1..=31, clamped to the last day of shorter months.
    Monthly {
        day: u8,
    },
    /// `month` 1..=12, `day` 1..=31 clamped to the month's length.
    Yearly {
        month: u8,
        day: u8,
    },
}

/// When a template fires: every `interval` units of `frequency`, starting
/// from the first matching date on or after `start_date`, up to and including
/// `end_date`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    pub frequency: Frequency,
    /// `>= 1`.
    pub interval: u32,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
}
