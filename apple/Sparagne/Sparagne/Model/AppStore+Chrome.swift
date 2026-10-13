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
    ///
    /// The list has to be one the row is in: USCITE, with nothing searched.
    /// PERSONA stays as it is, since the top expenses were read with it. The
    /// selection waits for the reload the month or the direction queue, and
    /// `reload()` makes it; on a vault only read there is none, and the grid
    /// only scrolls.
    func reveal(transactionId: Uuid, occurredAt: OffsetDateTime) {
        guard let date = CoreDate.date(occurredAt) else { return }
        let target = MonthKey(date)
        let queuesReload = month != target || direction != .expenses
        let searched = !searchText.isEmpty
        revealedRow = transactionId
        tab = .ledger
        direction = .expenses
        searchText = ""
        month = target
        guard !queuesReload else { return }
        // The month on screen already: a search narrowed it, and the list
        // has to come back whole; otherwise the row is there now.
        if searched {
            scheduleReload()
        } else {
            selectRevealedRow()
        }
    }
}
