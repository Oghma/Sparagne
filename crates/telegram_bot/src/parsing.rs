use std::collections::HashMap;

use engine::{Currency, Money};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum QuickKind {
    Income,
    Expense,
}

/// Suggests a category based on note text and user's category hints.
/// Returns the suggested category if a keyword matches.
pub(crate) fn suggest_category<'a>(
    note: Option<&str>,
    hints: &'a HashMap<String, String>,
) -> Option<&'a str> {
    let note = note?;
    let note_lower = note.to_lowercase();

    for (keyword, category) in hints {
        if note_lower.contains(&keyword.to_lowercase()) {
            return Some(category.as_str());
        }
    }
    None
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
/// Rules:
/// - `12.50 ...` and `-12.50 ...` => Expense
/// - `+12.50 ...` => Income
/// - optional `#tag` (max 1) => category (case-insensitive)
pub(crate) fn parse_quick_add(input: &str, currency: Currency) -> Result<QuickAdd, ParseError> {
    let trimmed = collapse_whitespace(input.trim());
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }

    let kind = if trimmed.starts_with('+') {
        QuickKind::Income
    } else {
        QuickKind::Expense
    };

    let mut parts = trimmed.splitn(2, ' ');
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
    fn expense_default_without_sign() -> Result<(), ParseError> {
        let parsed = parse_quick_add("12.50 bar", Currency::Eur)?;
        assert_eq!(parsed.kind, QuickKind::Expense);
        assert_eq!(parsed.amount_minor, 1250);
        Ok(())
    }

    #[test]
    fn expense_with_minus_sign() -> Result<(), ParseError> {
        let parsed = parse_quick_add("-12.50 bar", Currency::Eur)?;
        assert_eq!(parsed.kind, QuickKind::Expense);
        assert_eq!(parsed.amount_minor, 1250);
        Ok(())
    }

    #[test]
    fn income_with_plus_sign() -> Result<(), ParseError> {
        let parsed = parse_quick_add("+1000 stipendio", Currency::Eur)?;
        assert_eq!(parsed.kind, QuickKind::Income);
        assert_eq!(parsed.amount_minor, 100_000);
        Ok(())
    }

    #[test]
    fn tag_sets_category_and_is_removed_from_note() -> Result<(), ParseError> {
        let parsed = parse_quick_add("12.50 bar #Food caffè", Currency::Eur)?;
        assert_eq!(parsed.category.as_deref(), Some("food"));
        assert_eq!(parsed.note.as_deref(), Some("bar caffè"));
        Ok(())
    }

    #[test]
    fn tag_can_be_anywhere() -> Result<(), ParseError> {
        let parsed = parse_quick_add("12.50 #food bar caffè", Currency::Eur)?;
        assert_eq!(parsed.category.as_deref(), Some("food"));
        assert_eq!(parsed.note.as_deref(), Some("bar caffè"));
        Ok(())
    }

    #[test]
    fn rejects_more_than_one_tag() {
        match parse_quick_add("12.50 a #x b #y", Currency::Eur) {
            Err(ParseError::TooManyTags) => {}
            Err(err) => panic!("unexpected error: {err:?}"),
            Ok(_) => panic!("expected too-many-tags error"),
        }
    }
}
