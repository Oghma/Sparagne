import AppKit
import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Holds the next call to reach the core until it is released, so a test can
/// keep one of the editor's writes on its way while the window moves on, and
/// counts every call that reaches it.
private actor CoreGate {
    private(set) var calls = 0
    private var armed = false
    private var held: CheckedContinuation<Void, Never>?
    private var watchers: [CheckedContinuation<Void, Never>] = []

    func arm() { armed = true }

    /// The core's probe: every call goes through, but the first one after
    /// `arm` waits here.
    func pass() async {
        calls += 1
        guard armed else { return }
        armed = false
        await withCheckedContinuation { continuation in
            held = continuation
            for watcher in watchers { watcher.resume() }
            watchers = []
        }
    }

    /// Returns once a call is being held.
    func holding() async {
        guard held == nil else { return }
        await withCheckedContinuation { watchers.append($0) }
    }

    func release() {
        held?.resume()
        held = nil
    }
}

/// The Mastro edited cell by cell (`LedgerEditor`), driven the way the grid
/// drives it, a key or a click at a time, over a real in-memory core: a cell
/// is written the moment it is left, alone, and the new line when the cursor
/// leaves it with an amount.
struct LedgerEditorTests {
    /// Vault `Casa` with wallet `Conto` and envelope `Cash` beside
    /// Unallocated, and two expenses of today on Cash: `mutuo` 950.00, then
    /// `coop` 50.00, filed under Spesa. The Mastro is the tab on screen.
    private static func household(
        probe: (@Sendable () async -> Void)? = nil
    ) async throws -> (store: AppStore, editor: LedgerEditor) {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.editor.\(UUID().uuidString)"))
        let store = AppStore(core: try CoreActor.inMemory(author: "matteo", probe: probe), defaults: defaults)
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        await store.createEnvelope(name: "Cash", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        let envelope = try #require(store.flows.first { $0.name == "Cash" })
        await store.addRow(day: Date(), flowId: envelope.id, category: "Casa", note: "mutuo", amount: 95_000)
        await store.addRow(day: Date(), flowId: envelope.id, category: "Spesa", note: "coop", amount: 5_000)
        store.tab = .ledger
        #expect(store.presentedError == nil)
        #expect(store.rows.map(\.note) == ["mutuo", "coop"])
        return (store, store.ledgerEditor)
    }

    private static func row(_ note: String, in store: AppStore) throws -> TransactionRow {
        try #require(store.rows.first { $0.note == note })
    }

    private static func cell(_ row: TransactionRow?, _ field: RowField) -> CellFocus {
        CellFocus(row: row?.id, field: field)
    }

    private static func newLine(_ field: RowField) -> CellFocus {
        CellFocus(row: nil, field: field)
    }

    // MARK: - One cell, written alone

    @Test("A cell saved sends its field alone: a note changed meanwhile is not written back over")
    func cellSavedAlone() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        editor.select(Self.cell(coop, .amount))
        #expect(editor.type("60"))

        // Another cell of the row changes while the amount is being typed,
        // as a sync would change it.
        var patch = TransactionPatch()
        patch.note = "conad"
        await store.update(transactionId: coop.id, patch: patch)
        #expect(editor.isEditing)
        #expect(editor.text == "60")

        #expect(editor.move(.up))
        await store.settle()
        #expect(store.presentedError == nil)
        let saved = try #require(store.rows.first { $0.id == coop.id })
        #expect(saved.absoluteAmount == 6_000)
        #expect(saved.note == "conad")
    }

    @Test("A cell is saved whichever way it is left: a click on another, an arrow, ⇥, the grid left")
    func everyWayOutSaves() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        let mutuo = try Self.row("mutuo", in: store)

        editor.select(Self.cell(coop, .note))
        editor.type("conad")
        #expect(editor.select(Self.cell(mutuo, .category)))
        #expect(editor.cursor == Self.cell(mutuo, .category))
        #expect(!editor.isEditing)

        editor.type("Mutuo")
        #expect(editor.move(.right))
        #expect(editor.cursor == Self.cell(mutuo, .note))

        editor.type("rata")
        #expect(editor.move(.tab))
        #expect(editor.cursor == Self.cell(mutuo, .person))

        editor.select(Self.cell(coop, .amount))
        editor.type("12.30")
        #expect(editor.finish())
        #expect(editor.cursor == nil)

        await store.settle()
        #expect(store.presentedError == nil)
        #expect(try Self.row("conad", in: store).absoluteAmount == 1_230)
        let rata = try Self.row("rata", in: store)
        #expect(rata.id == mutuo.id)
        #expect(rata.category == "Mutuo")
    }

    @Test("Typing on a selected cell replaces what it says, then appends; ↩ and F2 start from what it says")
    func typeReplacesThenAppends() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)

        editor.select(Self.cell(coop, .category))
        #expect(editor.edit())
        #expect(editor.isEditing)
        #expect(editor.text == "Spesa")
        editor.cancel()

        editor.select(Self.cell(coop, .note))
        #expect(!editor.isEditing)
        #expect(editor.type("c"))
        #expect(editor.text == "c")
        #expect(editor.type("onad"))
        #expect(editor.text == "conad")
        // A click on the cell being edited leaves it being edited.
        #expect(editor.select(Self.cell(coop, .note)))
        #expect(editor.isEditing)
        editor.confirm()
        #expect(editor.cursor == Self.cell(coop, .note))
        #expect(!editor.isEditing)

        // ⌫ on a selected cell empties it.
        editor.select(Self.cell(coop, .amount))
        #expect(editor.type(""))
        #expect(editor.isEditing)
        #expect(editor.text.isEmpty)
        // An emptied amount leaves the amount alone.
        editor.confirm()

        await store.settle()
        let saved = try #require(store.rows.first { $0.id == coop.id })
        #expect(saved.note == "conad")
        #expect(saved.absoluteAmount == 5_000)
    }

    @Test("DATA starts from the bare day; a day typed moves the row inside the month, text that means no date does not")
    func dateCell() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        let calendar = Calendar.current
        let today = calendar.component(.day, from: coop.occurredAt)
        let other = today == 1 ? 2 : 1

        editor.select(Self.cell(coop, .date))
        #expect(editor.edit())
        #expect(editor.text == "\(today)")
        editor.text = "x"
        #expect(editor.move(.right))
        await store.settle()
        #expect(store.rows.first { $0.id == coop.id }?.occurredAt == coop.occurredAt)

        editor.select(Self.cell(coop, .date))
        editor.type("\(other)")
        #expect(editor.finish())
        await store.settle()
        let moved = try #require(store.rows.first { $0.id == coop.id })
        #expect(calendar.component(.day, from: moved.occurredAt) == other)
        #expect(MonthKey(moved.occurredAt) == store.month)
        // Same time of day, so the rows of a day keep their order.
        let time: Set<Calendar.Component> = [.hour, .minute, .second]
        #expect(calendar.dateComponents(time, from: moved.occurredAt) == calendar.dateComponents(time, from: coop.occurredAt))
    }

    @Test("ENVELOPE's text only narrows its list: the pick is what is saved")
    func envelopeCell() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        let unallocated = try #require(store.flows.first { $0.isUnallocated })

        editor.select(Self.cell(coop, .flow))
        #expect(editor.edit())
        #expect(editor.text.isEmpty)
        #expect(editor.flowId == coop.flowId)
        editor.text = "unal"
        editor.flowId = unallocated.id
        #expect(editor.move(.down))
        await store.settle()
        #expect(store.rows.first { $0.id == coop.id }?.flowId == unallocated.id)
    }

    // MARK: - esc

    @Test("esc while editing takes back that cell only, still selected; again, the cursor goes")
    func escRevertsTheCell() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)

        editor.select(Self.cell(coop, .note))
        editor.type("conad")
        editor.move(.left)
        editor.type("Altro")
        editor.cancel()
        #expect(editor.cursor == Self.cell(coop, .category))
        #expect(!editor.isEditing)
        #expect(editor.text.isEmpty)

        editor.cancel()
        #expect(editor.cursor == nil)

        await store.settle()
        let saved = try #require(store.rows.first { $0.id == coop.id })
        #expect(saved.note == "conad")
        #expect(saved.category == "Spesa")
    }

    @Test("esc on a selected cell of the new line empties the line and lets the cursor go")
    func escEmptiesTheNewLine() async throws {
        let (store, editor) = try await Self.household()
        editor.select(Self.newLine(.note))
        editor.type("pane")
        editor.move(.right)
        #expect(editor.newLine.note == "pane")

        editor.cancel()
        #expect(editor.cursor == nil)
        #expect(editor.newLine.note.isEmpty)
        await store.settle()
        #expect(store.rows.count == 2)
    }

    // MARK: - A cell that refuses

    @Test("A refused amount keeps the cell being edited as typed, and the cursor does not move")
    func refusedAmountStays() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        editor.select(Self.cell(coop, .amount))
        editor.type("abc")

        #expect(!editor.move(.up))
        #expect(store.presentedError != nil)
        #expect(editor.cursor == Self.cell(coop, .amount))
        #expect(editor.isEditing)
        #expect(editor.text == "abc")
        #expect(!editor.select(Self.cell(coop, .note)))
        #expect(!editor.finish())
        #expect(editor.cursor == Self.cell(coop, .amount))

        store.presentedError = nil
        editor.text = "7"
        #expect(editor.move(.up))
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(store.rows.first { $0.id == coop.id }?.absoluteAmount == 700)
    }

    @Test("Nothing can be typed into a vault only read or a deleted row, though their cells can be selected")
    func lockedCellsBeep() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        let vault = try #require(store.currentVault)

        // Deleted, and shown struck through.
        await store.void(transactionId: coop.id)
        await store.flushPendingUndo()
        store.showVoided = true
        await store.settle()
        let voided = try #require(store.rows.first { $0.id == coop.id })
        #expect(voided.voided)
        #expect(editor.select(Self.cell(voided, .note)))
        #expect(!editor.type("x"))
        #expect(!editor.edit())
        #expect(!editor.isEditing)

        // The role changes under a cell being typed into: it closes unsaved.
        let mutuo = try Self.row("mutuo", in: store)
        editor.select(Self.cell(mutuo, .note))
        editor.type("rata")
        store.setReadOnlyVaults([vault.id])
        #expect(editor.cursor == nil)
        #expect(editor.select(Self.cell(mutuo, .note)))
        #expect(!editor.type("x"))
        #expect(!editor.edit())
        #expect(editor.cursor == Self.cell(mutuo, .note))
        await store.settle()
        #expect(store.rows.first { $0.id == mutuo.id }?.note == "mutuo")
    }

    // MARK: - The new line

    @Test("The new line is registered when ↑ leaves it with an amount, and starts over blank on the envelope just used")
    func newLineRegistersOnUp() async throws {
        let (store, editor) = try await Self.household()
        let unallocated = try #require(store.flows.first { $0.isUnallocated })
        editor.select(Self.newLine(.flow))
        editor.edit()
        editor.flowId = unallocated.id
        editor.move(.tab)
        editor.type("Pane")
        editor.move(.tab)
        editor.type("forno")
        #expect(editor.move(.tab))
        #expect(editor.cursor == Self.newLine(.person))
        editor.move(.right)
        editor.type("3.20")

        let last = try Self.row("coop", in: store)
        #expect(editor.move(.up))
        #expect(editor.cursor == Self.cell(last, .amount))
        #expect(editor.newLine.note.isEmpty)
        #expect(editor.newLine.amount.isEmpty)
        #expect(editor.newLine.flowId == unallocated.id)

        await store.settle()
        #expect(store.presentedError == nil)
        let added = try Self.row("forno", in: store)
        #expect(added.absoluteAmount == 320)
        #expect(added.category == "Pane")
        #expect(added.flowId == unallocated.id)
        #expect(added.kind == .expense)
        #expect(added.person == "matteo")
    }

    @Test("A click on a stored row registers the new line, as ⇥ off its IMPORTO does, landing on its DATA again")
    func newLineRegistersOnClickAndTab() async throws {
        let (store, editor) = try await Self.household()
        let mutuo = try Self.row("mutuo", in: store)

        editor.select(Self.newLine(.note))
        editor.type("pane")
        editor.select(Self.newLine(.amount))
        editor.type("2")
        #expect(editor.select(Self.cell(mutuo, .category)))
        #expect(editor.newLine.note.isEmpty)

        editor.select(Self.newLine(.note))
        editor.type("latte")
        editor.select(Self.newLine(.amount))
        editor.type("1.5")
        #expect(editor.move(.tab))
        #expect(editor.cursor == Self.newLine(.date))
        #expect(!editor.isEditing)
        #expect(editor.newLine.note.isEmpty)

        await store.settle()
        #expect(store.presentedError == nil)
        #expect(try Self.row("pane", in: store).absoluteAmount == 200)
        #expect(try Self.row("latte", in: store).absoluteAmount == 150)
    }

    @Test("Leaving the grid registers the new line with an amount; ↓ on it, or leaving it without one, keeps the draft")
    func newLineStaysADraftWithoutAnAmount() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)

        // ↓ on the new line goes nowhere: the cell is saved into the line.
        editor.select(Self.newLine(.amount))
        editor.type("4")
        #expect(editor.move(.down))
        #expect(editor.cursor == Self.newLine(.amount))
        #expect(!editor.isEditing)
        #expect(editor.newLine.amount == "4")
        await store.settle()
        #expect(store.rows.count == 2)

        // Without an amount the line is left as typed.
        editor.type("")
        editor.move(.left)
        editor.type("pane")
        #expect(editor.move(.up))
        #expect(editor.cursor == Self.cell(coop, .person))
        #expect(editor.newLine.person == "pane")
        await store.settle()
        #expect(store.rows.count == 2)

        // With one, leaving the grid registers it.
        editor.select(Self.newLine(.person))
        editor.type("")
        editor.select(Self.newLine(.note))
        editor.type("pane")
        editor.select(Self.newLine(.amount))
        editor.type("2")
        #expect(editor.finish())
        #expect(editor.cursor == nil)
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(try Self.row("pane", in: store).absoluteAmount == 200)
    }

    @Test("↩ on the new line with an amount registers it and goes to the blank line's DATA; without one it stays")
    func confirmOnTheNewLine() async throws {
        let (store, editor) = try await Self.household()
        editor.select(Self.newLine(.note))
        editor.type("pane")
        editor.confirm()
        #expect(editor.cursor == Self.newLine(.note))
        #expect(!editor.isEditing)
        #expect(editor.newLine.note == "pane")

        editor.select(Self.newLine(.amount))
        editor.type("2")
        editor.confirm()
        #expect(editor.cursor == Self.newLine(.date))
        #expect(!editor.isEditing)
        #expect(editor.newLine.note.isEmpty)
        await store.settle()
        #expect(try Self.row("pane", in: store).absoluteAmount == 200)
    }

    @Test("A new line that refuses to register puts the caret on the cell that is wrong, and the cursor stays on the line")
    func newLineRefused() async throws {
        let (store, editor) = try await Self.household()
        store.showWalletColumn = true
        let coop = try Self.row("coop", in: store)
        #expect(editor.newLine.wallet == "Conto")

        editor.select(Self.newLine(.wallet))
        editor.type("Nessuno")
        editor.select(Self.newLine(.amount))
        editor.type("2")
        #expect(!editor.move(.up))
        #expect(store.presentedError != nil)
        #expect(editor.cursor == Self.newLine(.wallet))
        #expect(editor.isEditing)
        #expect(editor.text == "Nessuno")

        store.presentedError = nil
        editor.text = "Conto"
        #expect(editor.select(Self.cell(coop, .note)))
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(store.rows.count == 3)
    }

    @Test("The new line's DATA reads a bare day inside the month on screen")
    func newLineDate() async throws {
        let (store, editor) = try await Self.household()
        editor.select(Self.newLine(.date))
        editor.type("3")
        editor.move(.right)
        let day = editor.newLine.day
        #expect(Calendar.current.component(.day, from: day) == 3)
        #expect(MonthKey(day) == store.month)
    }

    @Test("⌘D fills the new line with the row, the caret in its IMPORTO")
    func duplicateFillsTheNewLine() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        let mutuo = try Self.row("mutuo", in: store)
        editor.select(Self.cell(mutuo, .note))
        editor.type("rata")

        editor.duplicate(coop)
        #expect(editor.cursor == Self.newLine(.amount))
        #expect(editor.isEditing)
        #expect(editor.text == "50.00")
        #expect(editor.newLine.note == "coop")
        #expect(editor.newLine.category == "Spesa")
        #expect(editor.newLine.flowId == coop.flowId)

        editor.text = "60.00"
        editor.confirm()
        await store.settle()
        #expect(store.presentedError == nil)
        // The cell being typed into before ⌘D was saved on the way.
        #expect(store.rows.first { $0.id == mutuo.id }?.note == "rata")
        #expect(store.rows.filter { $0.note == "coop" }.map(\.absoluteAmount) == [5_000, 6_000])
    }

    @Test("A copy left as ⌘D made it is not registered by leaving it, nor by a change of month")
    func untouchedCopyIsNotRegistered() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        let mutuo = try Self.row("mutuo", in: store)

        editor.duplicate(coop)
        // The amount saved as it came is no change.
        #expect(editor.move(.left))
        #expect(editor.select(Self.cell(mutuo, .note)))
        #expect(editor.newLine.note == "coop")
        #expect(editor.select(Self.newLine(.note)))
        #expect(editor.finish())
        await store.settle()
        #expect(store.rows.count == 2)
        #expect(editor.newLine.amount == "50.00")

        editor.duplicate(coop)
        store.stepMonth(by: -1)
        await store.settle()
        store.stepMonth(by: 1)
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(store.rows.count == 2)
        #expect(editor.newLine.note.isEmpty)
    }

    @Test("A copy with a cell changed is registered by leaving it; ↩ registers one unchanged; esc twice throws it away")
    func changedCopyIsRegistered() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        let mutuo = try Self.row("mutuo", in: store)

        editor.duplicate(coop)
        editor.text = "60.00"
        #expect(editor.move(.left))
        #expect(editor.select(Self.cell(mutuo, .note)))
        #expect(editor.newLine.note.isEmpty)
        await store.settle()
        #expect(store.rows.filter { $0.note == "coop" }.map(\.absoluteAmount) == [5_000, 6_000])

        editor.duplicate(mutuo)
        editor.confirm()
        #expect(editor.cursor == Self.newLine(.date))
        await store.settle()
        #expect(store.rows.filter { $0.note == "mutuo" }.count == 2)

        editor.duplicate(coop)
        editor.cancel()
        #expect(editor.cursor == Self.newLine(.amount))
        editor.cancel()
        #expect(editor.cursor == nil)
        #expect(editor.newLine.note.isEmpty)
        #expect(editor.newLine.amount.isEmpty)
        await store.settle()
        #expect(store.rows.count == 4)
    }

    // MARK: - The window moving under a cell

    @Test("A month changed with a cell open writes it to its row, as one undoable step")
    func monthChangeSavesTheCell() async throws {
        let (store, editor) = try await Self.household()
        let manager = try LedgerHistoryTests.attachManager(to: store)
        let coop = try Self.row("coop", in: store)
        let current = store.month

        editor.select(Self.cell(coop, .amount))
        editor.type("75")
        store.month = current.adding(months: -1)
        #expect(editor.cursor == nil)
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(store.rows.isEmpty)
        #expect(manager.canUndo)
        #expect(manager.undoActionName == String(localized: "Edit Row"))
        // The new line follows the month on screen.
        #expect(MonthKey(editor.newLine.day) == store.month)

        store.month = current
        await store.settle()
        #expect(store.rows.first { $0.id == coop.id }?.absoluteAmount == 7_500)

        manager.undo()
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(store.rows.first { $0.id == coop.id }?.absoluteAmount == 5_000)
    }

    @Test("A cell saved on the way to another month behind a write still on its way keeps its own undo step")
    func queuedCellKeepsItsUndoStep() async throws {
        let (store, editor) = try await Self.household()
        let manager = try LedgerHistoryTests.attachManager(to: store)
        let coop = try Self.row("coop", in: store)
        let current = store.month

        editor.select(Self.cell(coop, .note))
        editor.type("conad")
        editor.move(.right)
        editor.select(Self.cell(coop, .amount))
        editor.type("75")
        // The amount's write waits for the note's, and the next month's rows
        // are loaded by the time it gets its turn.
        store.month = current.adding(months: 1)
        await store.settle()
        store.month = current
        await store.settle()
        var saved = try #require(store.rows.first { $0.id == coop.id })
        #expect(saved.note == "conad")
        #expect(saved.absoluteAmount == 7_500)

        manager.undo()
        await store.settle()
        saved = try #require(store.rows.first { $0.id == coop.id })
        #expect(saved.note == "conad")
        #expect(saved.absoluteAmount == 5_000)

        manager.undo()
        await store.settle()
        #expect(store.rows.first { $0.id == coop.id }?.note == "coop")
        #expect(store.presentedError == nil)
    }

    @Test("USCITE to ENTRATE saves the cell, and registers the new line as the expense it was typed as")
    func directionChangeSavesTheCell() async throws {
        let (store, editor) = try await Self.household()
        let manager = try LedgerHistoryTests.attachManager(to: store)
        let coop = try Self.row("coop", in: store)

        editor.select(Self.cell(coop, .note))
        editor.type("conad")
        store.direction = .income
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(manager.undoActionName == String(localized: "Edit Row"))

        store.direction = .expenses
        await store.settle()
        #expect(store.rows.first { $0.id == coop.id }?.note == "conad")

        editor.select(Self.newLine(.note))
        editor.type("pane")
        editor.select(Self.newLine(.amount))
        editor.type("2")
        store.direction = .income
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(!store.rows.contains { $0.note == "pane" })
        #expect(editor.newLine.note.isEmpty)

        store.direction = .expenses
        await store.settle()
        #expect(try Self.row("pane", in: store).kind == .expense)
    }

    @Test("The PERSONA filter saves the cell; showing the transfers saves it too and keeps the cursor on its row")
    func filtersSaveTheCell() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)

        editor.select(Self.cell(coop, .note))
        editor.type("conad")
        store.person = "matteo"
        #expect(editor.cursor == nil)
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(store.rows.first { $0.id == coop.id }?.note == "conad")

        editor.select(Self.cell(coop, .category))
        editor.type("Altro")
        store.showTransfers = true
        #expect(editor.cursor == Self.cell(coop, .category))
        #expect(!editor.isEditing)
        await store.settle()
        #expect(store.rows.first { $0.id == coop.id }?.category == "Altro")
        #expect(editor.cursor == Self.cell(coop, .category))
    }

    @Test("A new name for the author follows into the new line's PERSONA, unless it names someone else")
    func newLineFollowsTheAuthor() async throws {
        let (store, editor) = try await Self.household()
        #expect(editor.newLine.person == "matteo")
        await store.setAuthor("Matteo L")
        #expect(editor.newLine.person == "Matteo L")

        editor.newLine.person = "elisa"
        await store.setAuthor("matteo")
        #expect(editor.newLine.person == "elisa")
    }

    @Test("Leaving the Mastro's tab saves the cell and lets the cursor go")
    func tabSwitchSaves() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        editor.select(Self.cell(coop, .note))
        editor.type("conad")

        store.tab = .summary
        #expect(editor.cursor == nil)
        await store.settle()
        #expect(store.rows.first { $0.id == coop.id }?.note == "conad")
    }

    @Test("A cell that refuses to save outlives the tab switch, being edited as typed")
    func refusedEditSurvivesTheTabSwitch() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        editor.select(Self.cell(coop, .amount))
        editor.type("abc")

        store.tab = .recurring
        #expect(store.presentedError != nil)
        store.presentedError = nil
        store.tab = .ledger
        #expect(editor.cursor == Self.cell(coop, .amount))
        #expect(editor.isEditing)
        #expect(editor.text == "abc")
    }

    @Test("Another vault selected with a cell open writes it to the vault being left")
    func vaultSwitchSavesTheCell() async throws {
        let (store, editor) = try await Self.household()
        let casa = try #require(store.currentVault)
        let coop = try Self.row("coop", in: store)
        await store.createVault(name: "Ufficio", walletName: "Cassa", openingBalance: 0)
        let ufficio = try #require(store.currentVault)
        await store.select(casa)

        editor.select(Self.cell(coop, .note))
        editor.type("conad")
        await store.select(ufficio)
        #expect(store.presentedError == nil)
        #expect(editor.cursor == nil)
        #expect(store.rows.isEmpty)

        await store.select(casa)
        #expect(store.rows.first { $0.id == coop.id }?.note == "conad")
    }

    @Test("Quitting saves the cell being typed into before the app goes")
    func quittingSavesTheCell() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        editor.select(Self.cell(coop, .amount))
        editor.type("90")
        let delegate = AppDelegate()
        delegate.store = store

        let replied = await withCheckedContinuation { (continuation: CheckedContinuation<Bool, Never>) in
            delegate.replyToTermination = { continuation.resume(returning: $0) }
            #expect(delegate.applicationShouldTerminate(NSApplication.shared) == .terminateLater)
        }

        #expect(replied)
        #expect(store.presentedError == nil)
        #expect(store.rows.first { $0.id == coop.id }?.absoluteAmount == 9_000)
        #expect(delegate.applicationShouldTerminate(NSApplication.shared) == .terminateNow)
    }

    @Test("Quitting with a cell that refuses to be saved stays, the cell on screen being edited; after esc it quits")
    func quittingWithARefusedCellStays() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        editor.select(Self.cell(coop, .amount))
        editor.type("abc")
        store.tab = .summary
        store.presentedError = nil
        let delegate = AppDelegate()
        delegate.store = store
        delegate.replyToTermination = { _ in Issue.record("no deferred reply was asked for") }

        #expect(delegate.applicationShouldTerminate(NSApplication.shared) == .terminateCancel)
        #expect(store.presentedError != nil)
        #expect(store.tab == .ledger)
        #expect(editor.cursor == Self.cell(coop, .amount))
        #expect(editor.isEditing)
        #expect(editor.text == "abc")

        editor.cancel()
        #expect(delegate.applicationShouldTerminate(NSApplication.shared) == .terminateNow)
        #expect(store.rows.first { $0.id == coop.id }?.absoluteAmount == 5_000)
    }

    @Test("A cursor on a row that leaves the screen goes with it: a delete, a search that leaves it out")
    func vanishedRowDropsTheCursor() async throws {
        let (store, editor) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        let mutuo = try Self.row("mutuo", in: store)

        editor.select(Self.cell(coop, .note))
        editor.type("conad")
        await store.void(transactionId: coop.id)
        #expect(editor.cursor == nil)
        #expect(!editor.isEditing)
        store.undo()

        editor.select(Self.cell(mutuo, .note))
        store.searchText = "coop"
        await store.reload()
        #expect(editor.cursor == nil)

        // A row still on screen keeps it.
        editor.select(Self.cell(coop, .category))
        await store.reload()
        #expect(editor.cursor == Self.cell(coop, .category))
    }

    @Test("Hiding WALLET moves a cursor on it to PERSONA and takes the sticky wallet off the new line")
    func walletColumnToggled() async throws {
        let (store, editor) = try await Self.household()
        store.showWalletColumn = true
        #expect(editor.newLine.wallet == "Conto")
        let coop = try Self.row("coop", in: store)
        editor.select(Self.cell(coop, .wallet))
        editor.type("Con")

        store.showWalletColumn = false
        #expect(editor.cursor == Self.cell(coop, .person))
        #expect(!editor.isEditing)
        #expect(editor.newLine.wallet.isEmpty)
        await store.settle()
        #expect(store.presentedError == nil)
    }

    @Test(
        "A write queued for a vault a sync deleted is dropped, not sent to the vault on screen by then",
        .timeLimit(.minutes(1))
    )
    func writeForADeletedVaultIsDropped() async throws {
        let gate = CoreGate()
        let (store, editor) = try await Self.household(probe: { await gate.pass() })
        let casa = try #require(store.currentVault)
        let coop = try Self.row("coop", in: store)
        await store.createVault(name: "Ufficio", walletName: "Cassa", openingBalance: 0)
        let ufficio = try #require(store.currentVault)
        await store.select(casa)

        // The note's write is held on its way to the core; the new line's
        // waits behind it.
        await gate.arm()
        editor.select(Self.cell(coop, .note))
        editor.type("conad")
        editor.select(Self.newLine(.note))
        editor.type("pane")
        editor.select(Self.newLine(.amount))
        editor.type("2")
        #expect(editor.finish())
        await gate.holding()

        // Meanwhile another member deletes Casa, and the sync moves the
        // window to the vault left.
        try await store.core.execute(vaultId: casa.id, .deleteVault)
        await store.refreshAfterSync()
        #expect(store.currentVault?.id == ufficio.id)

        // The note's write, on its way already, is refused with its vault;
        // the new line's never reaches the core, which would refuse it in
        // Ufficio too, with an alert about an envelope nobody can see.
        let before = await gate.calls
        await gate.release()
        await store.settle()
        #expect(await gate.calls == before)
        #expect(store.currentVault?.id == ufficio.id)
        #expect(!store.rows.contains { $0.note == "pane" })
        #expect(!editor.hasPendingWrites)
    }

    @Test("A PERSONA filter a sync leaves without rows is cleared without ending the cell being typed into")
    func staleFilterLeavesTheCellAlone() async throws {
        let (store, editor) = try await Self.household()
        let vault = try #require(store.currentVault)
        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "verdura", amount: 2_000, person: "elisa")
        store.person = "elisa"
        await store.settle()
        let verdura = try Self.row("verdura", in: store)
        #expect(store.rows.count == 1)

        editor.select(Self.newLine(.note))
        editor.type("pane")
        editor.select(Self.newLine(.amount))
        editor.type("2")

        // elisa's last row goes, deleted on another device.
        try await store.core.execute(vaultId: vault.id, .voidTransaction(transactionId: verdura.id))
        await store.reload()
        await store.settle()
        #expect(store.person == nil)
        #expect(editor.cursor == Self.newLine(.amount))
        #expect(editor.isEditing)
        #expect(editor.text == "2")
        #expect(editor.newLine.note == "pane")
        #expect(!store.rows.contains { $0.note == "pane" })
    }

    // MARK: - Undo

    @Test("Each cell saved is one Edit ▸ Undo step, and ⌘Z takes back the last one")
    func undoTakesBackTheLastCell() async throws {
        let (store, editor) = try await Self.household()
        let manager = try LedgerHistoryTests.attachManager(to: store)
        let coop = try Self.row("coop", in: store)

        editor.select(Self.cell(coop, .note))
        editor.type("conad")
        editor.move(.left)
        editor.type("Altro")
        editor.finish()
        await store.settle()
        var saved = try #require(store.rows.first { $0.id == coop.id })
        #expect(saved.note == "conad")
        #expect(saved.category == "Altro")

        manager.undo()
        await store.settle()
        saved = try #require(store.rows.first { $0.id == coop.id })
        #expect(saved.note == "conad")
        #expect(saved.category == "Spesa")

        manager.undo()
        await store.settle()
        saved = try #require(store.rows.first { $0.id == coop.id })
        #expect(saved.note == "coop")
        #expect(store.presentedError == nil)
    }
}
