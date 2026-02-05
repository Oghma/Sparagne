//! Trait for list selection behavior.
//!
//! This module provides a common interface for navigating selectable lists,
//! reducing duplication across wallets, flows, categories, members, and overlay
//! states.

/// A selectable list provides selection state and navigation methods.
///
/// This trait defines a common pattern for list selection that can be shared
/// across different state types. The `visible_count` method must be provided
/// by implementations since some lists depend on external filtering logic.
pub(crate) trait SelectableList {
    /// Returns the number of visible items in the list.
    fn visible_count(&self) -> usize;

    /// Returns the current selection index.
    fn selected(&self) -> usize;

    /// Sets the selection index.
    fn set_selected(&mut self, idx: usize);

    /// Moves selection to the next item, stopping at the last item.
    fn select_next(&mut self) {
        let len = self.visible_count();
        if len == 0 {
            return;
        }
        self.set_selected((self.selected() + 1).min(len - 1));
    }

    /// Moves selection to the previous item, stopping at the first item.
    fn select_prev(&mut self) {
        if self.visible_count() == 0 {
            return;
        }
        self.set_selected(self.selected().saturating_sub(1));
    }

    /// Ensures selection is within valid bounds after list contents change.
    fn clamp_selection(&mut self) {
        let len = self.visible_count();
        if len == 0 {
            self.set_selected(0);
        } else if self.selected() >= len {
            self.set_selected(len - 1);
        }
    }
}

/// Wrapper that provides visible count to types that need external context.
///
/// Used for wallets and flows where visible indices depend on the full app state.
pub(crate) struct SelectableWithCount<'a, T> {
    inner: &'a mut T,
    count: usize,
}

impl<'a, T> SelectableWithCount<'a, T> {
    pub(crate) fn new(inner: &'a mut T, count: usize) -> Self {
        Self { inner, count }
    }
}

impl<T: HasSelection> SelectableList for SelectableWithCount<'_, T> {
    fn visible_count(&self) -> usize {
        self.count
    }

    fn selected(&self) -> usize {
        self.inner.selected()
    }

    fn set_selected(&mut self, idx: usize) {
        self.inner.set_selected(idx);
    }
}

/// Trait for types that have selection state but may need external count.
pub(crate) trait HasSelection {
    fn selected(&self) -> usize;
    fn set_selected(&mut self, idx: usize);
}

/// Trait for types that have an archive visibility toggle.
pub(crate) trait HasArchiveToggle: HasSelection {
    fn show_archived(&self) -> bool;
    fn set_show_archived(&mut self, show: bool);

    fn toggle_show_archived(&mut self) {
        self.set_show_archived(!self.show_archived());
    }
}

/// Trait for form state types that support focus update.
pub(crate) trait UpdateFocus {
    fn update_focus(&mut self);
}

/// Trait for parent state types that have a form and error that can be reset.
pub(crate) trait Resettable {
    type Form: Default + UpdateFocus;

    fn form_mut(&mut self) -> &mut Self::Form;
    fn error_mut(&mut self) -> &mut Option<String>;

    fn reset_form(&mut self) {
        *self.form_mut() = Self::Form::default();
        self.form_mut().update_focus();
        *self.error_mut() = None;
    }
}
