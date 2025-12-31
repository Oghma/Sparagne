use std::collections::{HashMap, HashSet};

use sea_orm::{ConnectionTrait, DbBackend, QueryResult, Statement, Value, prelude::DateTimeUtc};
use sea_orm_migration::{SchemaManagerConnection, prelude::*};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};
use uuid::Uuid;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(Iden)]
enum Categories {
    Table,
    Id,
    VaultId,
    Name,
    NameNorm,
    Archived,
    IsSystem,
}

#[derive(Iden)]
enum CategoryAliases {
    Table,
    Id,
    VaultId,
    CategoryId,
    Alias,
    AliasNorm,
}

#[derive(Iden)]
enum Transactions {
    Table,
    CategoryId,
}

#[derive(Iden)]
enum Vaults {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Categories::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Categories::Id)
                            .blob()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Categories::VaultId).blob().not_null())
                    .col(ColumnDef::new(Categories::Name).string().not_null())
                    .col(ColumnDef::new(Categories::NameNorm).string().not_null())
                    .col(
                        ColumnDef::new(Categories::Archived)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Categories::IsSystem)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-categories-vault_id")
                            .from(Categories::Table, Categories::VaultId)
                            .to(Vaults::Table, Vaults::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-categories-vault_id-name_norm-unique")
                    .table(Categories::Table)
                    .col(Categories::VaultId)
                    .col(Categories::NameNorm)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(CategoryAliases::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CategoryAliases::Id)
                            .blob()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(CategoryAliases::VaultId).blob().not_null())
                    .col(
                        ColumnDef::new(CategoryAliases::CategoryId)
                            .blob()
                            .not_null(),
                    )
                    .col(ColumnDef::new(CategoryAliases::Alias).string().not_null())
                    .col(
                        ColumnDef::new(CategoryAliases::AliasNorm)
                            .string()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-category_aliases-vault_id")
                            .from(CategoryAliases::Table, CategoryAliases::VaultId)
                            .to(Vaults::Table, Vaults::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-category_aliases-category_id")
                            .from(CategoryAliases::Table, CategoryAliases::CategoryId)
                            .to(Categories::Table, Categories::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-category_aliases-vault_id-alias_norm-unique")
                    .table(CategoryAliases::Table)
                    .col(CategoryAliases::VaultId)
                    .col(CategoryAliases::AliasNorm)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Transactions::Table)
                    .add_column(ColumnDef::new(Transactions::CategoryId).blob())
                    .to_owned(),
            )
            .await?;

        backfill_categories(manager).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CategoryAliases::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Categories::Table).to_owned())
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Transactions::Table)
                    .drop_column(Transactions::CategoryId)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

struct CategoryVariant {
    display: String,
    norm: String,
    count: usize,
    earliest: DateTimeUtc,
}

struct TxRecord {
    id: Uuid,
    display: Option<String>,
}

#[derive(Clone)]
struct CanonicalCategory {
    id: Uuid,
    display: String,
    norm: String,
}

async fn backfill_categories(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let db = manager.get_connection();
    let backend = db.get_database_backend();

    let vault_rows = db
        .query_all(Statement::from_string(backend, "SELECT id FROM vaults;"))
        .await?;
    let mut vault_ids = Vec::with_capacity(vault_rows.len());
    for row in vault_rows {
        let vault_id = uuid_from_row(&row, "id")?;
        vault_ids.push(vault_id);
    }

    let mut uncategorized_by_vault = HashMap::new();
    for vault_id in &vault_ids {
        let uncategorized_id = Uuid::new_v4();
        insert_category(
            db,
            backend,
            uncategorized_id,
            *vault_id,
            "Uncategorized",
            "uncategorized",
            true,
        )
        .await?;
        uncategorized_by_vault.insert(*vault_id, uncategorized_id);
    }

    let tx_rows = db
        .query_all(Statement::from_string(
            backend,
            "SELECT id, vault_id, category, occurred_at FROM transactions;",
        ))
        .await?;

    let mut variants_by_vault: HashMap<Uuid, HashMap<String, CategoryVariant>> = HashMap::new();
    let mut tx_by_vault: HashMap<Uuid, Vec<TxRecord>> = HashMap::new();

    for row in tx_rows {
        let tx_id = uuid_from_row(&row, "id")?;
        let vault_id = uuid_from_row(&row, "vault_id")?;
        let category: Option<String> = row.try_get("", "category")?;
        let occurred_at: DateTimeUtc = row.try_get("", "occurred_at")?;

        let display = category.as_deref().and_then(normalize_display);
        if let Some(display) = display.clone() {
            let norm = match normalize_key(&display) {
                Some(norm) => norm,
                None => {
                    tx_by_vault.entry(vault_id).or_default().push(TxRecord {
                        id: tx_id,
                        display: None,
                    });
                    continue;
                }
            };

            let variants = variants_by_vault.entry(vault_id).or_default();
            match variants.get_mut(&display) {
                Some(existing) => {
                    existing.count += 1;
                    if occurred_at < existing.earliest {
                        existing.earliest = occurred_at;
                    }
                }
                None => {
                    variants.insert(
                        display.clone(),
                        CategoryVariant {
                            display: display.clone(),
                            norm,
                            count: 1,
                            earliest: occurred_at,
                        },
                    );
                }
            }

            tx_by_vault.entry(vault_id).or_default().push(TxRecord {
                id: tx_id,
                display: Some(display),
            });
        } else {
            tx_by_vault.entry(vault_id).or_default().push(TxRecord {
                id: tx_id,
                display: None,
            });
        }
    }

    for vault_id in vault_ids {
        let mut variants: Vec<CategoryVariant> = variants_by_vault
            .remove(&vault_id)
            .unwrap_or_default()
            .into_values()
            .collect();
        variants.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.display.len().cmp(&b.display.len()))
                .then_with(|| a.earliest.cmp(&b.earliest))
                .then_with(|| a.display.cmp(&b.display))
        });

        let mut canonicals: Vec<CanonicalCategory> = Vec::new();
        let mut display_to_canonical: HashMap<String, CanonicalCategory> = HashMap::new();
        let mut used_alias_norms: HashSet<String> = HashSet::new();

        for variant in variants {
            if let Some((canonical, canonical_norm)) =
                find_similar_canonical(&canonicals, &variant.norm)
            {
                display_to_canonical.insert(variant.display.clone(), canonical.clone());
                if variant.norm != canonical_norm && used_alias_norms.insert(variant.norm.clone()) {
                    insert_alias(
                        db,
                        backend,
                        Uuid::new_v4(),
                        vault_id,
                        canonical.id,
                        &variant.display,
                        &variant.norm,
                    )
                    .await?;
                }
                continue;
            }

            let category_id = Uuid::new_v4();
            insert_category(
                db,
                backend,
                category_id,
                vault_id,
                &variant.display,
                &variant.norm,
                false,
            )
            .await?;

            let canonical = CanonicalCategory {
                id: category_id,
                display: variant.display.clone(),
                norm: variant.norm.clone(),
            };
            display_to_canonical.insert(variant.display.clone(), canonical.clone());
            canonicals.push(canonical);
        }

        let tx_records = tx_by_vault.remove(&vault_id).unwrap_or_default();
        let uncategorized_id = uncategorized_by_vault[&vault_id];

        for tx in tx_records {
            let (category_id, category_display) = match tx
                .display
                .as_ref()
                .and_then(|display| display_to_canonical.get(display))
            {
                Some(canonical) => (canonical.id, Some(canonical.display.as_str())),
                None => (uncategorized_id, None),
            };

            update_transaction_category(db, backend, tx.id, category_id, category_display).await?;
        }
    }

    Ok(())
}

async fn insert_category(
    db: &SchemaManagerConnection<'_>,
    backend: DbBackend,
    id: Uuid,
    vault_id: Uuid,
    name: &str,
    name_norm: &str,
    is_system: bool,
) -> Result<(), DbErr> {
    let values = vec![
        id.as_bytes().to_vec().into(),
        vault_id.as_bytes().to_vec().into(),
        name.to_string().into(),
        name_norm.to_string().into(),
        Value::Bool(Some(false)),
        Value::Bool(Some(is_system)),
    ];
    db.execute(Statement::from_sql_and_values(
        backend,
        "INSERT INTO categories (id, vault_id, name, name_norm, archived, is_system) \
         VALUES (?, ?, ?, ?, ?, ?);",
        values,
    ))
    .await?;
    Ok(())
}

async fn insert_alias(
    db: &SchemaManagerConnection<'_>,
    backend: DbBackend,
    id: Uuid,
    vault_id: Uuid,
    category_id: Uuid,
    alias: &str,
    alias_norm: &str,
) -> Result<(), DbErr> {
    let values = vec![
        id.as_bytes().to_vec().into(),
        vault_id.as_bytes().to_vec().into(),
        category_id.as_bytes().to_vec().into(),
        alias.to_string().into(),
        alias_norm.to_string().into(),
    ];
    db.execute(Statement::from_sql_and_values(
        backend,
        "INSERT INTO category_aliases (id, vault_id, category_id, alias, alias_norm) \
         VALUES (?, ?, ?, ?, ?);",
        values,
    ))
    .await?;
    Ok(())
}

async fn update_transaction_category(
    db: &SchemaManagerConnection<'_>,
    backend: DbBackend,
    transaction_id: Uuid,
    category_id: Uuid,
    category_display: Option<&str>,
) -> Result<(), DbErr> {
    let values = vec![
        category_id.as_bytes().to_vec().into(),
        match category_display {
            Some(value) => value.to_string().into(),
            None => Value::String(None),
        },
        transaction_id.as_bytes().to_vec().into(),
    ];
    db.execute(Statement::from_sql_and_values(
        backend,
        "UPDATE transactions SET category_id = ?, category = ? WHERE id = ?;",
        values,
    ))
    .await?;
    Ok(())
}

fn uuid_from_row(row: &QueryResult, column: &str) -> Result<Uuid, DbErr> {
    let bytes: Vec<u8> = row.try_get("", column)?;
    Uuid::from_slice(&bytes)
        .map_err(|err| DbErr::Custom(format!("invalid UUID in column {column}: {err}")))
}

fn normalize_display(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out = String::new();
    for token in trimmed.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(token);
    }
    if out.is_empty() { None } else { Some(out) }
}

fn normalize_key(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut out = String::new();
    let mut prev_space = false;
    for ch in trimmed.nfkd() {
        if is_combining_mark(ch) {
            continue;
        }
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            prev_space = false;
        } else if !out.is_empty() && !prev_space {
            out.push(' ');
            prev_space = true;
        }
    }
    let normalized = out.trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}

fn similarity_threshold(input: &str) -> usize {
    let len = input.chars().count();
    if len <= 6 { 1 } else { 2 }
}

fn find_similar_canonical<'a>(
    canonicals: &'a [CanonicalCategory],
    norm: &str,
) -> Option<(&'a CanonicalCategory, &'a str)> {
    let threshold = similarity_threshold(norm);
    let mut best: Option<(&CanonicalCategory, usize)> = None;

    for canonical in canonicals {
        let distance = levenshtein(norm, canonical.norm.as_str());
        if distance > threshold {
            continue;
        }
        let replace = match &best {
            None => true,
            Some((best_cat, best_distance)) => {
                distance < *best_distance
                    || (distance == *best_distance && canonical.norm.len() < best_cat.norm.len())
            }
        };
        if replace {
            best = Some((canonical, distance));
        }
    }

    best.map(|(canonical, _)| (canonical, canonical.norm.as_str()))
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
