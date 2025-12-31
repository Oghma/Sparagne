use sea_orm::{ActiveValue, DatabaseTransaction, QueryFilter, prelude::*};
use uuid::Uuid;

use crate::{
    EngineError, ResultEngine, categories, category_aliases,
    util::{normalize_category_display, normalize_category_key},
};

use super::{Engine, parse_vault_uuid};

const UNCATEGORIZED_NAME: &str = "Uncategorized";
const UNCATEGORIZED_NAME_NORM: &str = "uncategorized";

pub(super) struct CategorySelection {
    pub(super) id: Uuid,
    pub(super) name: Option<String>,
}

impl Engine {
    pub(super) async fn resolve_category(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        input: Option<&str>,
    ) -> ResultEngine<CategorySelection> {
        let trimmed = input.map(str::trim).filter(|value| !value.is_empty());
        if trimmed.is_none() {
            return self.uncategorized_category(db_tx, vault_id).await;
        }

        let display = normalize_category_display(trimmed.unwrap())?;
        let normalized = normalize_category_key(&display)?;
        if normalized == UNCATEGORIZED_NAME_NORM {
            return self.uncategorized_category(db_tx, vault_id).await;
        }

        let vault_uuid = parse_vault_uuid(vault_id)?;
        if let Some(model) = categories::Entity::find()
            .filter(categories::Column::VaultId.eq(vault_uuid))
            .filter(categories::Column::NameNorm.eq(normalized.clone()))
            .one(db_tx)
            .await?
        {
            return Ok(Self::category_selection(&model));
        }

        if let Some((_, Some(model))) = category_aliases::Entity::find()
            .filter(category_aliases::Column::VaultId.eq(vault_uuid))
            .filter(category_aliases::Column::AliasNorm.eq(normalized.clone()))
            .find_also_related(categories::Entity)
            .one(db_tx)
            .await?
        {
            return Ok(Self::category_selection(&model));
        }

        if let Some(suggestion) =
            Self::find_similar_category(db_tx, vault_uuid, &normalized).await?
        {
            return Err(EngineError::InvalidName(format!(
                "category '{display}' too similar to existing '{}'; use '{}' to confirm",
                suggestion.name, suggestion.name
            )));
        }

        let id = Uuid::new_v4();
        let active = categories::ActiveModel {
            id: ActiveValue::Set(id),
            vault_id: ActiveValue::Set(vault_uuid),
            name: ActiveValue::Set(display.clone()),
            name_norm: ActiveValue::Set(normalized),
            archived: ActiveValue::Set(false),
            is_system: ActiveValue::Set(false),
        };
        active.insert(db_tx).await?;

        Ok(CategorySelection {
            id,
            name: Some(display),
        })
    }

    async fn uncategorized_category(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
    ) -> ResultEngine<CategorySelection> {
        let vault_uuid = parse_vault_uuid(vault_id)?;
        if let Some(model) = categories::Entity::find()
            .filter(categories::Column::VaultId.eq(vault_uuid))
            .filter(categories::Column::NameNorm.eq(UNCATEGORIZED_NAME_NORM))
            .one(db_tx)
            .await?
        {
            return Ok(Self::category_selection(&model));
        }

        let id = Uuid::new_v4();
        let active = categories::ActiveModel {
            id: ActiveValue::Set(id),
            vault_id: ActiveValue::Set(vault_uuid),
            name: ActiveValue::Set(UNCATEGORIZED_NAME.to_string()),
            name_norm: ActiveValue::Set(UNCATEGORIZED_NAME_NORM.to_string()),
            archived: ActiveValue::Set(false),
            is_system: ActiveValue::Set(true),
        };
        active.insert(db_tx).await?;

        Ok(CategorySelection { id, name: None })
    }

    fn category_selection(model: &categories::Model) -> CategorySelection {
        let name = if model.is_system && model.name_norm == UNCATEGORIZED_NAME_NORM {
            None
        } else {
            Some(model.name.clone())
        };
        CategorySelection { id: model.id, name }
    }

    async fn find_similar_category(
        db_tx: &DatabaseTransaction,
        vault_id: Uuid,
        normalized: &str,
    ) -> ResultEngine<Option<categories::Model>> {
        let candidates = categories::Entity::find()
            .filter(categories::Column::VaultId.eq(vault_id))
            .filter(categories::Column::IsSystem.eq(false))
            .all(db_tx)
            .await?;

        let threshold = similarity_threshold(normalized);
        let mut best: Option<(usize, categories::Model)> = None;

        for candidate in candidates {
            let distance = levenshtein(normalized, candidate.name_norm.as_str());
            if distance > threshold {
                continue;
            }
            let replace = match &best {
                None => true,
                Some((best_distance, best_model)) => {
                    distance < *best_distance
                        || (distance == *best_distance
                            && candidate.name_norm.len() < best_model.name_norm.len())
                }
            };
            if replace {
                best = Some((distance, candidate));
            }
        }

        Ok(best.map(|(_, model)| model))
    }
}

fn similarity_threshold(input: &str) -> usize {
    let len = input.chars().count();
    if len <= 6 { 1 } else { 2 }
}

fn levenshtein(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();

    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }

    let mut costs: Vec<usize> = (0..=right.len()).collect();

    for (i, left_char) in left.iter().enumerate() {
        let mut last_cost = i;
        costs[0] = i + 1;
        for (j, right_char) in right.iter().enumerate() {
            let next_cost = costs[j + 1];
            let mut cost = if left_char == right_char {
                last_cost
            } else {
                last_cost + 1
            };
            cost = cost.min(costs[j] + 1).min(next_cost + 1);
            costs[j + 1] = cost;
            last_cost = next_cost;
        }
    }

    costs[right.len()]
}
