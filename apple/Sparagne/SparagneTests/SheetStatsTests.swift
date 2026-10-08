import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The Mastro's status line figures (`SheetStats`): what is summed, over
/// which rows.
struct SheetStatsTests {
    private static func row(
        _ amount: Int64,
        kind: TransactionKind = .expense,
        voided: Bool = false
    ) -> TransactionRow {
        let isTransfer = kind == .transferWallet || kind == .transferFlow
        let view = TransactionView(
            id: UUID().uuidString,
            kind: kind,
            occurredAt: CoreDate.offset(Date()),
            amount: amount,
            categoryId: UUID().uuidString,
            category: "Spesa",
            categoryIsSystem: false,
            note: nil,
            person: "matteo",
            createdBy: "matteo",
            voided: voided,
            walletId: isTransfer ? nil : UUID().uuidString,
            flowId: isTransfer ? nil : UUID().uuidString,
            fromId: isTransfer ? UUID().uuidString : nil,
            toId: isTransfer ? UUID().uuidString : nil,
            legs: []
        )
        return TransactionRow(view: view, names: NameBook(snapshot: nil))
    }

    private static func selecting(_ rows: [TransactionRow]) -> RowSelection {
        var selection = RowSelection()
        for row in rows { selection.toggle(row.id) }
        return selection
    }

    @Test("With nothing picked, the figures are the visible rows': count, sum and the mean rounded to the cent")
    func visibleRows() {
        let rows = [Self.row(1_000), Self.row(2_000), Self.row(4_001)]
        let stats = SheetStats.make(rows: rows, selection: RowSelection())
        #expect(stats.scope == .visible)
        #expect(stats.count == 3)
        #expect(stats.sum == 7_001)
        // 7001 / 3 = 2333.67, rounded half away from zero.
        #expect(stats.mean == 2_334)
    }

    @Test("Two or more picked rows narrow the figures to the selection")
    func selectionOfTwo() {
        let rows = [Self.row(1_000), Self.row(2_000), Self.row(4_000)]
        let stats = SheetStats.make(rows: rows, selection: Self.selecting(Array(rows.suffix(2))))
        #expect(stats.scope == .selection)
        #expect(stats.count == 2)
        #expect(stats.sum == 6_000)
        #expect(stats.mean == 3_000)
    }

    @Test("One picked row is not a range: the figures stay on the visible rows")
    func selectionOfOne() {
        let rows = [Self.row(1_000), Self.row(2_000)]
        let stats = SheetStats.make(rows: rows, selection: Self.selecting([rows[0]]))
        #expect(stats.scope == .visible)
        #expect(stats.count == 2)
        #expect(stats.sum == 3_000)
    }

    @Test("A refund comes off the sum and still counts; a transfer and a voided row are left out")
    func refundsTransfersVoided() {
        let rows = [
            Self.row(10_000, kind: .expense),
            Self.row(2_500, kind: .refund),
            Self.row(50_000, kind: .transferWallet),
            Self.row(7_000, kind: .transferFlow),
            Self.row(99_999, kind: .expense, voided: true),
        ]
        let stats = SheetStats.make(rows: rows, selection: RowSelection())
        #expect(stats.count == 2)
        #expect(stats.sum == 7_500)
        #expect(stats.mean == 3_750)

        // Picked, the same rules hold.
        let picked = SheetStats.make(rows: rows, selection: Self.selecting(Array(rows.suffix(3))))
        #expect(picked.scope == .selection)
        #expect(picked.count == 0)
        #expect(picked.sum == 0)
        #expect(picked.mean == nil)
    }

    @Test("A month that is all refunds averages below zero, rounded away from zero")
    func negativeMean() {
        let rows = [Self.row(1_001, kind: .refund), Self.row(1_000, kind: .refund)]
        let stats = SheetStats.make(rows: rows, selection: RowSelection())
        #expect(stats.sum == -2_001)
        #expect(stats.mean == -1_001)
    }

    @Test("An empty sheet counts nothing, sums to zero and has no mean")
    func emptySheet() {
        let stats = SheetStats.make(rows: [], selection: RowSelection())
        #expect(stats == SheetStats(scope: .visible, count: 0, sum: 0, mean: nil))
    }
}
