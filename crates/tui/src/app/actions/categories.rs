use super::super::*;

use crate::{
    error::Result,
    text::{TextKey, t},
};
use api_types::category::{CategoryCreate, CategoryUpdate};

impl App {
    pub(crate) async fn load_categories(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let res = self
            .client
            .categories_list(api_types::category::CategoryList {
                vault_id,
                include_archived: Some(true),
            })
            .await;

        match res {
            Ok(response) => {
                self.state.categories.items = response.categories;
                if self.state.categories.selected >= self.state.categories.items.len() {
                    self.state.categories.selected =
                        self.state.categories.items.len().saturating_sub(1);
                }
                if matches!(
                    self.state.categories.mode,
                    CategoriesMode::Merge | CategoriesMode::Aliases
                ) {
                    self.state.categories.mode = CategoriesMode::List;
                    self.state.categories.merge = CategoryMergeState::default();
                    self.reset_category_aliases();
                }
                self.state.categories.error = None;
                self.connection_ok(None);
                if let Some(category) = self.selected_category() {
                    self.load_category_aliases(category.id).await?;
                } else {
                    self.reset_category_aliases();
                }
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_connection(err) else {
                    return Ok(());
                };
                self.state.categories.error = Some(msg);
            }
        }
        Ok(())
    }
    pub(crate) async fn submit_category_merge(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let items_len = self.state.categories.items.len();
        if items_len < 2 {
            self.set_toast(
                t(self.state.locale, TextKey::PromptAtLeastTwoCategories),
                ToastLevel::Error,
            );
            return Ok(());
        }
        let from_index = self.state.categories.merge.from_index.min(items_len - 1);
        let target_index = self.state.categories.merge.target_index.min(items_len - 1);
        let Some(from) = self.state.categories.items.get(from_index) else {
            self.set_toast(
                t(self.state.locale, TextKey::PromptSourceCategoryInvalid),
                ToastLevel::Error,
            );
            return Ok(());
        };
        let Some(target) = self.state.categories.items.get(target_index) else {
            self.set_toast(
                t(self.state.locale, TextKey::PromptDestinationCategoryInvalid),
                ToastLevel::Error,
            );
            return Ok(());
        };
        let from_id = from.id;
        let target_id = target.id;

        if !self.state.categories.merge.confirming {
            let res = self
                .client
                .categories_merge_preview(
                    from_id,
                    api_types::category::CategoryMergePreview {
                        vault_id,
                        into_category_id: target_id,
                    },
                )
                .await;
            match res {
                Ok(preview) => {
                    self.state.categories.merge.preview = Some(preview);
                    if self
                        .state
                        .categories
                        .merge
                        .preview
                        .as_ref()
                        .map(|p| p.ok)
                        .unwrap_or(false)
                    {
                        self.state.categories.merge.confirming = true;
                        self.set_toast(
                            t(self.state.locale, TextKey::SuccessMergePreviewOk),
                            ToastLevel::Info,
                        );
                    } else {
                        self.state.categories.merge.confirming = false;
                        self.set_toast(
                            t(self.state.locale, TextKey::PromptCheckConflicts),
                            ToastLevel::Error,
                        );
                    }
                }
                Err(err) => {
                    let Some(msg) = self.on_api_error_connection(err) else {
                        return Ok(());
                    };
                    self.state.categories.error = Some(msg);
                }
            }
            return Ok(());
        }

        let res = self
            .client
            .categories_merge(
                from_id,
                api_types::category::CategoryMerge {
                    vault_id,
                    into_category_id: target_id,
                },
            )
            .await;
        match res {
            Ok(_) => {
                self.state.categories.mode = CategoriesMode::List;
                self.state.categories.merge = CategoryMergeState::default();
                self.load_categories().await?;
                self.set_toast(
                    t(self.state.locale, TextKey::SuccessMergeCompleted),
                    ToastLevel::Success,
                );
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_connection(err) else {
                    return Ok(());
                };
                self.state.categories.error = Some(msg);
            }
        }
        Ok(())
    }
    pub(crate) async fn start_category_aliases(&mut self) -> Result<()> {
        let Some(category_id) = self.selected_category().map(|category| category.id) else {
            self.set_toast(
                t(self.state.locale, TextKey::PromptNoCategorySelected),
                ToastLevel::Error,
            );
            return Ok(());
        };
        self.state.categories.mode = CategoriesMode::Aliases;
        self.reset_category_aliases();
        self.load_category_aliases(category_id).await?;
        Ok(())
    }
    pub(crate) async fn reload_category_aliases(&mut self) -> Result<()> {
        let Some(category_id) = self.selected_category().map(|category| category.id) else {
            return Ok(());
        };
        self.load_category_aliases(category_id).await
    }
    pub(crate) async fn load_category_aliases(&mut self, category_id: uuid::Uuid) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let res = self
            .client
            .category_aliases_list(
                category_id,
                api_types::category::CategoryAliasList { vault_id },
            )
            .await;

        match res {
            Ok(response) => {
                self.state.categories.aliases.items = response.aliases;
                self.state.categories.aliases.category_id = Some(category_id);
                if self.state.categories.aliases.selected
                    >= self.state.categories.aliases.items.len()
                {
                    self.state.categories.aliases.selected =
                        self.state.categories.aliases.items.len().saturating_sub(1);
                }
                self.state.categories.aliases.error = None;
                self.connection_ok(None);
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_connection(err) else {
                    return Ok(());
                };
                self.state.categories.aliases.error = Some(msg);
            }
        }
        Ok(())
    }
    pub(crate) async fn submit_category_alias_create(&mut self) -> Result<()> {
        if self.state.categories.aliases.focus == AliasFocus::List {
            self.state.categories.aliases.focus = AliasFocus::Input;
            return Ok(());
        }

        let Some(category_id) = self.selected_category().map(|category| category.id) else {
            self.state.categories.aliases.error =
                Some(t(self.state.locale, TextKey::PromptNoCategorySelected).to_string());
            return Ok(());
        };
        let alias = self.state.categories.aliases.input.trim().to_string();
        if alias.is_empty() {
            self.state.categories.aliases.error =
                Some(t(self.state.locale, TextKey::PromptEnterAlias).to_string());
            return Ok(());
        }

        let res = self
            .client
            .category_alias_create(
                category_id,
                api_types::category::CategoryAliasCreate {
                    vault_id: self.current_vault_id()?,
                    alias,
                },
            )
            .await;

        match res {
            Ok(_) => {
                self.state.categories.aliases.input.clear();
                self.state.categories.aliases.focus = AliasFocus::List;
                self.load_category_aliases(category_id).await?;
                self.set_toast(
                    t(self.state.locale, TextKey::SuccessAliasCreated),
                    ToastLevel::Success,
                );
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorCreateAlias) else {
                    return Ok(());
                };
                self.state.categories.aliases.error = Some(msg);
            }
        }

        Ok(())
    }
    pub(crate) async fn delete_category_alias(&mut self) -> Result<()> {
        let Some(category_id) = self.selected_category().map(|category| category.id) else {
            self.state.categories.aliases.error =
                Some(t(self.state.locale, TextKey::PromptNoCategorySelected).to_string());
            return Ok(());
        };
        let Some(alias_id) = self
            .state
            .categories
            .aliases
            .items
            .get(self.state.categories.aliases.selected)
            .map(|alias| alias.id)
        else {
            self.state.categories.aliases.error =
                Some(t(self.state.locale, TextKey::PromptNoAliasSelected).to_string());
            return Ok(());
        };

        let res = self
            .client
            .category_alias_delete(
                category_id,
                alias_id,
                api_types::category::CategoryAliasDelete {
                    vault_id: self.current_vault_id()?,
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.load_category_aliases(category_id).await?;
                self.set_toast(
                    t(self.state.locale, TextKey::SuccessAliasDeleted),
                    ToastLevel::Success,
                );
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorDeleteAlias) else {
                    return Ok(());
                };
                self.state.categories.aliases.error = Some(msg);
            }
        }

        Ok(())
    }
    pub(crate) async fn submit_category_create(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;

        // Validate the form
        if let Some(err) = self.state.categories.form.validate_all() {
            self.state.categories.error = Some(err);
            return Ok(());
        }

        let name = self.state.categories.form.name.value().trim();
        if name.is_empty() {
            self.state.categories.error =
                Some(t(self.state.locale, TextKey::PromptEnterName).to_string());
            return Ok(());
        }

        let res = self
            .client
            .categories_create(CategoryCreate {
                vault_id,
                name: name.to_string(),
            })
            .await;

        match res {
            Ok(created) => {
                self.reset_category_form();
                self.state.categories.mode = CategoriesMode::List;
                self.load_categories().await?;
                self.select_category_by_id(created.id);
                self.set_toast(
                    t(self.state.locale, TextKey::SuccessCategoryCreated),
                    ToastLevel::Success,
                );
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorCreateCategory) else {
                    return Ok(());
                };
                self.state.categories.error = Some(msg);
            }
        }

        Ok(())
    }
    pub(crate) async fn submit_category_rename(&mut self) -> Result<()> {
        let Some((category_id, is_system)) = self.selected_category().map(|c| (c.id, c.is_system))
        else {
            self.state.categories.error =
                Some(t(self.state.locale, TextKey::PromptNoCategorySelected).to_string());
            return Ok(());
        };
        if is_system {
            self.state.categories.error = Some(
                t(
                    self.state.locale,
                    TextKey::ValidationSystemCategoryImmutable,
                )
                .to_string(),
            );
            return Ok(());
        }

        // Validate the form
        if let Some(err) = self.state.categories.form.validate_all() {
            self.state.categories.error = Some(err);
            return Ok(());
        }

        let name = self.state.categories.form.name.value().trim();
        if name.is_empty() {
            self.state.categories.error =
                Some(t(self.state.locale, TextKey::PromptEnterName).to_string());
            return Ok(());
        }

        let res = self
            .client
            .categories_update(
                category_id,
                CategoryUpdate {
                    vault_id: self.current_vault_id()?,
                    name: Some(name.to_string()),
                    archived: None,
                },
            )
            .await;

        match res {
            Ok(_) => {
                self.reset_category_form();
                self.state.categories.mode = CategoriesMode::List;
                self.load_categories().await?;
                self.set_toast(
                    t(self.state.locale, TextKey::SuccessCategoryUpdated),
                    ToastLevel::Success,
                );
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorUpdateCategory) else {
                    return Ok(());
                };
                self.state.categories.error = Some(msg);
            }
        }

        Ok(())
    }
    pub(crate) async fn toggle_category_archive(&mut self) -> Result<()> {
        let Some(category) = self.selected_category() else {
            self.state.categories.error =
                Some(t(self.state.locale, TextKey::PromptNoCategorySelected).to_string());
            return Ok(());
        };
        if category.is_system {
            self.state.categories.error = Some(
                t(
                    self.state.locale,
                    TextKey::ValidationSystemCategoryImmutable,
                )
                .to_string(),
            );
            return Ok(());
        }

        let res = self
            .client
            .categories_update(
                category.id,
                CategoryUpdate {
                    vault_id: self.current_vault_id()?,
                    name: None,
                    archived: Some(!category.archived),
                },
            )
            .await;

        match res {
            Ok(_) => {
                self.load_categories().await?;
                self.set_toast(
                    t(self.state.locale, TextKey::SuccessCategoryUpdated),
                    ToastLevel::Success,
                );
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorArchiveCategory) else {
                    return Ok(());
                };
                self.state.categories.error = Some(msg);
            }
        }

        Ok(())
    }
}
