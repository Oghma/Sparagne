use api_types::category::{CategoryAliasView, CategoryMergePreviewResponse, CategoryView};

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

#[allow(dead_code)]
impl CategoryFormState {
    /// Updates focus state on all fields.
    pub fn update_focus(&mut self, focused: bool) {
        self.name.state.focused = focused;
    }

    /// Returns true if all fields are valid.
    pub fn is_valid(&self) -> bool {
        self.name.state.validation.is_valid()
    }

    /// Validates all fields and returns the first error message if any.
    pub fn validate_all(&mut self) -> Option<String> {
        self.name.validate();
        self.name.state.validation.error_message().map(String::from)
    }

    /// Clears the form and resets to default state.
    pub fn clear(&mut self) {
        self.name.clear();
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
