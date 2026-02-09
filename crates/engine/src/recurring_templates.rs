//! Recurring transaction templates.
//!
//! A recurring template defines a transaction (income or expense) that repeats
//! on a schedule. Templates require explicit user approval before each
//! occurrence is executed.

use chrono::{Datelike, NaiveDate};
use sea_orm::entity::{ActiveValue, prelude::*};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{EngineError, ResultEngine, TransactionKind};

/// How often a recurring template repeats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "Text")]
#[serde(rename_all = "snake_case")]
pub enum RecurrenceFrequency {
    #[sea_orm(string_value = "daily")]
    Daily,
    #[sea_orm(string_value = "weekly")]
    Weekly,
    #[sea_orm(string_value = "monthly")]
    Monthly,
    #[sea_orm(string_value = "yearly")]
    Yearly,
}

/// Domain model for a recurring template.
#[derive(Clone, Debug)]
pub struct RecurringTemplate {
    pub id: Uuid,
    pub vault_id: Uuid,
    pub kind: TransactionKind,
    pub amount_minor: i64,
    pub wallet_id: Option<Uuid>,
    pub flow_id: Option<Uuid>,
    pub category_id: Uuid,
    pub note: Option<String>,
    pub created_by: String,
    pub frequency: RecurrenceFrequency,
    pub day_of_period: i32,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub enabled: bool,
    pub last_executed_date: Option<NaiveDate>,
    pub created_at: String,
    pub archived_at: Option<String>,
}

/// A template that is due for execution.
#[derive(Clone, Debug)]
pub struct PendingRecurring {
    pub template: RecurringTemplate,
    /// The date this occurrence represents.
    pub period_date: NaiveDate,
}

/// Validate `day_of_period` for the given frequency.
pub(crate) fn validate_day_of_period(
    frequency: RecurrenceFrequency,
    day_of_period: i32,
) -> ResultEngine<()> {
    match frequency {
        RecurrenceFrequency::Daily => {
            // Ignored for daily; accept 0.
        }
        RecurrenceFrequency::Weekly => {
            if !(1..=7).contains(&day_of_period) {
                return Err(EngineError::InvalidRecurring(
                    "day_of_period must be 1..7 (ISO weekday) for weekly frequency".to_string(),
                ));
            }
        }
        RecurrenceFrequency::Monthly => {
            if !(1..=28).contains(&day_of_period) {
                return Err(EngineError::InvalidRecurring(
                    "day_of_period must be 1..28 for monthly frequency".to_string(),
                ));
            }
        }
        RecurrenceFrequency::Yearly => {
            let month = day_of_period / 100;
            let day = day_of_period % 100;
            if !(1..=12).contains(&month) || !(1..=28).contains(&day) {
                return Err(EngineError::InvalidRecurring(
                    "day_of_period must be MMDD (month 1-12, day 1-28) for yearly frequency"
                        .to_string(),
                ));
            }
        }
    }
    Ok(())
}

/// Compute the current period date for a given frequency and reference date.
///
/// This is a pure function used for determining whether a template is "due".
pub fn compute_current_period_date(
    frequency: RecurrenceFrequency,
    day_of_period: i32,
    as_of_date: NaiveDate,
) -> NaiveDate {
    match frequency {
        RecurrenceFrequency::Daily => as_of_date,
        RecurrenceFrequency::Weekly => {
            let current_weekday = as_of_date.weekday().number_from_monday() as i32;
            let diff = current_weekday - day_of_period;
            if diff >= 0 {
                as_of_date - chrono::Duration::days(i64::from(diff))
            } else {
                as_of_date - chrono::Duration::days(i64::from(diff + 7))
            }
        }
        RecurrenceFrequency::Monthly => {
            let year = as_of_date.year();
            let month = as_of_date.month();
            let day = day_of_period.min(28) as u32;
            if as_of_date.day() >= day {
                NaiveDate::from_ymd_opt(year, month, day).unwrap_or(as_of_date)
            } else if month == 1 {
                NaiveDate::from_ymd_opt(year - 1, 12, day).unwrap_or(as_of_date)
            } else {
                NaiveDate::from_ymd_opt(year, month - 1, day).unwrap_or(as_of_date)
            }
        }
        RecurrenceFrequency::Yearly => {
            let month = (day_of_period / 100) as u32;
            let day = (day_of_period % 100).min(28) as u32;
            let year = as_of_date.year();
            let this_year_date =
                NaiveDate::from_ymd_opt(year, month, day).unwrap_or(as_of_date);
            if as_of_date >= this_year_date {
                this_year_date
            } else {
                NaiveDate::from_ymd_opt(year - 1, month, day).unwrap_or(as_of_date)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SeaORM entity
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "recurring_templates")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub vault_id: Uuid,
    pub kind: TransactionKind,
    pub amount_minor: i64,
    pub wallet_id: Option<Uuid>,
    pub flow_id: Option<Uuid>,
    pub category_id: Uuid,
    pub note: Option<String>,
    pub created_by: String,
    pub frequency: RecurrenceFrequency,
    pub day_of_period: i32,
    pub start_date: String,
    pub end_date: Option<String>,
    pub enabled: bool,
    pub last_executed_date: Option<String>,
    pub created_at: String,
    pub archived_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::vault::Entity",
        from = "Column::VaultId",
        to = "super::vault::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    Vaults,
}

impl Related<super::vault::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Vaults.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

impl TryFrom<Model> for RecurringTemplate {
    type Error = EngineError;

    fn try_from(model: Model) -> ResultEngine<Self> {
        let start_date = NaiveDate::parse_from_str(&model.start_date, "%Y-%m-%d")
            .map_err(|e| EngineError::InvalidRecurring(format!("bad start_date: {e}")))?;
        let end_date = model
            .end_date
            .as_deref()
            .map(|s| {
                NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .map_err(|e| EngineError::InvalidRecurring(format!("bad end_date: {e}")))
            })
            .transpose()?;
        let last_executed_date = model
            .last_executed_date
            .as_deref()
            .map(|s| {
                NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| {
                    EngineError::InvalidRecurring(format!("bad last_executed_date: {e}"))
                })
            })
            .transpose()?;

        Ok(Self {
            id: model.id,
            vault_id: model.vault_id,
            kind: model.kind,
            amount_minor: model.amount_minor,
            wallet_id: model.wallet_id,
            flow_id: model.flow_id,
            category_id: model.category_id,
            note: model.note,
            created_by: model.created_by,
            frequency: model.frequency,
            day_of_period: model.day_of_period,
            start_date,
            end_date,
            enabled: model.enabled,
            last_executed_date,
            created_at: model.created_at,
            archived_at: model.archived_at,
        })
    }
}

impl From<&RecurringTemplate> for ActiveModel {
    fn from(t: &RecurringTemplate) -> Self {
        Self {
            id: ActiveValue::Set(t.id),
            vault_id: ActiveValue::Set(t.vault_id),
            kind: ActiveValue::Set(t.kind),
            amount_minor: ActiveValue::Set(t.amount_minor),
            wallet_id: ActiveValue::Set(t.wallet_id),
            flow_id: ActiveValue::Set(t.flow_id),
            category_id: ActiveValue::Set(t.category_id),
            note: ActiveValue::Set(t.note.clone()),
            created_by: ActiveValue::Set(t.created_by.clone()),
            frequency: ActiveValue::Set(t.frequency),
            day_of_period: ActiveValue::Set(t.day_of_period),
            start_date: ActiveValue::Set(t.start_date.format("%Y-%m-%d").to_string()),
            end_date: ActiveValue::Set(t.end_date.map(|d| d.format("%Y-%m-%d").to_string())),
            enabled: ActiveValue::Set(t.enabled),
            last_executed_date: ActiveValue::Set(
                t.last_executed_date
                    .map(|d| d.format("%Y-%m-%d").to_string()),
            ),
            created_at: ActiveValue::Set(t.created_at.clone()),
            archived_at: ActiveValue::Set(t.archived_at.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn daily_period_date_is_as_of() {
        let date = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
        assert_eq!(
            compute_current_period_date(RecurrenceFrequency::Daily, 0, date),
            date,
        );
    }

    #[test]
    fn weekly_same_weekday() {
        // 2026-02-10 is a Tuesday (weekday 2).
        let date = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
        assert_eq!(
            compute_current_period_date(RecurrenceFrequency::Weekly, 2, date),
            date,
        );
    }

    #[test]
    fn weekly_earlier_weekday() {
        // 2026-02-10 is Tuesday (2), asking for Monday (1) => 2026-02-09.
        let date = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
        assert_eq!(
            compute_current_period_date(RecurrenceFrequency::Weekly, 1, date),
            NaiveDate::from_ymd_opt(2026, 2, 9).unwrap(),
        );
    }

    #[test]
    fn weekly_later_weekday_wraps() {
        // 2026-02-10 is Tuesday (2), asking for Friday (5) => previous Friday = 2026-02-06.
        let date = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
        assert_eq!(
            compute_current_period_date(RecurrenceFrequency::Weekly, 5, date),
            NaiveDate::from_ymd_opt(2026, 2, 6).unwrap(),
        );
    }

    #[test]
    fn monthly_same_or_later_day() {
        let date = NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        assert_eq!(
            compute_current_period_date(RecurrenceFrequency::Monthly, 1, date),
            NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
        );
    }

    #[test]
    fn monthly_earlier_day_goes_to_previous_month() {
        let date = NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
        assert_eq!(
            compute_current_period_date(RecurrenceFrequency::Monthly, 15, date),
            NaiveDate::from_ymd_opt(2026, 2, 15).unwrap(),
        );
    }

    #[test]
    fn monthly_january_wraps_to_december() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        assert_eq!(
            compute_current_period_date(RecurrenceFrequency::Monthly, 15, date),
            NaiveDate::from_ymd_opt(2025, 12, 15).unwrap(),
        );
    }

    #[test]
    fn yearly_same_or_later_date() {
        // Jan 15 = 115 in MMDD encoding; as_of = Feb 10 2026.
        let date = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
        assert_eq!(
            compute_current_period_date(RecurrenceFrequency::Yearly, 115, date),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        );
    }

    #[test]
    fn yearly_earlier_date_goes_to_previous_year() {
        // Dec 31 = 1231 in MMDD encoding; as_of = Feb 10 2026.
        let date = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
        assert_eq!(
            compute_current_period_date(RecurrenceFrequency::Yearly, 1231, date),
            NaiveDate::from_ymd_opt(2025, 12, 28).unwrap(), // capped to 28
        );
    }

    #[test]
    fn validate_weekly_day_of_period() {
        assert!(validate_day_of_period(RecurrenceFrequency::Weekly, 1).is_ok());
        assert!(validate_day_of_period(RecurrenceFrequency::Weekly, 7).is_ok());
        assert!(validate_day_of_period(RecurrenceFrequency::Weekly, 0).is_err());
        assert!(validate_day_of_period(RecurrenceFrequency::Weekly, 8).is_err());
    }

    #[test]
    fn validate_monthly_day_of_period() {
        assert!(validate_day_of_period(RecurrenceFrequency::Monthly, 1).is_ok());
        assert!(validate_day_of_period(RecurrenceFrequency::Monthly, 28).is_ok());
        assert!(validate_day_of_period(RecurrenceFrequency::Monthly, 0).is_err());
        assert!(validate_day_of_period(RecurrenceFrequency::Monthly, 29).is_err());
    }

    #[test]
    fn validate_yearly_day_of_period() {
        assert!(validate_day_of_period(RecurrenceFrequency::Yearly, 115).is_ok());
        assert!(validate_day_of_period(RecurrenceFrequency::Yearly, 1228).is_ok());
        assert!(validate_day_of_period(RecurrenceFrequency::Yearly, 1301).is_err());
        assert!(validate_day_of_period(RecurrenceFrequency::Yearly, 229).is_err());
    }
}
