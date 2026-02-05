//! Bulk category dialog state.

use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct BulkCategoryDialogState {
    pub(crate) transaction_ids: Vec<Uuid>,
    pub(crate) count: usize,
    pub(crate) input: String,
    pub(crate) error: Option<String>,
}
