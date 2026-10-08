import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// What ⌫ and ⌦ delete while no cell has the caret (`DeleteTarget`): the
/// rows picked, else the row under the pointer, else nothing.
struct DeleteTargetTests {
    private static func row(
        _ id: Uuid = UUID().uuidString,
        kind: TransactionKind = .expense,
        voided: Bool = false
    ) -> TransactionRow {
        let isTransfer = kind == .transferWallet || kind == .transferFlow
        let view = TransactionView(
            id: id,
            kind: kind,
            occurredAt: CoreDate.offset(Date()),
            amount: 1_250,
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

    @Test("With rows picked, ⌫ deletes them, whatever row is under the pointer")
    func selectionComesFirst() {
        let hovered = Self.row("c")
        let target = DeleteTarget.resolve(selection: ["a", "b"], hovered: hovered, editing: false, canWrite: true)
        #expect(target == .selection(["a", "b"]))
    }

    @Test("With nothing picked, ⌫ deletes the row under the pointer, and only it")
    func hoveredRowAlone() {
        let hovered = Self.row("c")
        #expect(DeleteTarget.resolve(selection: [], hovered: hovered, editing: false, canWrite: true) == .row("c"))
    }

    @Test("Nothing picked and nothing under the pointer: nothing, and the key goes on")
    func nothingAtAll() {
        #expect(DeleteTarget.resolve(selection: [], hovered: nil, editing: false, canWrite: true) == .none)
    }

    @Test("While a row is open or a cell has the caret, the keys edit the text")
    func editingKeepsTheKeys() {
        let hovered = Self.row("c")
        #expect(DeleteTarget.resolve(selection: [], hovered: hovered, editing: true, canWrite: true) == .none)
        #expect(DeleteTarget.resolve(selection: ["a"], hovered: hovered, editing: true, canWrite: true) == .none)
    }

    @Test("A vault this account only reads deletes nothing, picked or pointed at")
    func readOnlyDeletesNothing() {
        let hovered = Self.row("c")
        #expect(DeleteTarget.resolve(selection: [], hovered: hovered, editing: false, canWrite: false) == .none)
        #expect(DeleteTarget.resolve(selection: ["a"], hovered: hovered, editing: false, canWrite: false) == .none)
    }

    @Test("A row deleted already is not deleted again from under the pointer")
    func deletedRowIsPassedOver() {
        let hovered = Self.row("c", voided: true)
        #expect(!DeleteTarget.isDeletable(hovered))
        #expect(DeleteTarget.resolve(selection: [], hovered: hovered, editing: false, canWrite: true) == .none)
    }

    @Test("A transfer under the pointer is deleted alone, as its context menu deletes it")
    func transferAloneIsDeletable() {
        for kind in [TransactionKind.transferWallet, .transferFlow] {
            let hovered = Self.row("t", kind: kind)
            #expect(DeleteTarget.isDeletable(hovered))
            #expect(DeleteTarget.resolve(selection: [], hovered: hovered, editing: false, canWrite: true) == .row("t"))
        }
    }
}
