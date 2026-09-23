import Foundation
import SparagneCore
import Synchronization
import Testing

@testable import Sparagne

/// Counts `CoreActor`'s "a command was applied" hook: once per `execute`, once
/// per whole `executeBatch`.
private final class ExecutedCount: Sendable {
    private let value = Mutex(0)

    var count: Int { value.withLock { $0 } }

    func record() { value.withLock { $0 += 1 } }
}

/// The ledger's row selection and the bulk actions over it: the gestures on
/// `RowSelection`, then the store that keeps it and acts on it.
@MainActor
struct SelectionTests {
    // MARK: - The gestures

    @Test("⌘-click toggles a row, ⇧-click selects the range from the anchor and can shrink it")
    func gestures() {
        let order = ["a", "b", "c", "d", "e"]
        var selection = RowSelection()

        selection.toggle("b")
        #expect(selection.ids == ["b"])
        #expect(selection.anchor == "b")

        selection.extend(to: "d", in: order)
        #expect(selection.ids == ["b", "c", "d"])
        // A second ⇧-click from the same anchor replaces the range.
        selection.extend(to: "c", in: order)
        #expect(selection.ids == ["b", "c"])
        // Upwards works the same.
        selection.extend(to: "a", in: order)
        #expect(selection.ids == ["a", "b"])

        // ⌘-click adds a row outside the range and moves the anchor there,
        // keeping what was already selected.
        selection.toggle("e")
        #expect(selection.ids == ["a", "b", "e"])
        selection.extend(to: "d", in: order)
        #expect(selection.ids == ["a", "b", "d", "e"])

        // ⌘-click again takes a row out.
        selection.toggle("a")
        #expect(selection.ids == ["b", "d", "e"])

        selection.clear()
        #expect(selection.isEmpty)
        #expect(selection.anchor == nil)
    }

    @Test("⇧-click with no anchor selects the row alone, and a reload drops rows no longer on screen")
    func extendWithoutAnchorAndRetain() {
        var selection = RowSelection()
        selection.extend(to: "c", in: ["a", "b", "c"])
        #expect(selection.ids == ["c"])
        #expect(selection.anchor == "c")

        selection.selectAll(["a", "b", "c"])
        #expect(selection.count == 3)
        selection.retain(["a", "c"])
        #expect(selection.ids == ["a", "c"])
    }

    // MARK: - The store

    /// Vault `Casa` with four expenses this month, oldest first.
    private static func ledger(
        undoWindow: Duration = .seconds(60),
        sleeper: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) }
    ) async throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.selection.\(UUID().uuidString)"))
        let store = AppStore(
            core: try CoreActor.inMemory(author: "matteo"),
            defaults: defaults,
            undoWindow: undoWindow,
            sleeper: sleeper
        )
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        let calendar = Calendar.current
        for (day, note) in [(3, "pane"), (5, "latte"), (7, "caffè"), (9, "benzina")] {
            let date = try #require(
                calendar.date(from: DateComponents(year: store.month.year, month: store.month.month, day: day))
            )
            await store.addRow(day: date, flowId: nil, category: "Spesa", note: note, amount: 1_000)
        }
        #expect(store.presentedError == nil)
        #expect(store.rows.count == 4)
        return store
    }

    private static func id(_ note: String, in store: AppStore) throws -> Uuid {
        try #require(store.rows.first { $0.note == note }).id
    }

    @Test("The store selects by the order on screen, ⌘A takes every row, and a month change clears it")
    func storeSelection() async throws {
        let store = try await Self.ledger()
        let pane = try Self.id("pane", in: store)
        let caffe = try Self.id("caffè", in: store)

        store.toggleSelection(pane)
        store.extendSelection(to: caffe)
        #expect(store.selectedRows.map(\.note) == ["pane", "latte", "caffè"])

        store.selectAllRows()
        #expect(store.selection.count == 4)

        store.month = store.month.adding(months: -1)
        #expect(store.selection.isEmpty)
        await store.settle()

        store.month = store.month.adding(months: 1)
        await store.settle()
        store.selectAllRows()
        #expect(store.selection.count == 4)
        // Any filter, the search included, names another list.
        store.searchText = "pa"
        #expect(store.selection.isEmpty)
    }

    @Test("Voided rows and transfers can be selected, but bulk actions pass over them")
    func bulkSkipsVoidedAndTransfers() async throws {
        let store = try await Self.ledger()
        let vault = try #require(store.currentVault)
        await store.createWallet(name: "Contanti", openingBalance: 0)
        await store.submit(quickAdd: "tw> 10.00 @conto @contanti")
        try await store.core.execute(vaultId: vault.id, .voidTransaction(transactionId: try Self.id("latte", in: store)))
        store.showVoided = true
        store.showTransfers = true
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(store.rows.count == 5)

        store.selectAllRows()
        #expect(store.selection.count == 5)
        #expect(store.bulkTargets.map(\.note) == ["pane", "caffè", "benzina"])

        let targets = Set(store.bulkTargets.map(\.id))
        await store.voidSelection()
        #expect(store.pendingUndo.map { Set($0.ids) } == targets)
        #expect(store.selection.isEmpty)
    }

    @Test("A bulk void is one pending window, one toast, and one batch when it elapses")
    func bulkVoidIsOneBatch() async throws {
        let store = try await Self.ledger(undoWindow: .zero, sleeper: { _ in })
        let executed = ExecutedCount()
        await store.core.setOnExecuted { executed.record() }

        store.selectAllRows()
        await store.voidSelection()
        #expect(store.pendingUndo?.ids.count == 4)
        #expect(store.rows.isEmpty)
        await store.undoTask?.value

        #expect(store.presentedError == nil)
        #expect(store.pendingUndo == nil)
        #expect(executed.count == 1)
        #expect(store.wallets.first?.balance == 100_000)
        store.showVoided = true
        await store.settle()
        #expect(store.rows.count == 4)
        #expect(store.rows.allSatisfy { $0.voided })
    }

    @Test("A bulk re-categorize is all or nothing: one row refused leaves every row where it was")
    func bulkRecategorizeIsAtomic() async throws {
        let store = try await Self.ledger()
        let manager = try LedgerHistoryTests.attachManager(to: store)
        let vault = try #require(store.currentVault)
        store.selectAllRows()
        // Voided on another device since it was loaded: the grid still shows
        // it live, the core refuses to update it.
        try await store.core.execute(vaultId: vault.id, .voidTransaction(transactionId: try Self.id("latte", in: store)))
        #expect(store.bulkTargets.count == 4)

        await store.setSelectionCategory("Colazione")

        #expect(store.presentedError?.code == "invalid_command")
        #expect(!manager.canUndo)
        store.presentedError = nil
        await store.reload()
        #expect(store.rows.count == 3)
        #expect(store.rows.allSatisfy { $0.category == "Spesa" })
    }

    @Test("A read-only vault offers no selection")
    func readOnlyHasNoSelection() async throws {
        let store = try await Self.ledger()
        store.setReadOnlyVaults([try #require(store.currentVault).id])

        store.toggleSelection(try Self.id("pane", in: store))
        store.selectAllRows()
        #expect(store.selection.isEmpty)
    }
}
