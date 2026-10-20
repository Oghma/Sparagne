import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// What ⌫ and ⌦ delete while no cell has the caret (`DeleteTarget`): the
/// rows picked, else the row under the pointer, else nothing.
struct DeleteTargetTests {
    @Test("With rows picked, ⌫ deletes them, whatever row is under the pointer")
    func selectionComesFirst() {
        let hovered = TestRow.make("c")
        let target = DeleteTarget.resolve(selection: ["a", "b"], hovered: hovered, editing: false, canWrite: true)
        #expect(target == .selection(["a", "b"]))
    }

    @Test("With nothing picked, ⌫ deletes the row under the pointer, and only it")
    func hoveredRowAlone() {
        let hovered = TestRow.make("c")
        #expect(DeleteTarget.resolve(selection: [], hovered: hovered, editing: false, canWrite: true) == .row("c"))
    }

    @Test("Nothing picked and nothing under the pointer: nothing, and the key goes on")
    func nothingAtAll() {
        #expect(DeleteTarget.resolve(selection: [], hovered: nil, editing: false, canWrite: true) == .none)
    }

    @Test("While a row is open or a cell has the caret, the keys edit the text")
    func editingKeepsTheKeys() {
        let hovered = TestRow.make("c")
        #expect(DeleteTarget.resolve(selection: [], hovered: hovered, editing: true, canWrite: true) == .none)
        #expect(DeleteTarget.resolve(selection: ["a"], hovered: hovered, editing: true, canWrite: true) == .none)
    }

    @Test("A vault this account only reads deletes nothing, picked or pointed at")
    func readOnlyDeletesNothing() {
        let hovered = TestRow.make("c")
        #expect(DeleteTarget.resolve(selection: [], hovered: hovered, editing: false, canWrite: false) == .none)
        #expect(DeleteTarget.resolve(selection: ["a"], hovered: hovered, editing: false, canWrite: false) == .none)
    }

    @Test("A row deleted already is not deleted again from under the pointer")
    func deletedRowIsPassedOver() {
        let hovered = TestRow.make("c", voided: true)
        #expect(!DeleteTarget.isDeletable(hovered))
        #expect(DeleteTarget.resolve(selection: [], hovered: hovered, editing: false, canWrite: true) == .none)
    }

    // MARK: - ⌘⌫

    @Test("⌘⌫ deletes the rows picked, whatever row the cursor is on")
    func commandDeleteTakesTheSelectionFirst() {
        let cursorRow = TestRow.make("c")
        let target = DeleteTarget.resolveCommandDelete(
            selection: ["a", "b"],
            cursorRow: cursorRow,
            editing: false,
            canWrite: true
        )
        #expect(target == .selection(["a", "b"]))
    }

    @Test("⌘⌫ with nothing picked deletes the row the cursor is on")
    func commandDeleteTakesTheCursorRow() {
        let cursorRow = TestRow.make("c")
        #expect(DeleteTarget.resolveCommandDelete(selection: [], cursorRow: cursorRow, editing: false, canWrite: true) == .row("c"))
        let transfer = TestRow.make("t", kind: .transferFlow)
        #expect(DeleteTarget.resolveCommandDelete(selection: [], cursorRow: transfer, editing: false, canWrite: true) == .row("t"))
    }

    @Test("⌘⌫ deletes nothing on the new line, on a deleted row, while a cell is typed into, or on a vault only read")
    func commandDeleteDeletesNothing() {
        let cursorRow = TestRow.make("c")
        // The new line: no row under the cursor.
        #expect(DeleteTarget.resolveCommandDelete(selection: [], cursorRow: nil, editing: false, canWrite: true) == .none)
        let voided = TestRow.make("v", voided: true)
        #expect(DeleteTarget.resolveCommandDelete(selection: [], cursorRow: voided, editing: false, canWrite: true) == .none)
        #expect(DeleteTarget.resolveCommandDelete(selection: [], cursorRow: cursorRow, editing: true, canWrite: true) == .none)
        #expect(DeleteTarget.resolveCommandDelete(selection: ["a"], cursorRow: cursorRow, editing: true, canWrite: true) == .none)
        #expect(DeleteTarget.resolveCommandDelete(selection: [], cursorRow: cursorRow, editing: false, canWrite: false) == .none)
        #expect(DeleteTarget.resolveCommandDelete(selection: ["a"], cursorRow: cursorRow, editing: false, canWrite: false) == .none)
    }

    @Test("A transfer under the pointer is deleted alone, as its context menu deletes it")
    func transferAloneIsDeletable() {
        for kind in [TransactionKind.transferWallet, .transferFlow] {
            let hovered = TestRow.make("t", kind: kind)
            #expect(DeleteTarget.isDeletable(hovered))
            #expect(DeleteTarget.resolve(selection: [], hovered: hovered, editing: false, canWrite: true) == .row("t"))
        }
    }
}
