use sea_orm::{
    ActiveValue, DatabaseTransaction, QueryFilter, QueryOrder,
    prelude::*,
    sea_query::{Expr, Value},
};
use uuid::Uuid;

use crate::{
    Category, CategoryAlias, EngineError, ResultEngine, categories, category_aliases, transactions,
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
    pub async fn list_categories(
        &self,
        vault_id: &str,
        user_id: &str,
        include_archived: bool,
    ) -> ResultEngine<Vec<Category>> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_by_id(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;
                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;

                let mut query = categories::Entity::find()
                    .filter(categories::Column::VaultId.eq(vault_uuid))
                    .order_by_asc(categories::Column::Name);
                if !include_archived {
                    query = query.filter(categories::Column::Archived.eq(false));
                }
                let items = query
                    .all(db_tx)
                    .await?
                    .into_iter()
                    .map(Category::from)
                    .collect();
                Ok(items)
            })
        })
        .await
    }

    pub async fn create_category(
        &self,
        vault_id: &str,
        name: &str,
        user_id: &str,
    ) -> ResultEngine<Category> {
        let vault_id = vault_id.to_string();
        let name = name.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_by_id_write(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;

                let display = normalize_category_display(&name)?;
                let normalized = normalize_category_key(&display)?;
                if normalized == UNCATEGORIZED_NAME_NORM {
                    return Err(EngineError::InvalidName(
                        "category name is reserved".to_string(),
                    ));
                }

                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;
                if categories::Entity::find()
                    .filter(categories::Column::VaultId.eq(vault_uuid))
                    .filter(categories::Column::NameNorm.eq(normalized.clone()))
                    .one(db_tx)
                    .await?
                    .is_some()
                {
                    return Err(EngineError::ExistingKey(display));
                }
                if category_aliases::Entity::find()
                    .filter(category_aliases::Column::VaultId.eq(vault_uuid))
                    .filter(category_aliases::Column::AliasNorm.eq(normalized.clone()))
                    .one(db_tx)
                    .await?
                    .is_some()
                {
                    return Err(EngineError::ExistingKey(display));
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
                let model = active.insert(db_tx).await?;
                Ok(Category::from(model))
            })
        })
        .await
    }

    pub async fn update_category(
        &self,
        vault_id: &str,
        category_id: Uuid,
        name: Option<&str>,
        archived: Option<bool>,
        user_id: &str,
    ) -> ResultEngine<Category> {
        let vault_id = vault_id.to_string();
        let name = name.map(str::to_string);
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_by_id_write(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;

                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;
                let model = categories::Entity::find_by_id(category_id)
                    .filter(categories::Column::VaultId.eq(vault_uuid))
                    .one(db_tx)
                    .await?
                    .ok_or_else(|| EngineError::KeyNotFound("category not exists".to_string()))?;
                if model.is_system {
                    return Err(EngineError::InvalidName(
                        "system categories cannot be modified".to_string(),
                    ));
                }

                let mut name_norm = model.name_norm.clone();
                let mut name_display = model.name.clone();
                if let Some(new_name) = name.as_deref() {
                    let display = normalize_category_display(new_name)?;
                    let normalized = normalize_category_key(&display)?;
                    if normalized == UNCATEGORIZED_NAME_NORM {
                        return Err(EngineError::InvalidName(
                            "category name is reserved".to_string(),
                        ));
                    }

                    let conflict = categories::Entity::find()
                        .filter(categories::Column::VaultId.eq(vault_uuid))
                        .filter(categories::Column::NameNorm.eq(normalized.clone()))
                        .filter(categories::Column::Id.ne(category_id))
                        .one(db_tx)
                        .await?
                        .is_some();
                    if conflict {
                        return Err(EngineError::ExistingKey(display));
                    }
                    let alias_conflict = category_aliases::Entity::find()
                        .filter(category_aliases::Column::VaultId.eq(vault_uuid))
                        .filter(category_aliases::Column::AliasNorm.eq(normalized.clone()))
                        .one(db_tx)
                        .await?
                        .is_some();
                    if alias_conflict {
                        return Err(EngineError::ExistingKey(display));
                    }

                    name_norm = normalized;
                    name_display = display;
                }

                let archived = archived.unwrap_or(model.archived);
                let active = categories::ActiveModel {
                    id: ActiveValue::Set(category_id),
                    name: ActiveValue::Set(name_display.clone()),
                    name_norm: ActiveValue::Set(name_norm.clone()),
                    archived: ActiveValue::Set(archived),
                    ..Default::default()
                };
                active.update(db_tx).await?;

                if name_display != model.name {
                    transactions::Entity::update_many()
                        .col_expr(
                            transactions::Column::Category,
                            Expr::value(name_display.clone()),
                        )
                        .filter(transactions::Column::CategoryId.eq(category_id))
                        .exec(db_tx)
                        .await?;
                }

                Ok(Category {
                    id: category_id,
                    name: name_display,
                    archived,
                    is_system: model.is_system,
                })
            })
        })
        .await
    }

    pub async fn list_category_aliases(
        &self,
        vault_id: &str,
        category_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<Vec<CategoryAlias>> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_by_id(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;
                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;
                engine
                    .require_category_in_vault(db_tx, vault_uuid, category_id)
                    .await?;

                let aliases = category_aliases::Entity::find()
                    .filter(category_aliases::Column::VaultId.eq(vault_uuid))
                    .filter(category_aliases::Column::CategoryId.eq(category_id))
                    .order_by_asc(category_aliases::Column::Alias)
                    .all(db_tx)
                    .await?
                    .into_iter()
                    .map(CategoryAlias::from)
                    .collect();
                Ok(aliases)
            })
        })
        .await
    }

    pub async fn create_category_alias(
        &self,
        vault_id: &str,
        category_id: Uuid,
        alias: &str,
        user_id: &str,
    ) -> ResultEngine<CategoryAlias> {
        let vault_id = vault_id.to_string();
        let alias = alias.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_by_id_write(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;
                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;
                let category = engine
                    .require_category_in_vault(db_tx, vault_uuid, category_id)
                    .await?;
                if category.is_system {
                    return Err(EngineError::InvalidName(
                        "system categories cannot have aliases".to_string(),
                    ));
                }
                if category.archived {
                    return Err(EngineError::InvalidName(
                        "archived categories cannot have aliases".to_string(),
                    ));
                }

                let display = normalize_category_display(&alias)?;
                let normalized = normalize_category_key(&display)?;
                if normalized == UNCATEGORIZED_NAME_NORM {
                    return Err(EngineError::InvalidName(
                        "alias name is reserved".to_string(),
                    ));
                }
                if normalized == category.name_norm {
                    return Err(EngineError::ExistingKey(display));
                }

                if categories::Entity::find()
                    .filter(categories::Column::VaultId.eq(vault_uuid))
                    .filter(categories::Column::NameNorm.eq(normalized.clone()))
                    .one(db_tx)
                    .await?
                    .is_some()
                {
                    return Err(EngineError::ExistingKey(display));
                }
                if category_aliases::Entity::find()
                    .filter(category_aliases::Column::VaultId.eq(vault_uuid))
                    .filter(category_aliases::Column::AliasNorm.eq(normalized.clone()))
                    .one(db_tx)
                    .await?
                    .is_some()
                {
                    return Err(EngineError::ExistingKey(display));
                }

                let active = category_aliases::ActiveModel {
                    id: ActiveValue::Set(Uuid::new_v4()),
                    vault_id: ActiveValue::Set(vault_uuid),
                    category_id: ActiveValue::Set(category_id),
                    alias: ActiveValue::Set(display.clone()),
                    alias_norm: ActiveValue::Set(normalized),
                };
                let model = active.insert(db_tx).await?;
                Ok(CategoryAlias::from(model))
            })
        })
        .await
    }

    pub async fn delete_category_alias(
        &self,
        vault_id: &str,
        category_id: Uuid,
        alias_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<()> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_by_id_write(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;
                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;
                engine
                    .require_category_in_vault(db_tx, vault_uuid, category_id)
                    .await?;

                let result = category_aliases::Entity::delete_many()
                    .filter(category_aliases::Column::Id.eq(alias_id))
                    .filter(category_aliases::Column::CategoryId.eq(category_id))
                    .filter(category_aliases::Column::VaultId.eq(vault_uuid))
                    .exec(db_tx)
                    .await?;
                if result.rows_affected == 0 {
                    return Err(EngineError::KeyNotFound("alias not exists".to_string()));
                }
                Ok(())
            })
        })
        .await
    }

    pub async fn merge_category(
        &self,
        vault_id: &str,
        from_category_id: Uuid,
        into_category_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<Category> {
        if from_category_id == into_category_id {
            return Err(EngineError::InvalidName(
                "cannot merge a category into itself".to_string(),
            ));
        }
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_by_id_write(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;
                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;

                let from = engine
                    .require_category_in_vault(db_tx, vault_uuid, from_category_id)
                    .await?;
                let into = engine
                    .require_category_in_vault(db_tx, vault_uuid, into_category_id)
                    .await?;
                if from.is_system {
                    return Err(EngineError::InvalidName(
                        "system categories cannot be merged".to_string(),
                    ));
                }
                if into.archived {
                    return Err(EngineError::InvalidName(
                        "target category is archived".to_string(),
                    ));
                }

                let mut reserved: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                reserved.insert(into.name_norm.clone());
                let target_aliases = category_aliases::Entity::find()
                    .filter(category_aliases::Column::VaultId.eq(vault_uuid))
                    .filter(category_aliases::Column::CategoryId.eq(into_category_id))
                    .all(db_tx)
                    .await?;
                for alias in &target_aliases {
                    reserved.insert(alias.alias_norm.clone());
                }

                let from_aliases = category_aliases::Entity::find()
                    .filter(category_aliases::Column::VaultId.eq(vault_uuid))
                    .filter(category_aliases::Column::CategoryId.eq(from_category_id))
                    .all(db_tx)
                    .await?;

                for alias in &from_aliases {
                    if reserved.contains(&alias.alias_norm) {
                        return Err(EngineError::ExistingKey(alias.alias.clone()));
                    }
                }
                if from.name_norm != into.name_norm && reserved.contains(&from.name_norm) {
                    return Err(EngineError::ExistingKey(from.name.clone()));
                }

                let category_display =
                    if into.is_system && into.name_norm == UNCATEGORIZED_NAME_NORM {
                        Value::String(None)
                    } else {
                        Value::String(Some(Box::new(into.name.clone())))
                    };

                transactions::Entity::update_many()
                    .col_expr(
                        transactions::Column::CategoryId,
                        Expr::value(into_category_id),
                    )
                    .col_expr(
                        transactions::Column::Category,
                        Expr::value(category_display),
                    )
                    .filter(transactions::Column::CategoryId.eq(from_category_id))
                    .exec(db_tx)
                    .await?;

                if !from_aliases.is_empty() {
                    category_aliases::Entity::update_many()
                        .col_expr(
                            category_aliases::Column::CategoryId,
                            Expr::value(into_category_id),
                        )
                        .filter(category_aliases::Column::CategoryId.eq(from_category_id))
                        .exec(db_tx)
                        .await?;
                }

                if from.name_norm != into.name_norm {
                    let alias_active = category_aliases::ActiveModel {
                        id: ActiveValue::Set(Uuid::new_v4()),
                        vault_id: ActiveValue::Set(vault_uuid),
                        category_id: ActiveValue::Set(into_category_id),
                        alias: ActiveValue::Set(from.name.clone()),
                        alias_norm: ActiveValue::Set(from.name_norm.clone()),
                    };
                    alias_active.insert(db_tx).await?;
                }

                let active = categories::ActiveModel {
                    id: ActiveValue::Set(from_category_id),
                    archived: ActiveValue::Set(true),
                    ..Default::default()
                };
                active.update(db_tx).await?;

                Ok(Category {
                    id: into.id,
                    name: into.name,
                    archived: into.archived,
                    is_system: into.is_system,
                })
            })
        })
        .await
    }

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
            .filter(categories::Column::Archived.eq(false))
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
            if model.archived {
                return Err(EngineError::InvalidName("category is archived".to_string()));
            }
            return Ok(Self::category_selection(&model));
        }

        if categories::Entity::find()
            .filter(categories::Column::VaultId.eq(vault_uuid))
            .filter(categories::Column::NameNorm.eq(normalized.clone()))
            .filter(categories::Column::Archived.eq(true))
            .one(db_tx)
            .await?
            .is_some()
        {
            return Err(EngineError::InvalidName("category is archived".to_string()));
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

    pub(super) async fn resolve_category_input(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        category_id: Option<Uuid>,
        input: Option<&str>,
    ) -> ResultEngine<CategorySelection> {
        if let Some(category_id) = category_id {
            return self.category_by_id(db_tx, vault_id, category_id).await;
        }
        self.resolve_category(db_tx, vault_id, input).await
    }

    async fn category_by_id(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        category_id: Uuid,
    ) -> ResultEngine<CategorySelection> {
        let vault_uuid = parse_vault_uuid(vault_id)?;
        let model = categories::Entity::find_by_id(category_id)
            .filter(categories::Column::VaultId.eq(vault_uuid))
            .one(db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("category not exists".to_string()))?;
        if model.archived {
            return Err(EngineError::InvalidName("category is archived".to_string()));
        }
        Ok(Self::category_selection(&model))
    }

    async fn require_category_in_vault(
        &self,
        db_tx: &DatabaseTransaction,
        vault_uuid: Uuid,
        category_id: Uuid,
    ) -> ResultEngine<categories::Model> {
        categories::Entity::find_by_id(category_id)
            .filter(categories::Column::VaultId.eq(vault_uuid))
            .one(db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("category not exists".to_string()))
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
            .filter(categories::Column::Archived.eq(false))
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
