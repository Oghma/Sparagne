//! Recurring templates: schedule types and period arithmetic.
//!
//! A template fires on a sequence of *period dates* computed from its
//! [`Schedule`]. Nothing is ever materialized automatically: the app lists the
//! due periods with `Core::pending_recurring` and the user executes or skips
//! each one with a command.

use chrono::{Datelike, NaiveDate, TimeDelta};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{DomainError, Result, TransactionKind};

/// Rhythm of a recurring template.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Schedule {
    pub frequency: Frequency,
    /// `>= 1`.
    pub interval: u32,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
}

impl Schedule {
    /// Structural check. Days beyond a month's length are legal: they are
    /// clamped, month by month, from the originally requested day.
    pub fn validate(&self) -> Result<()> {
        if self.interval < 1 {
            return Err(invalid("interval must be >= 1"));
        }
        match self.frequency {
            Frequency::Daily => {}
            Frequency::Weekly { weekday } => {
                if !(1..=7).contains(&weekday) {
                    return Err(invalid("weekday must be 1..=7"));
                }
            }
            Frequency::Monthly { day } => {
                if !(1..=31).contains(&day) {
                    return Err(invalid("day of month must be 1..=31"));
                }
            }
            Frequency::Yearly { month, day } => {
                if !(1..=12).contains(&month) {
                    return Err(invalid("month must be 1..=12"));
                }
                if !(1..=31).contains(&day) {
                    return Err(invalid("day of month must be 1..=31"));
                }
            }
        }
        if let Some(end) = self.end_date
            && end < self.start_date
        {
            return Err(invalid("end_date must be >= start_date"));
        }
        Ok(())
    }

    /// First date on or after `start_date` matching the frequency, ignoring
    /// `end_date`. `None` when the schedule is malformed or out of range.
    #[must_use]
    pub fn first_occurrence(&self) -> Option<NaiveDate> {
        let start = self.start_date;
        match self.frequency {
            Frequency::Daily => Some(start),
            Frequency::Weekly { weekday } => {
                if !(1..=7).contains(&weekday) {
                    return None;
                }
                let current = i64::from(start.weekday().number_from_monday());
                let ahead = (i64::from(weekday) - current).rem_euclid(7);
                start.checked_add_signed(TimeDelta::try_days(ahead)?)
            }
            Frequency::Monthly { day } => {
                let candidate = clamped(start.year(), start.month(), day)?;
                if candidate >= start {
                    return Some(candidate);
                }
                let (year, month) = next_month(start.year(), start.month())?;
                clamped(year, month, day)
            }
            Frequency::Yearly { month, day } => {
                let candidate = clamped(start.year(), u32::from(month), day)?;
                if candidate >= start {
                    return Some(candidate);
                }
                clamped(start.year().checked_add(1)?, u32::from(month), day)
            }
        }
    }

    /// Every period date of the schedule, ascending. Bounded by `end_date`
    /// when set, otherwise unbounded: callers stop it themselves.
    pub fn occurrences(&self) -> impl Iterator<Item = NaiveDate> {
        Occurrences {
            schedule: *self,
            first: self.first_occurrence(),
            index: 0,
            done: self.validate().is_err(),
        }
    }

    /// All period dates up to and including `min(today, end_date)`.
    #[must_use]
    pub fn due_until(&self, today: NaiveDate) -> Vec<NaiveDate> {
        let limit = match self.end_date {
            Some(end) => end.min(today),
            None => today,
        };
        self.occurrences().take_while(|d| *d <= limit).collect()
    }

    /// Whether `date` is one of the period dates.
    #[must_use]
    pub fn is_occurrence(&self, date: NaiveDate) -> bool {
        if self.validate().is_err() || date < self.start_date {
            return false;
        }
        if self.end_date.is_some_and(|end| date > end) {
            return false;
        }
        let Some(first) = self.first_occurrence() else {
            return false;
        };
        if date < first {
            return false;
        }
        let interval = i64::from(self.interval);
        let steps = match self.frequency {
            Frequency::Daily => (date - first).num_days() / interval,
            Frequency::Weekly { .. } => (date - first).num_days() / (interval * 7),
            Frequency::Monthly { .. } => {
                let months = (i64::from(date.year()) - i64::from(first.year())) * 12
                    + i64::from(date.month())
                    - i64::from(first.month());
                months / interval
            }
            Frequency::Yearly { .. } => {
                (i64::from(date.year()) - i64::from(first.year())) / interval
            }
        };
        u64::try_from(steps)
            .ok()
            .and_then(|n| self.nth(first, n))
            .is_some_and(|found| found == date)
    }

    /// Period date `n` steps after `first`. The day of month is re-clamped
    /// from the originally requested day at every step, so a monthly 31 gives
    /// Jan 31, Feb 28, Mar 31 instead of drifting to 28.
    fn nth(&self, first: NaiveDate, n: u64) -> Option<NaiveDate> {
        let step = i64::from(self.interval).checked_mul(i64::try_from(n).ok()?)?;
        match self.frequency {
            Frequency::Daily => first.checked_add_signed(TimeDelta::try_days(step)?),
            Frequency::Weekly { .. } => {
                first.checked_add_signed(TimeDelta::try_days(step.checked_mul(7)?)?)
            }
            Frequency::Monthly { day } => {
                let total = i64::from(first.year())
                    .checked_mul(12)?
                    .checked_add(i64::from(first.month()) - 1)?
                    .checked_add(step)?;
                let year = i32::try_from(total.div_euclid(12)).ok()?;
                let month = u32::try_from(total.rem_euclid(12) + 1).ok()?;
                clamped(year, month, day)
            }
            Frequency::Yearly { month, day } => {
                let year = i32::try_from(i64::from(first.year()).checked_add(step)?).ok()?;
                clamped(year, u32::from(month), day)
            }
        }
    }
}

/// Iterator behind [`Schedule::occurrences`].
struct Occurrences {
    schedule: Schedule,
    first: Option<NaiveDate>,
    index: u64,
    done: bool,
}

impl Iterator for Occurrences {
    type Item = NaiveDate;

    fn next(&mut self) -> Option<NaiveDate> {
        if self.done {
            return None;
        }
        let first = self.first?;
        let next = self.schedule.nth(first, self.index);
        let Some(date) = next.filter(|d| self.schedule.end_date.is_none_or(|end| *d <= end)) else {
            self.done = true;
            return None;
        };
        match self.index.checked_add(1) {
            Some(next) => self.index = next,
            None => self.done = true,
        }
        Some(date)
    }
}

fn invalid(message: &str) -> DomainError {
    DomainError::InvalidCommand(message.to_string())
}

/// `day` clamped to the length of `month`, as a date.
fn clamped(year: i32, month: u32, day: u8) -> Option<NaiveDate> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let day = u32::from(day).min(days_in_month(year, month)?);
    NaiveDate::from_ymd_opt(year, month, day)
}

fn days_in_month(year: i32, month: u32) -> Option<u32> {
    let (next_year, next_month) = next_month(year, month)?;
    let first = NaiveDate::from_ymd_opt(next_year, next_month, 1)?;
    Some(first.pred_opt()?.day())
}

fn next_month(year: i32, month: u32) -> Option<(i32, u32)> {
    match month {
        1..=11 => Some((year, month + 1)),
        12 => Some((year.checked_add(1)?, 1)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Read-side views
// ---------------------------------------------------------------------------

/// A recurring template as the UI sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct RecurringView {
    pub id: Uuid,
    /// Only `Income` or `Expense`.
    pub kind: TransactionKind,
    /// Absolute, `> 0`.
    pub amount: i64,
    /// `None` = the only active wallet at execution time.
    pub wallet_id: Option<Uuid>,
    /// `None` = Unallocated.
    pub flow_id: Option<Uuid>,
    /// Free text, resolved at execution time.
    pub category: Option<String>,
    pub note: Option<String>,
    pub schedule: Schedule,
    pub enabled: bool,
    pub archived: bool,
}

/// A template with the period dates still waiting for a decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct PendingRecurring {
    pub template: RecurringView,
    /// Ascending, oldest missed period first; never empty.
    pub due: Vec<NaiveDate>,
}

/// How a period was handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Executed,
    Skipped,
}

impl RunOutcome {
    /// Tag stored in `recurring_runs.outcome`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Executed => "executed",
            Self::Skipped => "skipped",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "executed" => Ok(Self::Executed),
            "skipped" => Ok(Self::Skipped),
            other => Err(DomainError::Storage(format!("unknown outcome '{other}'"))),
        }
    }
}

/// One handled period of a template.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct RecurringRunView {
    pub period_date: NaiveDate,
    pub outcome: RunOutcome,
    /// Set only when the outcome is `Executed`.
    pub transaction_id: Option<Uuid>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn schedule(frequency: Frequency, interval: u32, start: NaiveDate) -> Schedule {
        Schedule {
            frequency,
            interval,
            start_date: start,
            end_date: None,
        }
    }

    fn take(s: &Schedule, n: usize) -> Vec<NaiveDate> {
        s.occurrences().take(n).collect()
    }

    #[test]
    fn daily_with_interval() {
        let s = schedule(Frequency::Daily, 3, date(2026, 1, 1));
        assert_eq!(
            take(&s, 4),
            vec![
                date(2026, 1, 1),
                date(2026, 1, 4),
                date(2026, 1, 7),
                date(2026, 1, 10)
            ]
        );
        assert!(s.is_occurrence(date(2026, 1, 7)));
        assert!(!s.is_occurrence(date(2026, 1, 8)));
        assert!(!s.is_occurrence(date(2025, 12, 29)));
    }

    #[test]
    fn daily_every_day() {
        let s = schedule(Frequency::Daily, 1, date(2026, 2, 27));
        assert_eq!(
            take(&s, 3),
            vec![date(2026, 2, 27), date(2026, 2, 28), date(2026, 3, 1)]
        );
    }

    #[test]
    fn weekly_aligns_to_the_weekday() {
        // 2026-01-01 is a Thursday; Monday = 1.
        let s = schedule(Frequency::Weekly { weekday: 1 }, 1, date(2026, 1, 1));
        assert_eq!(
            take(&s, 3),
            vec![date(2026, 1, 5), date(2026, 1, 12), date(2026, 1, 19)]
        );
        assert!(!s.is_occurrence(date(2026, 1, 1)));
        assert!(s.is_occurrence(date(2026, 1, 12)));

        // A start already on the target weekday fires that same day.
        let same = schedule(Frequency::Weekly { weekday: 4 }, 2, date(2026, 1, 1));
        assert_eq!(
            take(&same, 3),
            vec![date(2026, 1, 1), date(2026, 1, 15), date(2026, 1, 29)]
        );
        assert!(!same.is_occurrence(date(2026, 1, 8)));
    }

    #[test]
    fn monthly_clamps_without_drifting() {
        let s = schedule(Frequency::Monthly { day: 31 }, 1, date(2026, 1, 1));
        assert_eq!(
            take(&s, 4),
            vec![
                date(2026, 1, 31),
                date(2026, 2, 28),
                date(2026, 3, 31),
                date(2026, 4, 30)
            ]
        );
        assert!(s.is_occurrence(date(2026, 2, 28)));
        assert!(!s.is_occurrence(date(2026, 2, 27)));

        // Leap February.
        let leap = schedule(Frequency::Monthly { day: 31 }, 1, date(2028, 1, 1));
        assert_eq!(take(&leap, 2), vec![date(2028, 1, 31), date(2028, 2, 29)]);
    }

    #[test]
    fn monthly_starts_next_month_when_the_day_has_passed() {
        let s = schedule(Frequency::Monthly { day: 15 }, 1, date(2026, 1, 20));
        assert_eq!(take(&s, 2), vec![date(2026, 2, 15), date(2026, 3, 15)]);

        // The day of the start date itself counts.
        let today = schedule(Frequency::Monthly { day: 20 }, 1, date(2026, 1, 20));
        assert_eq!(take(&today, 1), vec![date(2026, 1, 20)]);
    }

    #[test]
    fn monthly_with_interval_crosses_the_year() {
        let s = schedule(Frequency::Monthly { day: 10 }, 5, date(2026, 10, 1));
        assert_eq!(
            take(&s, 3),
            vec![date(2026, 10, 10), date(2027, 3, 10), date(2027, 8, 10)]
        );
    }

    #[test]
    fn yearly_feb_29_falls_back_on_common_years() {
        let s = schedule(Frequency::Yearly { month: 2, day: 29 }, 1, date(2024, 1, 1));
        assert_eq!(
            take(&s, 5),
            vec![
                date(2024, 2, 29),
                date(2025, 2, 28),
                date(2026, 2, 28),
                date(2027, 2, 28),
                date(2028, 2, 29)
            ]
        );
        assert!(s.is_occurrence(date(2027, 2, 28)));
        assert!(!s.is_occurrence(date(2027, 3, 1)));
    }

    #[test]
    fn yearly_starts_next_year_when_the_date_has_passed() {
        let s = schedule(Frequency::Yearly { month: 3, day: 1 }, 2, date(2026, 6, 1));
        assert_eq!(take(&s, 2), vec![date(2027, 3, 1), date(2029, 3, 1)]);
    }

    #[test]
    fn end_date_bounds_the_iterator() {
        let s = Schedule {
            frequency: Frequency::Monthly { day: 1 },
            interval: 1,
            start_date: date(2026, 1, 1),
            end_date: Some(date(2026, 3, 15)),
        };
        assert_eq!(
            s.occurrences().collect::<Vec<_>>(),
            vec![date(2026, 1, 1), date(2026, 2, 1), date(2026, 3, 1)]
        );
        assert!(!s.is_occurrence(date(2026, 4, 1)));
    }

    #[test]
    fn due_until_stops_at_today_and_at_end_date() {
        let s = Schedule {
            frequency: Frequency::Daily,
            interval: 1,
            start_date: date(2026, 1, 1),
            end_date: Some(date(2026, 1, 3)),
        };
        assert_eq!(
            s.due_until(date(2026, 1, 2)),
            vec![date(2026, 1, 1), date(2026, 1, 2)]
        );
        assert_eq!(
            s.due_until(date(2026, 5, 1)),
            vec![date(2026, 1, 1), date(2026, 1, 2), date(2026, 1, 3)]
        );
        assert!(s.due_until(date(2025, 12, 31)).is_empty());
    }

    #[test]
    fn is_occurrence_agrees_with_the_iterator() {
        let schedules = [
            schedule(Frequency::Daily, 2, date(2026, 1, 1)),
            schedule(Frequency::Weekly { weekday: 7 }, 3, date(2026, 1, 1)),
            schedule(Frequency::Monthly { day: 30 }, 2, date(2026, 1, 5)),
            schedule(
                Frequency::Yearly { month: 12, day: 31 },
                1,
                date(2026, 1, 1),
            ),
        ];
        for s in schedules {
            let expected: Vec<_> = s.occurrences().take(30).collect();
            let last = expected.last().copied().unwrap();
            let mut day = s.start_date;
            while day <= last {
                assert_eq!(
                    s.is_occurrence(day),
                    expected.contains(&day),
                    "{:?} on {day}",
                    s.frequency
                );
                day = day.succ_opt().unwrap();
            }
        }
    }

    #[test]
    fn validation_rejects_malformed_schedules() {
        let bad = [
            schedule(Frequency::Daily, 0, date(2026, 1, 1)),
            schedule(Frequency::Weekly { weekday: 0 }, 1, date(2026, 1, 1)),
            schedule(Frequency::Weekly { weekday: 8 }, 1, date(2026, 1, 1)),
            schedule(Frequency::Monthly { day: 0 }, 1, date(2026, 1, 1)),
            schedule(Frequency::Monthly { day: 32 }, 1, date(2026, 1, 1)),
            schedule(Frequency::Yearly { month: 0, day: 1 }, 1, date(2026, 1, 1)),
            schedule(Frequency::Yearly { month: 13, day: 1 }, 1, date(2026, 1, 1)),
            schedule(Frequency::Yearly { month: 1, day: 32 }, 1, date(2026, 1, 1)),
            Schedule {
                frequency: Frequency::Daily,
                interval: 1,
                start_date: date(2026, 1, 2),
                end_date: Some(date(2026, 1, 1)),
            },
        ];
        for s in bad {
            assert!(
                matches!(s.validate(), Err(DomainError::InvalidCommand(_))),
                "{s:?} should be invalid"
            );
            assert!(s.occurrences().next().is_none(), "{s:?} should be empty");
            assert!(!s.is_occurrence(s.start_date));
        }
        let ok = Schedule {
            frequency: Frequency::Monthly { day: 31 },
            interval: 1,
            start_date: date(2026, 1, 1),
            end_date: Some(date(2026, 1, 1)),
        };
        assert!(ok.validate().is_ok());
    }

    #[test]
    fn schedule_round_trips_through_json() {
        let s = Schedule {
            frequency: Frequency::Yearly { month: 2, day: 29 },
            interval: 1,
            start_date: date(2026, 1, 1),
            end_date: Some(date(2030, 1, 1)),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"start_date\":\"2026-01-01\""));
        assert_eq!(serde_json::from_str::<Schedule>(&json).unwrap(), s);
    }
}
