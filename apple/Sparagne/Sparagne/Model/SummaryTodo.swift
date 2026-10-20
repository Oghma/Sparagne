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
    /// `dueRecurring` is `AppStore.actionableDueCount`, `pendingAllocation`
    /// the plan's period due today (`nil` on a vault the account only reads:
    /// neither can be decided there), `yearCategories` the year's category
    /// totals, whose system row Uncategorized counts the rows without one.
    ///
    /// Those totals are read with the PERSONA filter, so under it the count
    /// is that person's rows without a category. The chip opens the oldest
    /// of them, looked up in the same year under the same filter
    /// (`AppStore.revealFirstUncategorized`): the Mastro alone shows one
    /// month and one direction, which rarely hold them.
    static func items(
        dueRecurring: Int,
        pendingAllocation: PendingAllocation?,
        yearCategories: [CategoryTotals]
    ) -> [SummaryTodoItem] {
        var items: [SummaryTodoItem] = []
        if dueRecurring > 0 {
            items.append(SummaryTodoItem(kind: .dueRecurring(count: dueRecurring), tab: .recurring))
        }
        if let pending = pendingAllocation {
            items.append(
                SummaryTodoItem(
                    kind: .pendingAllocation(period: pending.periodDate, missed: Int(pending.missed)),
                    tab: .allocation
                )
            )
        }
        let uncategorized = yearCategories
            .filter { TransactionRow.isUncategorized($0.name, isSystem: $0.isSystem) }
            .reduce(0) { $0 + Int($1.count) }
        if uncategorized > 0 {
            items.append(SummaryTodoItem(kind: .uncategorized(count: uncategorized), tab: .ledger))
        }
        return items
    }
}
