//! Exact decimal amounts, read from the text and never through a float.
//!
//! Unlike [`crate::Money::parse_major`], which refuses what it cannot hold,
//! a statement amount with more decimals than the currency is rounded half
//! away from zero: card exports write top-ups with a dozen decimals.

/// An amount of the statement in minor units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Amount {
    /// Signed as the file wrote it.
    pub minor: i64,
    /// The decimals past the currency's changed the value.
    pub rounded: bool,
}

/// Parses `[+|-] digits [ sep digits ]`, where `sep` is `,` when
/// `decimal_comma` and `.` otherwise. The other separator is refused rather
/// than guessed as a thousands separator. `minor_units` is the number of
/// decimals the currency keeps.
pub(super) fn parse_amount(
    text: &str,
    decimal_comma: bool,
    minor_units: u8,
) -> Result<Amount, String> {
    let trimmed = text.trim();
    let not_an_amount = || format!("'{trimmed}' is not an amount");
    if trimmed.is_empty() {
        return Err("the amount is empty".to_string());
    }
    // U+2212 is the minus sign some spreadsheets write instead of a hyphen.
    let (negative, rest) = if let Some(rest) = trimmed.strip_prefix(['-', '\u{2212}']) {
        (true, rest)
    } else if let Some(rest) = trimmed.strip_prefix('+') {
        (false, rest)
    } else {
        (false, trimmed)
    };
    let rest = rest.trim_start();
    let (separator, other) = if decimal_comma {
        (',', '.')
    } else {
        ('.', ',')
    };
    if rest.contains(other) {
        return Err(format!(
            "'{trimmed}' has a '{other}': thousands separators are not read"
        ));
    }
    let (whole, fraction) = rest.split_once(separator).unwrap_or((rest, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if (whole.is_empty() && fraction.is_empty()) || !digits(whole) || !digits(fraction) {
        return Err(not_an_amount());
    }

    let too_large = || format!("'{trimmed}' is too large");
    let keep = usize::from(minor_units);
    let (kept, dropped) = fraction.split_at(fraction.len().min(keep));
    let mut magnitude: i64 = 0;
    for byte in whole
        .bytes()
        .chain(kept.bytes())
        .chain(std::iter::repeat_n(b'0', keep - kept.len()))
    {
        magnitude = magnitude
            .checked_mul(10)
            .and_then(|m| m.checked_add(i64::from(byte - b'0')))
            .ok_or_else(too_large)?;
    }
    if dropped.bytes().next().is_some_and(|b| b >= b'5') {
        magnitude = magnitude.checked_add(1).ok_or_else(too_large)?;
    }
    Ok(Amount {
        minor: if negative { -magnitude } else { magnitude },
        rounded: dropped.bytes().any(|b| b != b'0'),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn eur(text: &str) -> (i64, bool) {
        let amount = parse_amount(text, false, 2).unwrap();
        (amount.minor, amount.rounded)
    }

    #[test]
    fn exact_amounts_keep_their_sign() {
        assert_eq!(eur("12.34"), (1234, false));
        assert_eq!(eur("-24.99"), (-2499, false));
        assert_eq!(eur("+3"), (300, false));
        assert_eq!(eur(" 7.5 "), (750, false));
        assert_eq!(eur("- 0.01"), (-1, false));
        assert_eq!(eur("\u{2212}4.20"), (-420, false));
        assert_eq!(eur(".5"), (50, false));
        assert_eq!(eur("0"), (0, false));
    }

    #[test]
    fn extra_decimals_round_half_away_from_zero() {
        assert_eq!(eur("126.017164395008"), (12602, true));
        assert_eq!(eur("0.005"), (1, true));
        assert_eq!(eur("-0.005"), (-1, true));
        assert_eq!(eur("0.00499"), (0, true));
        assert_eq!(eur("-2.994999"), (-299, true));
        assert_eq!(eur("9.999"), (1000, true));
    }

    #[test]
    fn trailing_zeros_are_not_rounding() {
        assert_eq!(eur("12.500"), (1250, false));
        assert_eq!(eur("3.000000000000"), (300, false));
    }

    #[test]
    fn the_decimal_comma_swaps_the_separators() {
        let amount = parse_amount("-1850,5", true, 2).unwrap();
        assert_eq!(amount.minor, -185_050);
        assert!(parse_amount("1.850,50", true, 2).is_err());
        assert!(parse_amount("1,850.50", false, 2).is_err());
        assert!(parse_amount("12,50", false, 2).is_err());
    }

    #[test]
    fn garbage_is_refused() {
        for text in [
            "", "-", "abc", "1.2.3", "12 EUR", "€12", "1e3", "--1", "+-1", ".", "1 000",
        ] {
            assert!(parse_amount(text, false, 2).is_err(), "{text:?}");
        }
        assert!(parse_amount("99999999999999999999", false, 2).is_err());
    }

    #[test]
    fn currencies_without_decimals_round_to_units() {
        let amount = parse_amount("1234.5", false, 0).unwrap();
        assert_eq!((amount.minor, amount.rounded), (1235, true));
    }
}
