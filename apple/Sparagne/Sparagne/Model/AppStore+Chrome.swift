import Foundation

/// What the window's chrome reads off the store: the top bar's due pill and
/// the Ricorrenze tab's count (`docs/v2/UI.md` §2.5).
extension AppStore {
    /// Every period waiting for a decision, over all the templates: a
    /// template three months behind counts three times, since each period is
    /// confirmed or skipped on its own.
    var dueRecurringCount: Int {
        pendingRecurringItems.reduce(0) { $0 + $1.due.count }
    }

    /// The due count the chrome shows: none on a vault the account only
    /// reads, where nothing can be confirmed or skipped.
    var actionableDueCount: Int {
        isReadOnly ? 0 : dueRecurringCount
    }
}
