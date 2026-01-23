use api_types::category::{CategoryAliasView, CategoryMergePreviewResponse, CategoryView};

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

#[derive(Debug, Default)]
pub struct CategoryFormState {
    pub name: String,
    pub error: Option<String>,
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
