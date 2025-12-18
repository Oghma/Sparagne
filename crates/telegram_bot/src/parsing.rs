use engine::{Currency, Money};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuickKind {
    Income,
    Expense,
    Refund,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct QuickAdd {
    pub kind: QuickKind,
    pub amount_minor: i64,
    pub category: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ParseError {
    #[error("importo non valido")]
    InvalidAmount,
    #[error("troppi tag: massimo 1")]
    TooManyTags,
    #[error("testo vuoto")]
    Empty,
}

fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parses a quick-add message into a draft transaction.
///
/// Rules (v2):
/// - `12.50 ...` and `-12.50 ...` => Expense
/// - `+12.50 ...` => Income
/// - `r 12.50 ...` => Refund
/// - optional `#tag` (max 1) => category (case-insensitive)
pub(crate) fn parse_quick_add(input: &str, currency: Currency) -> Result<QuickAdd, ParseError> {
    let trimmed = collapse_whitespace(input.trim());
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }

    let (kind, rest) = if let Some(rest) = trimmed.strip_prefix("r ") {
        (QuickKind::Refund, rest)
    } else if let Some(rest) = trimmed.strip_prefix("R ") {
        (QuickKind::Refund, rest)
    } else if trimmed.starts_with('+') {
        (QuickKind::Income, trimmed.as_str())
    } else {
        (QuickKind::Expense, trimmed.as_str())
    };

    let mut parts = rest.splitn(2, ' ');
    let amount_str = parts.next().ok_or(ParseError::InvalidAmount)?;
    let tail = parts.next().unwrap_or("").trim();

    let amount = Money::parse_major(amount_str, currency).map_err(|_| ParseError::InvalidAmount)?;
    let amount_minor =
        i64::try_from(amount.minor().unsigned_abs()).map_err(|_| ParseError::InvalidAmount)?;
    if amount_minor <= 0 {
        return Err(ParseError::InvalidAmount);
    }

    let mut tag: Option<String> = None;
    let mut note_tokens: Vec<&str> = Vec::new();
    for token in tail.split_whitespace() {
        if let Some(raw) = token.strip_prefix('#') {
            if raw.is_empty() {
                note_tokens.push(token);
                continue;
            }
            if tag.is_some() {
                return Err(ParseError::TooManyTags);
            }
            tag = Some(raw.to_ascii_lowercase());
        } else {
            note_tokens.push(token);
        }
    }

    let note = collapse_whitespace(&note_tokens.join(" "));
    let note = (!note.is_empty()).then_some(note);

    Ok(QuickAdd {
        kind,
        amount_minor,
        category: tag,
        note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expense_default_without_sign() {
        let parsed = parse_quick_add("12.50 bar", Currency::Eur).unwrap();
        assert_eq!(parsed.kind, QuickKind::Expense);
        assert_eq!(parsed.amount_minor, 1250);
    }

    #[test]
    fn expense_with_minus_sign() {
        let parsed = parse_quick_add("-12.50 bar", Currency::Eur).unwrap();
        assert_eq!(parsed.kind, QuickKind::Expense);
        assert_eq!(parsed.amount_minor, 1250);
    }

    #[test]
    fn income_with_plus_sign() {
        let parsed = parse_quick_add("+1000 stipendio", Currency::Eur).unwrap();
        assert_eq!(parsed.kind, QuickKind::Income);
        assert_eq!(parsed.amount_minor, 100_000);
    }

    #[test]
    fn refund_prefix_r() {
        let parsed = parse_quick_add("r 5.20 amazon", Currency::Eur).unwrap();
        assert_eq!(parsed.kind, QuickKind::Refund);
        assert_eq!(parsed.amount_minor, 520);
    }

    #[test]
    fn tag_sets_category_and_is_removed_from_note() {
        let parsed = parse_quick_add("12.50 bar #Food caffè", Currency::Eur).unwrap();
        assert_eq!(parsed.category.as_deref(), Some("food"));
        assert_eq!(parsed.note.as_deref(), Some("bar caffè"));
    }

    #[test]
    fn tag_can_be_anywhere() {
        let parsed = parse_quick_add("12.50 #food bar caffè", Currency::Eur).unwrap();
        assert_eq!(parsed.category.as_deref(), Some("food"));
        assert_eq!(parsed.note.as_deref(), Some("bar caffè"));
    }

    #[test]
    fn rejects_more_than_one_tag() {
        let err = parse_quick_add("12.50 a #x b #y", Currency::Eur).unwrap_err();
        assert!(matches!(err, ParseError::TooManyTags));
    }
}
