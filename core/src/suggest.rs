//! Category suggestions learned from the vault's own history: the category a
//! note was filed under before.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{Core, DomainError, Result};

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
    pub fn suggest_categories(
        &self,
        vault_id: Uuid,
        notes: &[String],
        since: DateTime<Utc>,
    ) -> Result<Vec<Option<CategorySuggestion>>> {
        let _ = (vault_id, notes, since);
        Err(DomainError::InvalidCommand(
            "suggest_categories: not implemented".to_string(),
        ))
    }
}
