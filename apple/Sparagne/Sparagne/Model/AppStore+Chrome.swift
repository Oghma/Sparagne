import Foundation
import SparagneCore

/// What the window's chrome reads off the store: the top bar's due pill and
/// the Ricorrenze tab's count.
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

    /// One of the RIEPILOGO's top expenses was clicked: the Mastro on the
    /// row's month, the row selected and scrolled into view once
    /// (`revealedRow`, which the grid consumes).
    func reveal(transactionId: Uuid, occurredAt: OffsetDateTime) {
        // A stub for now.
        _ = (transactionId, occurredAt)
    }
}
