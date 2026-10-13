import Foundation
import SparagneCore

/// One thing waiting on the RIEPILOGO's strip, with the tab that deals
/// with it.
struct SummaryTodoItem: Identifiable, Hashable, Sendable {
    enum Kind: Hashable, Sendable {
        /// Recurring periods waiting for a decision, over every template.
        case dueRecurring(count: Int)
        /// The allocation plan's period waiting for a decision, and the
        /// older ones deciding it closes too.
        case pendingAllocation(period: NaiveDate, missed: Int)
        /// Rows of the year so far filed under Uncategorized.
        case uncategorized(count: Int)
    }

    let kind: Kind
    /// Where a click goes.
    let tab: LedgerTab

    var id: String {
        switch kind {
        case .dueRecurring: "recurring"
        case .pendingAllocation: "allocation"
        case .uncategorized: "uncategorized"
        }
    }
}

/// The RIEPILOGO's strip of things to do; empty, the strip is not drawn.
enum SummaryTodo {
    /// `dueRecurring` is `AppStore.dueRecurringCount`, `pendingAllocation`
    /// the plan's period due today, `yearCategories` the year's category
    /// totals, whose system row Uncategorized counts the rows without one.
    static func items(
        dueRecurring: Int,
        pendingAllocation: PendingAllocation?,
        yearCategories: [CategoryTotals]
    ) -> [SummaryTodoItem] {
        // A stub for now.
        _ = (dueRecurring, pendingAllocation, yearCategories)
        return []
    }
}
