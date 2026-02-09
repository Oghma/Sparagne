use api_types::category::{CategoryAliasView, CategoryMergePreviewResponse, CategoryView};

use super::selectable::{Resettable, SelectableList, UpdateFocus};
use crate::ui::forms::TextField;

#[derive(Debug)]
pub struct CategoriesState {
    pub selected: usize,
    pub mode: CategoriesMode,
    pub error: Option<String>,
    pub items: Vec<CategoryView>,
    pub merge: CategoryMergeState,
    pub form: CategoryFormState,
    pub aliases: CategoryAliasState,
}

impl SelectableList for CategoriesState {
    fn visible_count(&self) -> usize {
        self.items.len()
    }

    fn selected(&self) -> usize {
        self.selected
    }

    fn set_selected(&mut self, idx: usize) {
        self.selected = idx;
    }
}

impl Default for CategoriesState {
    fn default() -> Self {
        Self {
            selected: 0,
            mode: CategoriesMode::List,
            error: None,
            items: Vec::new(),
            merge: CategoryMergeState::default(),
            form: CategoryFormState::default(),
            aliases: CategoryAliasState::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategoriesMode {
    List,
    Merge,
    Create,
    Rename,
    Aliases,
}

#[derive(Debug, Default)]
pub struct CategoryMergeState {
    pub from_index: usize,
    pub target_index: usize,
    pub preview: Option<CategoryMergePreviewResponse>,
    pub confirming: bool,
}

#[derive(Debug, Clone)]
pub struct CategoryFormState {
    pub name: TextField,
}

impl Default for CategoryFormState {
    fn default() -> Self {
        Self {
            name: TextField::new("Name").required(true).min_length(1),
        }
    }
}

impl UpdateFocus for CategoryFormState {
    fn update_focus(&mut self) {
        self.name.state.focused = true;
    }
}

impl Resettable for CategoriesState {
    type Form = CategoryFormState;

    fn form_mut(&mut self) -> &mut Self::Form {
        &mut self.form
    }

    fn error_mut(&mut self) -> &mut Option<String> {
        &mut self.error
    }
}

impl CategoryFormState {
    /// Validates all fields and returns the first error message if any.
    pub(crate) fn validate_all(&mut self) -> Option<String> {
        self.name.validate();
        self.name.state.validation.error_message().map(String::from)
    }
}

#[derive(Debug)]
pub struct CategoryAliasState {
    pub items: Vec<CategoryAliasView>,
    pub selected: usize,
    pub input: String,
    pub error: Option<String>,
    pub focus: AliasFocus,
    pub category_id: Option<uuid::Uuid>,
}

impl SelectableList for CategoryAliasState {
    fn visible_count(&self) -> usize {
        self.items.len()
    }

    fn selected(&self) -> usize {
        self.selected
    }

    fn set_selected(&mut self, idx: usize) {
        self.selected = idx;
    }
}

impl Default for CategoryAliasState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
            input: String::new(),
            error: None,
            focus: AliasFocus::List,
            category_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasFocus {
    List,
    Input,
}
