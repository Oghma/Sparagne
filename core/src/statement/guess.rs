//! How a statement writes its dates and amounts, read from the values of the
//! columns the user picked, for a mapping that starts blank. A format is
//! guessed only when it reads every value: one value it refuses and the guess
//! says nothing, so the user's own choice, or the default, is what the
//! preview shows.

use chrono_tz::Tz;

use super::amount::parse_amount;
use super::csv;
use super::dates::parse_date;
use super::{StatementDateFormat, StatementFormatGuess, plan};

const GUESS_ROWS: usize = 50;

/// The date format and the decimal mark that read every non-empty value of
/// the first 50 data rows of `date_column` and `amount_column` (named as a
/// mapping names them), for a mapping that starts blank. Each is `None` when
/// its column is not named, not in the header or empty, or when no format
/// reads all its values; a delimiter that is not one character guesses
/// nothing.
#[must_use]
pub fn guess(
    text: &str,
    delimiter: &str,
    date_column: Option<&str>,
    amount_column: Option<&str>,
) -> StatementFormatGuess {
    let Ok(delimiter) = plan::delimiter(delimiter) else {
        return StatementFormatGuess::default();
    };
    let mut records = csv::records(text, delimiter);
    let Some(header) = records.next() else {
        return StatementFormatGuess::default();
    };
    let index = |name: Option<&str>| csv::column(&header.fields, name?);
    let (date, amount) = (index(date_column), index(amount_column));
    let rows: Vec<Vec<String>> = records
        .take(GUESS_ROWS)
        .map(|record| record.fields)
        .collect();
    let values = |column: Option<usize>| -> Vec<&str> {
        column.map_or_else(Vec::new, |column| {
            rows.iter()
                .filter_map(|fields| fields.get(column))
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .collect()
        })
    };
    StatementFormatGuess {
        date_format: date_format(&values(date)),
        decimal_comma: decimal_comma(&values(amount)),
    }
}

/// The first format that reads every value. The UTC one is tried only when
/// every value says UTC (it also reads a bare local time as one); a time
/// rules out the formats of a day. Day-first comes before month-first: a
/// first number over 12 rules month-first out, a second one over 12 rules
/// day-first out, and with neither the date is read as Italian banks write
/// it.
fn date_format(values: &[&str]) -> Option<StatementDateFormat> {
    if values.is_empty() {
        return None;
    }
    let says_utc = values
        .iter()
        .all(|value| value.ends_with('Z') || value.ends_with("UTC") || value.ends_with("utc"));
    let candidates = [
        StatementDateFormat::DateTimeUtc,
        StatementDateFormat::IsoDateTime,
        StatementDateFormat::IsoDate,
        StatementDateFormat::DayMonthYear,
        StatementDateFormat::MonthDayYear,
    ];
    candidates
        .into_iter()
        .filter(|format| says_utc || *format != StatementDateFormat::DateTimeUtc)
        .find(|format| {
            values
                .iter()
                .all(|value| parse_date(value, format, Tz::UTC).is_ok())
        })
}

/// `false` when a decimal point reads every value, `true` when a decimal
/// comma does. Both read every value only when none has a separator, and
/// whole numbers say nothing about the mark; neither, when a value has a
/// thousands separator (`1.850,00`) or is no amount.
fn decimal_comma(values: &[&str]) -> Option<bool> {
    if values.is_empty() {
        return None;
    }
    // The decimals kept do not matter: only whether the text reads.
    let reads = |comma: bool| {
        values
            .iter()
            .all(|value| parse_amount(value, comma, 2).is_ok())
    };
    match (reads(false), reads(true)) {
        (true, false) => Some(false),
        (false, true) => Some(true),
        _ => None,
    }
}
