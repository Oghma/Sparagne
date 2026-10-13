import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The RIEPILOGO's strip of things to do, each with the tab that deals with
/// it.
struct SummaryTodoTests {
    private static func uncategorized(count: UInt32) -> CategoryTotals {
        SummaryCategoriesTests.totals("Uncategorized", netExpense: 1_000, count: count, isSystem: true)
    }

    @Test("Nothing waiting, no strip")
    func nothing() {
        let items = SummaryTodo.items(
            dueRecurring: 0,
            pendingAllocation: nil,
            yearCategories: [SummaryCategoriesTests.totals("Casa", netExpense: 1_000, count: 4)]
        )
        #expect(items.isEmpty)
    }

    @Test("The due periods, the allocation's period and the rows without a category, in this order")
    func allThree() {
        let items = SummaryTodo.items(
            dueRecurring: 3,
            pendingAllocation: PendingAllocation(planId: "plan", periodDate: "2026-10-01", missed: 2),
            yearCategories: [
                SummaryCategoriesTests.totals("Casa", netExpense: 50_000, count: 4),
                Self.uncategorized(count: 7),
            ]
        )
        #expect(
            items.map(\.kind) == [
                .dueRecurring(count: 3),
                .pendingAllocation(period: "2026-10-01", missed: 2),
                .uncategorized(count: 7),
            ]
        )
        #expect(items.map(\.tab) == [.recurring, .allocation, .ledger])
        #expect(items.map(\.id) == ["recurring", "allocation", "uncategorized"])
    }

    @Test("Uncategorized with no rows in the year is nothing to do")
    func uncategorizedWithoutRows() {
        #expect(SummaryTodo.items(dueRecurring: 0, pendingAllocation: nil, yearCategories: [Self.uncategorized(count: 0)]).isEmpty)
    }

    @Test("A period of the allocation plan due today, with none missed before it")
    func allocationAlone() {
        let items = SummaryTodo.items(
            dueRecurring: 0,
            pendingAllocation: PendingAllocation(planId: "plan", periodDate: "2026-10-01", missed: 0),
            yearCategories: []
        )
        #expect(items.map(\.kind) == [.pendingAllocation(period: "2026-10-01", missed: 0)])
    }
}
