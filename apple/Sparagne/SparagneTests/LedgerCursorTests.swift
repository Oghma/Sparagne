import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Where the Mastro's cursor goes on each key, and which cells can be typed
/// into (`LedgerCursor`): the lines are the stored rows, then the new line
/// while the vault can be written to.
struct LedgerCursorTests {
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

    /// Rows `a`, `b` and `c`, top to bottom.
    private static func cursor(canWrite: Bool = true, showsWallet: Bool = false) -> LedgerCursor {
        LedgerCursor(rows: [row("a"), row("b"), row("c")], canWrite: canWrite, showsWallet: showsWallet)
    }

    private static func cell(_ row: Uuid?, _ field: RowField) -> CellFocus {
        CellFocus(row: row, field: field)
    }

    /// Where `move` pressed over and over from `start` goes, one cell per
    /// press.
    private static func walk(_ cursor: LedgerCursor, _ move: CursorMove, from start: CellFocus, times: Int) -> [CellFocus] {
        var cells: [CellFocus] = []
        var at = start
        for _ in 0..<times {
            at = cursor.target(of: move, from: at)
            cells.append(at)
        }
        return cells
    }

    // MARK: - ⇥ and ⇧⇥

    @Test("⇥ walks a line left to right without WALLET while the column is hidden, then DATA of the next row")
    func tabOrderWithoutWallet() {
        let cursor = Self.cursor()
        #expect(cursor.fields == [.date, .flow, .category, .note, .person, .amount])
        let cells = Self.walk(cursor, .tab, from: Self.cell("a", .date), times: 7)
        #expect(cells == [
            Self.cell("a", .flow),
            Self.cell("a", .category),
            Self.cell("a", .note),
            Self.cell("a", .person),
            Self.cell("a", .amount),
            Self.cell("b", .date),
            Self.cell("b", .flow),
        ])
    }

    @Test("⇥ goes through WALLET between DESCRIZIONE and PERSONA while the column is shown")
    func tabOrderWithWallet() {
        let cursor = Self.cursor(showsWallet: true)
        #expect(cursor.fields == [.date, .flow, .category, .note, .wallet, .person, .amount])
        let cells = Self.walk(cursor, .tab, from: Self.cell("b", .note), times: 4)
        #expect(cells == [
            Self.cell("b", .wallet),
            Self.cell("b", .person),
            Self.cell("b", .amount),
            Self.cell("c", .date),
        ])
    }

    @Test("⇧⇥ walks back, from DATA to IMPORTO of the row above, and stops on the first row's DATA")
    func backTabOrder() {
        let cursor = Self.cursor(showsWallet: true)
        let cells = Self.walk(cursor, .backTab, from: Self.cell("b", .flow), times: 3)
        #expect(cells == [Self.cell("b", .date), Self.cell("a", .amount), Self.cell("a", .person)])
        #expect(cursor.target(of: .backTab, from: Self.cell("a", .date)) == Self.cell("a", .date))
        // From the new line, back onto the last stored row.
        #expect(cursor.target(of: .backTab, from: Self.cell(nil, .date)) == Self.cell("c", .amount))
    }

    @Test("⇥ off the last stored row's IMPORTO lands on the new line's DATA, and off the new line's on its own DATA")
    func tabIntoAndAroundTheNewLine() {
        let cursor = Self.cursor()
        #expect(cursor.target(of: .tab, from: Self.cell("c", .amount)) == Self.cell(nil, .date))
        #expect(cursor.target(of: .tab, from: Self.cell(nil, .amount)) == Self.cell(nil, .date))
        #expect(cursor.target(of: .tab, from: Self.cell(nil, .note)) == Self.cell(nil, .person))
    }

    @Test("A vault only read has no new line: the last row's IMPORTO is the end of ⇥ and ↓")
    func readOnlyHasNoNewLine() {
        let cursor = Self.cursor(canWrite: false)
        #expect(cursor.target(of: .tab, from: Self.cell("c", .amount)) == Self.cell("c", .amount))
        #expect(cursor.target(of: .down, from: Self.cell("c", .note)) == Self.cell("c", .note))
        // Selecting still moves: only typing is refused.
        #expect(cursor.target(of: .up, from: Self.cell("c", .note)) == Self.cell("b", .note))
    }

    // MARK: - The arrows

    @Test("↑ and ↓ keep the column, from the first row down to the new line, which ↓ does not leave")
    func upAndDown() {
        let cursor = Self.cursor()
        let down = Self.walk(cursor, .down, from: Self.cell("a", .category), times: 4)
        #expect(down == [
            Self.cell("b", .category),
            Self.cell("c", .category),
            Self.cell(nil, .category),
            Self.cell(nil, .category),
        ])
        let up = Self.walk(cursor, .up, from: Self.cell(nil, .amount), times: 4)
        #expect(up == [
            Self.cell("c", .amount),
            Self.cell("b", .amount),
            Self.cell("a", .amount),
            Self.cell("a", .amount),
        ])
    }

    @Test("With no row in the month the new line is the only line: ↑, ↓ and ⇧⇥ off its DATA stay")
    func emptyMonth() {
        let cursor = LedgerCursor(rows: [], canWrite: true, showsWallet: false)
        let start = Self.cell(nil, .date)
        #expect(cursor.target(of: .up, from: start) == start)
        #expect(cursor.target(of: .down, from: start) == start)
        #expect(cursor.target(of: .backTab, from: start) == start)
        #expect(cursor.target(of: .tab, from: start) == Self.cell(nil, .flow))
    }

    @Test("← and → move one column and stop at the edges, passing over WALLET while it is hidden")
    func leftAndRight() {
        let hidden = Self.cursor()
        #expect(hidden.target(of: .left, from: Self.cell("b", .date)) == Self.cell("b", .date))
        #expect(hidden.target(of: .right, from: Self.cell("b", .amount)) == Self.cell("b", .amount))
        #expect(hidden.target(of: .right, from: Self.cell("b", .note)) == Self.cell("b", .person))
        #expect(hidden.target(of: .left, from: Self.cell(nil, .flow)) == Self.cell(nil, .date))

        let shown = Self.cursor(showsWallet: true)
        #expect(shown.target(of: .right, from: Self.cell("b", .note)) == Self.cell("b", .wallet))
        #expect(shown.target(of: .left, from: Self.cell("b", .person)) == Self.cell("b", .wallet))
    }

    @Test("A cell of a row no longer on screen goes nowhere")
    func vanishedRowStays() {
        let cursor = Self.cursor()
        let gone = Self.cell("z", .note)
        for move in [CursorMove.up, .down, .left, .right, .tab, .backTab] {
            #expect(cursor.target(of: move, from: gone) == gone)
        }
    }

    // MARK: - What can be typed into

    @Test("Every cell of a live entry and of the new line can be typed into, WALLET only while shown")
    func entriesAreEditable() {
        let hidden = Self.cursor()
        for field in hidden.fields {
            #expect(hidden.isEditable(Self.cell("a", field)))
            #expect(hidden.isEditable(Self.cell(nil, field)))
        }
        #expect(!hidden.isEditable(Self.cell("a", .wallet)))
        #expect(!hidden.isEditable(Self.cell(nil, .wallet)))
        let shown = Self.cursor(showsWallet: true)
        #expect(shown.isEditable(Self.cell("a", .wallet)))
        #expect(shown.isEditable(Self.cell(nil, .wallet)))
    }

    @Test("A transfer's DATA, CATEGORIA, DESCRIZIONE and IMPORTO can be typed into, its two ends and its author not")
    func transferCells() {
        for kind in [TransactionKind.transferWallet, .transferFlow] {
            let cursor = LedgerCursor(rows: [Self.row("t", kind: kind)], canWrite: true, showsWallet: true)
            let editable = cursor.fields.filter { cursor.isEditable(Self.cell("t", $0)) }
            #expect(editable == [.date, .category, .note, .amount])
        }
    }

    @Test("Nothing can be typed into a deleted row, a vault only read, nor a row no longer on screen")
    func lockedCells() {
        let voided = LedgerCursor(rows: [Self.row("v", voided: true)], canWrite: true, showsWallet: true)
        #expect(voided.fields.allSatisfy { !voided.isEditable(Self.cell("v", $0)) })
        // The new line beside it still can.
        #expect(voided.isEditable(Self.cell(nil, .amount)))

        let readOnly = Self.cursor(canWrite: false, showsWallet: true)
        #expect(readOnly.fields.allSatisfy { !readOnly.isEditable(Self.cell("a", $0)) })
        #expect(readOnly.fields.allSatisfy { !readOnly.isEditable(Self.cell(nil, $0)) })

        #expect(!Self.cursor().isEditable(Self.cell("z", .note)))
    }
}
