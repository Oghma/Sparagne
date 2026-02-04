use super::super::*;

use api_types::category::CategoryView;
use crate::text::{TextKey, t};
impl App {
    pub(crate) fn categories_select_next(&mut self) {
        let len = self.state.categories.items.len();
        if len == 0 {
            return;
        }
        self.state.categories.selected = (self.state.categories.selected + 1).min(len - 1);
        self.state.categories.aliases.category_id = None;
        self.state.categories.aliases.items.clear();
        self.state.categories.aliases.selected = 0;
    }

    pub(crate) fn categories_select_prev(&mut self) {
        if self.state.categories.items.is_empty() {
            return;
        }
        self.state.categories.selected = self.state.categories.selected.saturating_sub(1);
        self.state.categories.aliases.category_id = None;
        self.state.categories.aliases.items.clear();
        self.state.categories.aliases.selected = 0;
    }
    pub(crate) fn category_merge_select_next(&mut self) {
        let len = self.state.categories.items.len();
        if len < 2 {
            return;
        }
        let from = self.state.categories.merge.from_index;
        let mut idx = self.state.categories.merge.target_index;
        loop {
            idx = (idx + 1) % len;
            if idx != from {
                break;
            }
        }
        self.state.categories.merge.target_index = idx;
        self.state.categories.merge.preview = None;
        self.state.categories.merge.confirming = false;
    }

    pub(crate) fn category_merge_select_prev(&mut self) {
        let len = self.state.categories.items.len();
        if len < 2 {
            return;
        }
        let from = self.state.categories.merge.from_index;
        let mut idx = self.state.categories.merge.target_index;
        loop {
            idx = (idx + len - 1) % len;
            if idx != from {
                break;
            }
        }
        self.state.categories.merge.target_index = idx;
        self.state.categories.merge.preview = None;
        self.state.categories.merge.confirming = false;
    }
    pub(crate) fn category_alias_select_next(&mut self) {
        let len = self.state.categories.aliases.items.len();
        if len == 0 {
            return;
        }
        self.state.categories.aliases.selected =
            (self.state.categories.aliases.selected + 1).min(len - 1);
    }

    pub(crate) fn category_alias_select_prev(&mut self) {
        if self.state.categories.aliases.items.is_empty() {
            return;
        }
        self.state.categories.aliases.selected =
            self.state.categories.aliases.selected.saturating_sub(1);
    }

    pub(crate) fn toggle_alias_focus(&mut self) {
        let focus = self.state.categories.aliases.focus;
        self.state.categories.aliases.focus = match focus {
            AliasFocus::List => AliasFocus::Input,
            AliasFocus::Input => AliasFocus::List,
        };
    }
    pub(crate) fn start_category_merge(&mut self) {
        let len = self.state.categories.items.len();
        if len < 2 {
            self.set_toast(t(self.state.locale, TextKey::PromptAtLeastTwoCategories), ToastLevel::Error);
            return;
        }

        let from_index = self.state.categories.selected.min(len - 1);
        let mut target_index = (from_index + 1) % len;
        if target_index == from_index {
            target_index = 0;
        }
        self.state.categories.mode = CategoriesMode::Merge;
        self.state.categories.merge = CategoryMergeState {
            from_index,
            target_index,
            preview: None,
            confirming: false,
        };
    }

    pub(crate) fn start_category_create(&mut self) {
        self.reset_category_form();
        self.state.categories.mode = CategoriesMode::Create;
        self.reset_category_aliases();
    }

    pub(crate) fn start_category_rename(&mut self) {
        let Some((_category_id, name, is_system)) = self
            .selected_category()
            .map(|category| (category.id, category.name.clone(), category.is_system))
        else {
            self.state.categories.error = Some(t(self.state.locale, TextKey::PromptNoCategorySelected).to_string());
            return;
        };
        if is_system {
            self.state.categories.error =
                Some(t(self.state.locale, TextKey::ValidationSystemCategoryImmutable).to_string());
            return;
        }
        self.reset_category_form();
        self.state.categories.form.name.set_value(name);
        self.state.categories.mode = CategoriesMode::Rename;
        self.reset_category_aliases();
    }
    pub(crate) fn selected_category(&self) -> Option<&CategoryView> {
        self.state
            .categories
            .items
            .get(self.state.categories.selected)
    }

    pub(crate) fn select_category_by_id(&mut self, category_id: uuid::Uuid) {
        if let Some(pos) = self
            .state
            .categories
            .items
            .iter()
            .position(|category| category.id == category_id)
        {
            self.state.categories.selected = pos;
        }
    }
}
