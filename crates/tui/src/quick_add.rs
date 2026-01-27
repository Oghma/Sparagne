use engine::{Currency, Money};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickAddKind {
    Income,
    Expense,
    Refund,
}

#[derive(Debug, Clone)]
pub struct QuickAddParsed {
    pub kind: QuickAddKind,
    pub amount_minor: i64,
    pub category: Option<String>,
    pub flow: Option<String>,
    pub note: Option<String>,
}

pub fn parse(input: &str, currency: Currency) -> Result<QuickAddParsed, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Inserisci un importo.".to_string());
    }

    let (kind, rest) = if let Some(stripped) = trimmed.strip_prefix('r') {
        (QuickAddKind::Refund, stripped.trim_start())
    } else if let Some(stripped) = trimmed.strip_prefix('R') {
        (QuickAddKind::Refund, stripped.trim_start())
    } else if let Some(stripped) = trimmed.strip_prefix('+') {
        (QuickAddKind::Income, stripped.trim_start())
    } else if let Some(stripped) = trimmed.strip_prefix('-') {
        (QuickAddKind::Expense, stripped.trim_start())
    } else {
        (QuickAddKind::Expense, trimmed)
    };

    let mut parts = rest.splitn(2, ' ');
    let amount_raw = parts.next().unwrap_or("").trim();
    if amount_raw.is_empty() {
        return Err("Importo mancante.".to_string());
    }
    let note_raw = parts.next().unwrap_or("").trim();

    let amount = Money::parse_major(amount_raw, currency)
        .map_err(|_| "Importo non valido.".to_string())?
        .minor()
        .abs();
    if amount == 0 {
        return Err("Importo deve essere > 0.".to_string());
    }

    let (category, flow, note) = parse_tags(note_raw)?;

    Ok(QuickAddParsed {
        kind,
        amount_minor: amount,
        category,
        flow,
        note,
    })
}

fn parse_tags(note_raw: &str) -> Result<(Option<String>, Option<String>, Option<String>), String> {
    if note_raw.is_empty() {
        return Ok((None, None, None));
    }

    let mut category: Option<String> = None;
    let mut flow: Option<String> = None;
    let mut kept: Vec<&str> = Vec::new();

    for token in note_raw.split_whitespace() {
        if let Some(rest) = token.strip_prefix('#') {
            if rest.is_empty() {
                kept.push(token);
                continue;
            }
            if category.is_some() {
                return Err("Troppi tag: massimo 1.".to_string());
            }
            category = Some(rest.to_lowercase());
        } else if let Some(rest) = token.strip_prefix('>') {
            if rest.is_empty() {
                kept.push(token);
                continue;
            }
            if flow.is_some() {
                return Err("Troppi envelope: massimo 1.".to_string());
            }
            flow = Some(rest.to_lowercase());
        } else {
            kept.push(token);
        }
    }

    let note = kept.join(" ");
    let note = if note.is_empty() { None } else { Some(note) };
    Ok((category, flow, note))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_with_envelope() {
        let parsed = parse("15 pizza >ufficio", Currency::Eur).expect("parse should succeed");
        assert_eq!(parsed.kind, QuickAddKind::Expense);
        assert_eq!(parsed.amount_minor, 1_500);
        assert_eq!(parsed.flow.as_deref(), Some("ufficio"));
        assert_eq!(parsed.note.as_deref(), Some("pizza"));
    }

    #[test]
    fn parse_with_category_and_envelope() {
        let parsed =
            parse("+10 bonus #lavoro >ufficio", Currency::Eur).expect("parse should succeed");
        assert_eq!(parsed.kind, QuickAddKind::Income);
        assert_eq!(parsed.category.as_deref(), Some("lavoro"));
        assert_eq!(parsed.flow.as_deref(), Some("ufficio"));
        assert_eq!(parsed.note.as_deref(), Some("bonus"));
    }

    #[test]
    fn parse_rejects_multiple_envelopes() {
        let err = parse("15 pizza >ufficio >casa", Currency::Eur).expect_err("should fail");
        assert!(err.contains("envelope"));
    }
}
