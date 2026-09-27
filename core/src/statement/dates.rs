//! Statement dates, turned into the instant and offset a transaction keeps.
//!
//! A UTC instant is shown in the user's timezone; a written offset is kept as
//! it is; a local time without an offset is read in the user's timezone; a
//! date without a time is placed at local noon, far from both midnights, so
//! the accounting day never shifts.

use chrono::{
    DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat, TimeDelta, TimeZone,
    Utc,
};
use chrono_tz::Tz;

use super::StatementDateFormat;

/// Offset date-times [`StatementDateFormat::IsoDateTime`] reads besides
/// RFC 3339.
const OFFSET_FORMATS: [&str; 4] = [
    "%Y-%m-%dT%H:%M:%S%.f%:z",
    "%Y-%m-%d %H:%M:%S%.f%:z",
    "%Y-%m-%dT%H:%M:%S%.f%z",
    "%Y-%m-%d %H:%M:%S%.f%z",
];

/// Date-times without an offset.
const LOCAL_FORMATS: [&str; 4] = [
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%dT%H:%M",
    "%Y-%m-%d %H:%M",
];

/// Where a date without a time is placed.
const NOON: NaiveTime = NaiveTime::from_hms_opt(12, 0, 0).expect("noon is a valid time");

/// A date of the statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StatementDate {
    /// When the transaction happened, with the offset it keeps.
    pub at: DateTime<FixedOffset>,
    /// What the file says, for the row's id: the UTC instant when the text
    /// fixes one (`2026-09-16T08:54:40Z`), else the wall-clock time
    /// (`2026-09-16T10:54:40`) or the date (`2026-09-16`). It does not depend
    /// on the timezone of the import, so importing the same file from another
    /// timezone still finds the rows already imported.
    pub stamp: String,
}

/// What the text of a date fixes.
enum Written {
    /// An instant shown in the user's timezone.
    Utc(DateTime<Utc>),
    /// An instant with the offset it was written with.
    Offset(DateTime<FixedOffset>),
    /// A wall-clock time in the user's timezone.
    Local(NaiveDateTime),
    /// A day, placed at local noon.
    Day(NaiveDate),
}

/// Reads `text` as `format`; the error is a message for the preview.
pub(super) fn parse_date(
    text: &str,
    format: &StatementDateFormat,
    tz: Tz,
) -> Result<StatementDate, String> {
    let text = text.trim();
    let written = match format {
        StatementDateFormat::DateTimeUtc => utc(text).map(Written::Utc),
        StatementDateFormat::IsoDate => NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .ok()
            .map(Written::Day),
        StatementDateFormat::IsoDateTime => iso_date_time(text),
        StatementDateFormat::DayMonthYear => {
            numeric_date(text).and_then(|(day, month, year)| day_of(year, month, day))
        }
        StatementDateFormat::MonthDayYear => {
            numeric_date(text).and_then(|(month, day, year)| day_of(year, month, day))
        }
        StatementDateFormat::Custom { pattern } => custom(text, pattern),
    };
    let utc_stamp = |instant: DateTime<Utc>| instant.to_rfc3339_opts(SecondsFormat::Secs, true);
    match written {
        Some(Written::Utc(instant)) => Ok(StatementDate {
            at: in_zone(instant, tz),
            stamp: utc_stamp(instant),
        }),
        Some(Written::Offset(at)) => Ok(StatementDate {
            at,
            stamp: utc_stamp(at.with_timezone(&Utc)),
        }),
        Some(Written::Local(naive)) => Ok(StatementDate {
            at: local(naive, tz),
            stamp: naive.format("%Y-%m-%dT%H:%M:%S").to_string(),
        }),
        Some(Written::Day(date)) => Ok(StatementDate {
            at: local(date.and_time(NOON), tz),
            stamp: date.format("%Y-%m-%d").to_string(),
        }),
        None if text.is_empty() => Err("the date is empty".to_string()),
        None => Err(format!("'{text}' is not a date in the chosen format")),
    }
}

/// `2026-09-16 08:54:40 UTC`; the ` UTC` or `Z` suffix may be missing, an
/// explicit offset is honoured.
fn utc(text: &str) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Some(parsed.with_timezone(&Utc));
    }
    let bare = text
        .strip_suffix("UTC")
        .or_else(|| text.strip_suffix("utc"))
        .or_else(|| text.strip_suffix('Z'))
        .unwrap_or(text)
        .trim_end();
    LOCAL_FORMATS
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(bare, format).ok())
        .map(|naive| Utc.from_utc_datetime(&naive))
}

fn iso_date_time(text: &str) -> Option<Written> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Some(Written::Offset(parsed));
    }
    if let Some(parsed) = OFFSET_FORMATS
        .iter()
        .find_map(|format| DateTime::parse_from_str(text, format).ok())
    {
        return Some(Written::Offset(parsed));
    }
    LOCAL_FORMATS
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(text, format).ok())
        .map(Written::Local)
}

/// A chrono pattern, tried with an offset, then as a local date-time, then
/// as a date.
fn custom(text: &str, pattern: &str) -> Option<Written> {
    if pattern.trim().is_empty() {
        return None;
    }
    if let Ok(parsed) = DateTime::parse_from_str(text, pattern) {
        return Some(Written::Offset(parsed));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(text, pattern) {
        return Some(Written::Local(naive));
    }
    NaiveDate::parse_from_str(text, pattern)
        .ok()
        .map(Written::Day)
}

/// Three numbers split by the same `/`, `.` or `-`, in file order. A
/// two-digit year is in this century.
fn numeric_date(text: &str) -> Option<(u32, u32, i32)> {
    let separator = text.chars().find(|c| !c.is_ascii_digit())?;
    if !matches!(separator, '/' | '.' | '-') {
        return None;
    }
    let mut parts = text.split(separator);
    let (first, second, year) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let number = |part: &str, widths: &[usize]| {
        if widths.contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit()) {
            part.parse::<u32>().ok()
        } else {
            None
        }
    };
    let year = number(year, &[2, 4])?;
    let year = if year < 100 { 2000 + year } else { year };
    Some((
        number(first, &[1, 2])?,
        number(second, &[1, 2])?,
        i32::try_from(year).ok()?,
    ))
}

fn day_of(year: i32, month: u32, day: u32) -> Option<Written> {
    NaiveDate::from_ymd_opt(year, month, day).map(Written::Day)
}

fn in_zone(instant: DateTime<Utc>, tz: Tz) -> DateTime<FixedOffset> {
    instant.with_timezone(&tz).fixed_offset()
}

/// A wall-clock time in `tz`. The repeated hour of a DST change resolves to
/// its first occurrence; the skipped hour moves forward by an hour, which is
/// what the clock showed.
fn local(naive: NaiveDateTime, tz: Tz) -> DateTime<FixedOffset> {
    tz.from_local_datetime(&naive)
        .earliest()
        .or_else(|| {
            tz.from_local_datetime(&(naive + TimeDelta::hours(1)))
                .earliest()
        })
        .map_or_else(
            || in_zone(Utc.from_utc_datetime(&naive), tz),
            |dt| dt.fixed_offset(),
        )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    const ROME: Tz = chrono_tz::Europe::Rome;

    fn parse(text: &str, format: &StatementDateFormat) -> String {
        parse_date(text, format, ROME).unwrap().at.to_rfc3339()
    }

    fn stamp(text: &str, format: &StatementDateFormat, tz: Tz) -> String {
        parse_date(text, format, tz).unwrap().stamp
    }

    #[test]
    fn the_stamp_is_what_the_file_says_whatever_the_timezone() {
        let new_york = chrono_tz::America::New_York;
        for tz in [ROME, new_york] {
            assert_eq!(
                stamp(
                    "2026-09-16 08:54:40 UTC",
                    &StatementDateFormat::DateTimeUtc,
                    tz
                ),
                "2026-09-16T08:54:40Z"
            );
            assert_eq!(
                stamp(
                    "2026-09-16T10:54:40.5+02:00",
                    &StatementDateFormat::IsoDateTime,
                    tz
                ),
                "2026-09-16T08:54:40Z"
            );
            assert_eq!(
                stamp("2026-09-16T10:54:40", &StatementDateFormat::IsoDateTime, tz),
                "2026-09-16T10:54:40"
            );
            assert_eq!(
                stamp("16/09/2026", &StatementDateFormat::DayMonthYear, tz),
                "2026-09-16"
            );
        }
        let rome = parse_date("16/09/2026", &StatementDateFormat::DayMonthYear, ROME).unwrap();
        let ny = parse_date("16/09/2026", &StatementDateFormat::DayMonthYear, new_york).unwrap();
        assert_ne!(rome.at, ny.at);
    }

    #[test]
    fn utc_instants_are_shown_in_the_timezone() {
        let format = StatementDateFormat::DateTimeUtc;
        assert_eq!(
            parse("2026-09-16 08:54:40 UTC", &format),
            "2026-09-16T10:54:40+02:00"
        );
        assert_eq!(
            parse("2026-03-31 23:30:00 UTC", &format),
            "2026-04-01T01:30:00+02:00"
        );
        assert_eq!(
            parse("2026-01-10 23:30:00", &format),
            "2026-01-11T00:30:00+01:00"
        );
        assert_eq!(
            parse("2026-01-10T23:30:00.250Z", &format),
            "2026-01-11T00:30:00.250+01:00"
        );
        assert!(parse_date("16/09/2026", &format, ROME).is_err());
    }

    #[test]
    fn iso_date_times_keep_a_written_offset() {
        let format = StatementDateFormat::IsoDateTime;
        assert_eq!(
            parse("2026-09-16T08:54:40-04:00", &format),
            "2026-09-16T08:54:40-04:00"
        );
        assert_eq!(
            parse("2026-09-16 08:54:40+0530", &format),
            "2026-09-16T08:54:40+05:30"
        );
        assert_eq!(
            parse("2026-09-16T08:54:40", &format),
            "2026-09-16T08:54:40+02:00"
        );
        assert_eq!(
            parse("2026-12-16 08:54", &format),
            "2026-12-16T08:54:00+01:00"
        );
    }

    #[test]
    fn dates_without_a_time_sit_at_local_noon() {
        let noon_rome = "2026-09-16T12:00:00+02:00";
        assert_eq!(
            parse("2026-09-16", &StatementDateFormat::IsoDate),
            noon_rome
        );
        for text in ["16/09/2026", "16.09.2026", "16-09-2026", "16/9/26"] {
            assert_eq!(parse(text, &StatementDateFormat::DayMonthYear), noon_rome);
        }
        assert_eq!(
            parse("09/16/2026", &StatementDateFormat::MonthDayYear),
            noon_rome
        );
        assert_eq!(
            parse("01/02/2026", &StatementDateFormat::DayMonthYear),
            "2026-02-01T12:00:00+01:00"
        );
    }

    #[test]
    fn impossible_or_malformed_dates_are_refused() {
        for text in [
            "31/02/2026",
            "16/09",
            "16/09/2026/1",
            "16 09 2026",
            "16/09-2026",
            "",
            "1/2/203",
        ] {
            assert!(
                parse_date(text, &StatementDateFormat::DayMonthYear, ROME).is_err(),
                "{text:?}"
            );
        }
        assert!(parse_date("16/09/2026", &StatementDateFormat::MonthDayYear, ROME).is_err());
        assert!(parse_date("2026-09-16T08:00", &StatementDateFormat::IsoDate, ROME).is_err());
    }

    #[test]
    fn custom_patterns_go_through_chrono() {
        let pattern = |p: &str| StatementDateFormat::Custom {
            pattern: p.to_string(),
        };
        assert_eq!(
            parse("16 Sep 2026 07:05", &pattern("%d %b %Y %H:%M")),
            "2026-09-16T07:05:00+02:00"
        );
        assert_eq!(
            parse("20260916", &pattern("%Y%m%d")),
            "2026-09-16T12:00:00+02:00"
        );
        assert_eq!(
            parse("16.09.2026 07:05 +0000", &pattern("%d.%m.%Y %H:%M %z")),
            "2026-09-16T07:05:00+00:00"
        );
        assert!(parse_date("2026-09-16", &pattern(""), ROME).is_err());
    }

    #[test]
    fn the_skipped_hour_moves_forward_and_the_repeated_one_takes_the_first() {
        let format = StatementDateFormat::IsoDateTime;
        assert_eq!(
            parse("2026-03-29T02:30:00", &format),
            "2026-03-29T03:30:00+02:00"
        );
        assert_eq!(
            parse("2026-10-25T02:30:00", &format),
            "2026-10-25T02:30:00+02:00"
        );
    }
}
