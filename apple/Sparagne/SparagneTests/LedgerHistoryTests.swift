import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Edit ▸ Undo and Redo over the ledger's writes (`LedgerHistory`), driven the
/// way the menu drives them: `undo()` and `redo()` on an `UndoManager`, then
/// the store once the queued writes have landed.
@MainActor
struct LedgerHistoryTests {
    /// Vault `Casa` with a wallet, and an undo manager attached after the
    /// setup, so the stack starts empty.
    private static func ledger(
        undoWindow: Duration = .seconds(60),
        sleeper: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) }
    ) async throws -> (store: AppStore, manager: UndoManager) {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.history.\(UUID().uuidString)"))
        let store = AppStore(
            core: try CoreActor.inMemory(author: "matteo"),
            defaults: defaults,
            undoWindow: undoWindow,
            sleeper: sleeper
        )
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        return (store, try Self.attachManager(to: store))
    }

    /// A manager as the window's behaves between two events. Grouping by
    /// event closes a group when the run loop turns, which a test awaiting on
    /// the main actor never waits for, so every step would pile into one
    /// group; the history opens a group of its own per step, which is what
    /// this leaves to count.
    static func attachManager(to store: AppStore) throws -> UndoManager {
        let manager = UndoManager()
        manager.groupsByEvent = false
        store.attach(undoManager: manager)
        #expect(!manager.canUndo)
        return manager
    }

    private static func row(_ note: String, in store: AppStore) throws -> TransactionRow {
        try #require(store.rows.first { $0.note == note })
    }

    private static func view(_ id: Uuid, in store: AppStore) throws -> TransactionView {
        try #require(store.transactions.first { $0.id == id })
    }

    @Test("An edit comes back with the stored values: Uncategorized as a blank name, a cleared note, the same instant")
    func undoRedoAnEdit() async throws {
        let (store, manager) = try await Self.ledger()
        // Uncategorized, and a note: the two values a label could get wrong.
        await store.addRow(day: Date(), flowId: nil, category: nil, note: "pizza", amount: 1_250)
        let before = try Self.row("pizza", in: store)
        let stored = try Self.view(before.id, in: store)
        #expect(before.category == String(localized: "Uncategorized"))

        // Another day of the same month, so the row stays on screen.
        let calendar = Calendar.current
        let step = calendar.component(.day, from: before.occurredAt) > 1 ? -1 : 1
        var patch = TransactionPatch()
        patch.category = "Cibo"
        patch.note = ""
        patch.amount = 2_000
        patch.occurredAt = AppStore.stamp(
            day: try #require(calendar.date(byAdding: .day, value: step, to: before.occurredAt)),
            likeTimeOf: before.occurredAt
        )
        await store.update(transactionId: before.id, patch: patch)
        #expect(store.presentedError == nil)
        #expect(manager.canUndo)
        #expect(manager.undoActionName == String(localized: "Edit Row"))
        let edited = try #require(store.rows.first { $0.id == before.id })
        #expect(edited.category == "Cibo")
        #expect(edited.note.isEmpty)

        manager.undo()
        await store.settle()
        #expect(store.presentedError == nil)
        let undone = try Self.view(before.id, in: store)
        #expect(undone.categoryId == stored.categoryId)
        #expect(undone.categoryIsSystem)
        #expect(undone.note == "pizza")
        #expect(undone.amount == 1_250)
        #expect(undone.occurredAt == stored.occurredAt)
        // The localized label was never sent back as a name to create.
        #expect(!store.categories.contains { !$0.isSystem && $0.name == String(localized: "Uncategorized") })
        #expect(manager.canRedo)

        manager.redo()
        await store.settle()
        let redone = try Self.view(before.id, in: store)
        #expect(redone.category == "Cibo")
        #expect(redone.note == nil)
        #expect(redone.amount == 2_000)
        #expect(manager.canUndo)
    }

    @Test("A note cleared by an edit is cleared again by undo when it was empty before")
    func undoRestoresAnEmptyNote() async throws {
        let (store, manager) = try await Self.ledger()
        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "", amount: 500)
        let row = try #require(store.rows.first)

        var patch = TransactionPatch()
        patch.note = "coop"
        await store.update(transactionId: row.id, patch: patch)
        #expect(try Self.view(row.id, in: store).note == "coop")

        manager.undo()
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(try Self.view(row.id, in: store).note == nil)
    }

    @Test("Undo voids an added row at once, and redo adds it back under a new id")
    func undoRedoAnAddedRow() async throws {
        let (store, manager) = try await Self.ledger()
        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "coop", amount: 5_000)
        let added = try Self.row("coop", in: store)
        #expect(manager.undoActionName == String(localized: "Add Row"))

        manager.undo()
        await store.settle()
        #expect(store.presentedError == nil)
        // Straight to the core: no toast to wait out.
        #expect(store.pendingUndo == nil)
        #expect(!store.rows.contains { $0.note == "coop" })
        #expect(store.wallets.first?.balance == 100_000)

        manager.redo()
        await store.settle()
        #expect(store.presentedError == nil)
        let readded = try Self.row("coop", in: store)
        #expect(readded.id != added.id)
        #expect(readded.absoluteAmount == 5_000)
        #expect(readded.occurredAt == added.occurredAt)

        // And undo follows it to its new id.
        manager.undo()
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(!store.rows.contains { $0.note == "coop" })
        #expect(store.wallets.first?.balance == 100_000)
    }

    @Test("⌘Z while the toast is up cancels the pending void, and the core never hears of it")
    func undoCancelsThePendingVoid() async throws {
        let (store, manager) = try await Self.ledger()
        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "coop", amount: 5_000)
        let row = try Self.row("coop", in: store)

        await store.void(transactionId: row.id)
        #expect(store.pendingUndo != nil)
        #expect(manager.undoActionName == String(localized: "Delete Transactions"))

        manager.undo()
        await store.settle()
        #expect(store.pendingUndo == nil)
        #expect(store.rows.contains { $0.id == row.id })
        #expect(store.wallets.first?.balance == 95_000)
        // The add is next, untouched by the cancelled void.
        #expect(manager.undoActionName == String(localized: "Add Row"))
    }

    @Test("A void that reached the core leaves nothing to undo, not even the row's own earlier steps")
    func flushedVoidIsNotUndoable() async throws {
        let (store, manager) = try await Self.ledger()
        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "coop", amount: 5_000)
        let row = try Self.row("coop", in: store)
        var patch = TransactionPatch()
        patch.amount = 6_000
        await store.update(transactionId: row.id, patch: patch)

        await store.void(transactionId: row.id)
        await store.flushPendingUndo()

        #expect(store.presentedError == nil)
        #expect(!manager.canUndo)
        #expect(!manager.canRedo)
    }

    @Test("The toast's own Undo button leaves Edit ▸ Undo with nothing to cancel")
    func toastButtonForgetsTheStep() async throws {
        let (store, manager) = try await Self.ledger()
        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "coop", amount: 5_000)
        let row = try Self.row("coop", in: store)

        await store.void(transactionId: row.id)
        store.undo()
        #expect(manager.undoActionName == String(localized: "Add Row"))
    }

    @Test("Switching vault clears the stack")
    func switchingVaultClears() async throws {
        let (store, manager) = try await Self.ledger()
        let casa = try #require(store.currentVault)
        await store.createVault(name: "Mare", walletName: "Cassa", openingBalance: 0)
        await store.select(casa)
        #expect(!manager.canUndo)

        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "coop", amount: 5_000)
        #expect(manager.canUndo)
        let mare = try #require(store.vaults.first { $0.name == "Mare" })
        await store.select(mare)
        #expect(!manager.canUndo)
        #expect(!manager.canRedo)
    }

    @Test("A bulk re-categorize is one step: one ⌘Z puts every row back")
    func bulkRecategorizeIsOneStep() async throws {
        let (store, manager) = try await Self.ledger()
        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "pane", amount: 100)
        await store.addRow(day: Date(), flowId: nil, category: nil, note: "latte", amount: 200)
        store.selectAllRows()
        #expect(store.bulkTargets.count == 2)

        await store.setSelectionCategory("Colazione")
        #expect(store.presentedError == nil)
        #expect(store.rows.allSatisfy { $0.category == "Colazione" })
        #expect(manager.undoActionName == String(localized: "Category Change"))

        manager.undo()
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(try Self.row("pane", in: store).category == "Spesa")
        #expect(try Self.row("latte", in: store).category == String(localized: "Uncategorized"))
        // The next step down is the second add, not half of the bulk change.
        #expect(manager.undoActionName == String(localized: "Add Row"))

        manager.redo()
        await store.settle()
        #expect(store.rows.allSatisfy { $0.category == "Colazione" })
    }
}
