//! Category suggestions learned from the vault's own history: the category a
//! note was filed under before.
//!
//! A note is compared by its key: the note normalized like a category name
//! ([`normalize_category_key`]: no diacritics, lowercase, punctuation as one
//! space) without the words made only of digits, so `"Pizza 2"` and
//! `"pizza"` are the same note. A whole-key match wins; failing that, the first
//! word of the note is matched against the first word of past notes, which
//! needs more evidence before it says anything.

use std::collections::{HashMap, hash_map};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use uuid::Uuid;

use crate::{Core, Result, category::normalize_category_key};

/// Shortest first word, in characters, a first-word match may rest on.
const MIN_WORD_CHARS: usize = 3;
/// A first-word match needs the word to have been used this many times...
const MIN_WORD_USES: u64 = 2;
/// ...and one category to hold at least this share of those uses, in percent.
const MIN_WORD_SHARE: u64 = 60;

/// The category a note is most likely to belong to.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CategorySuggestion {
    pub category_id: Uuid,
    pub name: String,
    /// How many live transactions since the cutoff back the suggestion.
    pub uses: u32,
    /// `true` when the whole note matched; `false` for a first-word match.
    pub exact: bool,
}

impl Core {
    /// One suggestion per note, in the same order (`None` when the history
    /// says nothing), looking at live transactions since `since`.
    ///
    /// The history is every income, expense and refund of the vault that is
    /// not voided, has a note and sits in a category that is neither a system
    /// one nor archived. A note whose key matches past notes exactly gets the
    /// category used most often for them, the most recent one on a tie.
    /// Otherwise its first word (at least three characters) is looked up among
    /// the first words of past notes, and answers only when that word was used
    /// at least twice and one category holds at least 60% of those uses. An
    /// unknown vault has no history, so every note gets `None`.
    pub fn suggest_categories(
        &self,
        vault_id: Uuid,
        notes: &[String],
        since: DateTime<Utc>,
    ) -> Result<Vec<Option<CategorySuggestion>>> {
        let keys: Vec<Option<String>> = notes.iter().map(|note| note_key(note)).collect();
        if keys.iter().all(Option::is_none) {
            return Ok(vec![None; notes.len()]);
        }
        let history = History::load(&self.conn, vault_id, since)?;
        Ok(keys
            .iter()
            .map(|key| key.as_deref().and_then(|key| history.suggest(key)))
            .collect())
    }
}

/// How often one category was used under one key.
#[derive(Clone, Copy, Debug)]
struct Tally {
    uses: u64,
    /// `(occurred_at, transaction id)` of the latest use: the tie-breaker,
    /// never equal for two categories since a transaction has one category.
    last: (i64, Uuid),
}

impl Tally {
    fn add(&mut self, at: (i64, Uuid)) {
        self.uses += 1;
        self.last = self.last.max(at);
    }
}

/// Category id -> its uses under one key.
type Tallies = HashMap<Uuid, Tally>;

/// The candidate history, loaded once and indexed both ways.
struct History {
    names: HashMap<Uuid, String>,
    by_key: HashMap<String, Tallies>,
    by_first_word: HashMap<String, Tallies>,
}

impl History {
    fn load(conn: &Connection, vault_id: Uuid, since: DateTime<Utc>) -> Result<Self> {
        let mut stmt = conn.prepare(
            "SELECT t.id, t.occurred_at, t.note, t.category_id, c.name
             FROM transactions t JOIN categories c ON c.id = t.category_id
             WHERE t.vault_id = ?1 AND t.voided_at IS NULL
               AND t.kind IN ('income', 'expense', 'refund')
               AND t.occurred_at >= ?2
               AND t.note IS NOT NULL AND t.note <> ''
               AND c.is_system = 0 AND c.archived = 0",
        )?;
        let mut rows = stmt.query(params![vault_id, since.timestamp()])?;
        let mut history = Self {
            names: HashMap::new(),
            by_key: HashMap::new(),
            by_first_word: HashMap::new(),
        };
        while let Some(row) = rows.next()? {
            let note: String = row.get(2)?;
            let Some(key) = note_key(&note) else {
                continue;
            };
            let at = (row.get::<_, i64>(1)?, row.get::<_, Uuid>(0)?);
            let category: Uuid = row.get(3)?;
            if let hash_map::Entry::Vacant(slot) = history.names.entry(category) {
                slot.insert(row.get(4)?);
            }
            if let Some(word) = first_word(&key) {
                count(
                    history.by_first_word.entry(word.to_string()).or_default(),
                    category,
                    at,
                );
            }
            count(history.by_key.entry(key).or_default(), category, at);
        }
        Ok(history)
    }

    fn suggest(&self, key: &str) -> Option<CategorySuggestion> {
        if let Some((category, tally)) = self.by_key.get(key).and_then(best) {
            return self.suggestion(category, tally, true);
        }
        let tallies = self.by_first_word.get(first_word(key)?)?;
        let total: u64 = tallies.values().map(|tally| tally.uses).sum();
        let (category, tally) = best(tallies)?;
        if total >= MIN_WORD_USES && tally.uses * 100 >= total * MIN_WORD_SHARE {
            self.suggestion(category, tally, false)
        } else {
            None
        }
    }

    fn suggestion(&self, category: Uuid, tally: Tally, exact: bool) -> Option<CategorySuggestion> {
        Some(CategorySuggestion {
            category_id: category,
            name: self.names.get(&category)?.clone(),
            uses: u32::try_from(tally.uses).unwrap_or(u32::MAX),
            exact,
        })
    }
}

fn count(tallies: &mut Tallies, category: Uuid, at: (i64, Uuid)) {
    tallies
        .entry(category)
        .and_modify(|tally| tally.add(at))
        .or_insert(Tally { uses: 1, last: at });
}

/// The most used category, the most recently used on a tie.
fn best(tallies: &Tallies) -> Option<(Uuid, Tally)> {
    tallies
        .iter()
        .max_by_key(|(_, tally)| (tally.uses, tally.last))
        .map(|(category, tally)| (*category, *tally))
}

/// The key a note is compared by; `None` when nothing but digits and
/// punctuation is left.
fn note_key(note: &str) -> Option<String> {
    let key = normalize_category_key(note).ok()?;
    let words: Vec<&str> = key
        .split(' ')
        .filter(|word| !word.chars().all(char::is_numeric))
        .collect();
    if words.is_empty() {
        None
    } else {
        Some(words.join(" "))
    }
}

/// The first word of a key, when it is long enough to mean something.
fn first_word(key: &str) -> Option<&str> {
    key.split(' ')
        .next()
        .filter(|word| word.chars().count() >= MIN_WORD_CHARS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_key_drops_accents_punctuation_and_bare_numbers() {
        assert_eq!(note_key("  Caffè, 2 "), Some("caffe".to_string()));
        assert_eq!(note_key("Pizza 12.50€"), Some("pizza".to_string()));
        assert_eq!(note_key("2x pizza"), Some("2x pizza".to_string()));
        assert_eq!(note_key("12/03"), None);
        assert_eq!(note_key("!!!"), None);
        assert_eq!(note_key(""), None);
    }

    #[test]
    fn a_first_word_needs_three_characters() {
        assert_eq!(first_word("bar sport"), Some("bar"));
        assert_eq!(first_word("da mario"), None);
        assert_eq!(first_word("gas luce"), Some("gas"));
    }
}
