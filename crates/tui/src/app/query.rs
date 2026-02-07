//! Search and filtering utilities.
//!
//! This module provides functions for normalizing search queries, performing
//! fuzzy matching, and filtering entities (transactions, wallets, flows, etc.)
//! based on user search input.

use api_types::transaction::TransactionView;

use crate::app::PaletteCommand;

use super::format::transaction_kind_label;

/// Normalizes a search query by trimming whitespace and converting to
/// lowercase.
pub(crate) fn normalize_query(query: &str) -> String {
    query.trim().to_lowercase()
}

/// Checks if a transaction matches a search query.
///
/// Searches across: kind label, note, category name, amount, and timestamp.
/// Empty queries always match.
pub(crate) fn transaction_matches_query(tx: &TransactionView, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let kind = transaction_kind_label(tx.kind);
    if kind.contains(query) {
        return true;
    }
    if tx
        .note
        .as_ref()
        .map(|note| note.to_lowercase().contains(query))
        .unwrap_or(false)
    {
        return true;
    }
    if tx
        .category
        .as_ref()
        .map(|category| category.to_lowercase().contains(query))
        .unwrap_or(false)
    {
        return true;
    }
    let amount = tx.amount_minor.abs().to_string();
    if amount.contains(query) {
        return true;
    }
    let when = tx.occurred_at.format("%Y-%m-%d %H:%M").to_string();
    when.contains(query)
}

/// Filters and sorts palette commands, prioritizing MRU when query is empty.
///
/// When no query is provided, commands are sorted by most-recently-used (MRU)
/// order. When a query is present, commands are fuzzy-matched and sorted by
/// match quality.
pub(crate) fn filter_commands(query: &str, mru: &[PaletteCommand]) -> Vec<PaletteCommand> {
    let query = query.trim().to_lowercase();
    let all = PaletteCommand::all();

    if query.is_empty() {
        // When no query, show MRU first, then remaining commands
        let mut result = Vec::with_capacity(all.len());
        for cmd in mru {
            if !result.contains(cmd) {
                result.push(*cmd);
            }
        }
        for cmd in &all {
            if !result.contains(cmd) {
                result.push(*cmd);
            }
        }
        return result;
    }

    let mut scored = all
        .into_iter()
        .filter_map(|cmd| {
            let label = cmd.label().to_lowercase();
            fuzzy_score(&label, &query).map(|score| (score, cmd))
        })
        .collect::<Vec<_>>();

    scored.sort_by_key(|(score, _)| *score);
    scored.into_iter().map(|(_, cmd)| cmd).collect()
}

/// Computes a fuzzy match score for a label against a query.
///
/// Returns Some(score) if all query characters appear in order in the label,
/// where a lower score indicates a better match. Returns None if the query
/// doesn't match.
fn fuzzy_score(label: &str, query: &str) -> Option<usize> {
    let mut score = 0usize;
    let mut pos = 0usize;
    for ch in query.chars() {
        if let Some(idx) = label[pos..].find(ch) {
            score += idx;
            pos += idx + 1;
        } else {
            return None;
        }
    }
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::filter_commands;
    use crate::app::PaletteCommand;

    #[test]
    fn palette_includes_category_commands() {
        let all = PaletteCommand::all();
        assert!(all.contains(&PaletteCommand::Categories));
        assert!(all.contains(&PaletteCommand::CategoryAliases));
        assert!(all.contains(&PaletteCommand::Members));
    }

    #[test]
    fn filter_commands_matches_category_queries() {
        let commands = filter_commands("cat", &[]);
        assert!(commands.contains(&PaletteCommand::Categories));
        let commands = filter_commands("alias", &[]);
        assert!(commands.contains(&PaletteCommand::CategoryAliases));
        let commands = filter_commands("member", &[]);
        assert!(commands.contains(&PaletteCommand::Members));
    }

    #[test]
    fn filter_commands_prioritizes_mru_when_empty() {
        let mru = vec![PaletteCommand::Refresh, PaletteCommand::Categories];
        let commands = filter_commands("", &mru);
        // MRU commands should be first
        assert_eq!(commands[0], PaletteCommand::Refresh);
        assert_eq!(commands[1], PaletteCommand::Categories);
    }
}
