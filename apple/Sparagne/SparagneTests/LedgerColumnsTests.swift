import AppKit
import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The ledger's column widths: dragged in the header, fitted by a double
/// click, kept for the next launch and put back by View > Reset Column Widths.
struct LedgerColumnsTests {
    @Test("A column opens at its standard width; a drag is rounded and kept between the minimum and the maximum")
    func clamping() {
        var widths = LedgerColumnWidths()
        #expect(widths.isStandard)
        #expect(widths[.flow] == LedgerColumn.flow.standard)
        #expect(widths.outer(.flow) == 84)

        widths[.flow] = 140.4
        #expect(widths[.flow] == 140)
        #expect(!widths.isStandard)

        widths[.category] = 3
        #expect(widths[.category] == LedgerColumn.minimum)
        widths[.person] = 5_000
        #expect(widths[.person] == LedgerColumn.maximum)

        // Dragged back onto the standard, a column is no longer a change.
        widths = LedgerColumnWidths()
        widths[.amount] = 120
        widths[.amount] = LedgerColumn.amount.standard
        #expect(widths.isStandard)
    }

    @Test("Only the changed columns are stored, and anything unreadable reads as the standard")
    func storedForm() {
        var widths = LedgerColumnWidths()
        widths[.flow] = 150
        #expect(widths.stored == ["flow": 150])

        let read = LedgerColumnWidths(stored: ["flow": 150, "category": "wide", "colour": 12, "date": 2])
        #expect(read[.flow] == 150)
        #expect(read[.category] == LedgerColumn.category.standard)
        #expect(read[.date] == LedgerColumn.minimum)
    }

    @Test("The columns after the description grow to the left, so their edge follows the pointer")
    func handleSides() {
        #expect(LedgerColumn.allCases.filter(\.growsLeft) == [.wallet, .person, .amount])
    }

    // MARK: - Over a store

    @Test("The widths are kept for the next launch, and Reset Column Widths puts them all back")
    func persistence() async throws {
        let defaults = try TestStore.defaults()
        let store = try await TestStore.make(defaults: defaults)
        store.ledgerColumns[.category] = 200
        store.saveLedgerColumns()

        let relaunched = try TestStore.fresh(defaults: defaults)
        #expect(relaunched.ledgerColumns[.category] == 200)

        relaunched.resetLedgerColumns()
        #expect(relaunched.ledgerColumns.isStandard)
        let again = try TestStore.fresh(defaults: defaults)
        #expect(again.ledgerColumns.isStandard)
    }

    @Test("A double click fits the column to the widest thing in it, its heading included")
    func fitting() async throws {
        let store = try await TestStore.make()
        await store.createEnvelope(name: "Fondo emergenza per la macchina", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        let long = try #require(store.flows.first { $0.name.hasPrefix("Fondo") })
        await store.addRow(day: Date(), flowId: long.id, category: "Bar", note: "caffè", amount: 150)
        await store.addRow(day: Date(), flowId: long.id, category: "Bar", note: "rimborso", amount: 1_234_567, kind: .refund)
        #expect(store.presentedError == nil)

        let font = NSFont.monospacedDigitSystemFont(ofSize: 12, weight: .regular)
        let fit = { (column: LedgerColumn) in
            LedgerColumnFit.width(of: column, rows: store.rows, due: store.duePeriods, store: store)
        }

        // The long name, the select's mark beside it, a little air.
        let name = LedgerColumnFit.width(of: long.name, font: font)
        #expect(fit(.flow) == LedgerColumn.clamp(name + 12 + 2))
        #expect(fit(.flow) > LedgerColumn.flow.standard)

        // The refund's minus counts in the figure.
        let refund = LedgerColumnFit.width(of: "\u{2212}\(LedgerMoney.bare(1_234_567))", font: font)
        #expect(fit(.amount) >= refund)

        // "Bar" is shorter than its heading: the heading wins.
        let heading = LedgerColumnFit.width(
            of: RowField.category.label,
            font: NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .medium)
        )
        #expect(fit(.category) == LedgerColumn.clamp(max(heading, LedgerColumnFit.width(of: "Bar", font: font)) + 2))
    }
}
