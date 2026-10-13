import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// "Set Envelope…" over a selection: every live entry moved to one envelope
/// in one batch, all or nothing, one undo step back.
struct BulkEnvelopeTests {
    /// Vault `Casa` with envelopes Spesa (may go negative) and Risparmi (may
    /// not, empty), and three expenses on Spesa.
    private static func ledger() async throws -> (store: AppStore, manager: UndoManager, spesa: Uuid, risparmi: Uuid) {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.bulk-envelope.\(UUID().uuidString)"))
        let store = AppStore(core: try CoreActor.inMemory(author: "matteo"), defaults: defaults)
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        await store.createEnvelope(name: "Spesa", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        await store.createEnvelope(name: "Risparmi", mode: .unlimited, allowNegative: false, openingAllocation: 0)
        let spesa = try #require(store.flows.first { $0.name == "Spesa" }).id
        let risparmi = try #require(store.flows.first { $0.name == "Risparmi" }).id
        for note in ["pane", "latte", "caffè"] {
            await store.addRow(day: Date(), flowId: spesa, category: "Spesa", note: note, amount: 1_000)
        }
        #expect(store.presentedError == nil)
        return (store, try LedgerHistoryTests.attachManager(to: store), spesa, risparmi)
    }

    private static func envelope(_ note: String, in store: AppStore) throws -> Uuid? {
        try #require(store.rows.first { $0.note == note }).flowId
    }

    @Test("The selected rows move to the envelope picked, and one undo puts them all back")
    func movesAndUndoes() async throws {
        let (store, manager, spesa, _) = try await Self.ledger()
        let unallocated = try #require(store.flows.first { $0.isUnallocated }).id
        store.selectAllRows()
        #expect(store.bulkEnvelope == spesa)

        await store.setSelectionEnvelope(unallocated)
        #expect(store.presentedError == nil)
        #expect(store.rows.allSatisfy { $0.flowId == unallocated })
        #expect(store.bulkEnvelope == unallocated)
        #expect(manager.undoActionName == String(localized: "Envelope Change"))

        manager.undo()
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(store.rows.allSatisfy { $0.flowId == spesa })
    }

    @Test("Rows already on the envelope are left out, and rows spread over several share none")
    func skipsRowsAlreadyThere() async throws {
        let (store, manager, spesa, _) = try await Self.ledger()
        let unallocated = try #require(store.flows.first { $0.isUnallocated }).id
        let pane = try #require(store.rows.first { $0.note == "pane" })
        store.toggleSelection(pane.id)
        await store.setSelectionEnvelope(unallocated)
        #expect(try Self.envelope("pane", in: store) == unallocated)

        store.selectAllRows()
        #expect(store.bulkEnvelope == nil)
        await store.setSelectionEnvelope(unallocated)
        #expect(store.rows.allSatisfy { $0.flowId == unallocated })

        // The step down moves latte and caffè back; pane was not part of it.
        manager.undo()
        await store.settle()
        #expect(try Self.envelope("pane", in: store) == unallocated)
        #expect(try Self.envelope("latte", in: store) == spesa)
        #expect(try Self.envelope("caffè", in: store) == spesa)

        // Moved again, then asked again with every row there already: nothing
        // is sent, so the step down is still the real move.
        store.selectAllRows()
        await store.setSelectionEnvelope(unallocated)
        await store.setSelectionEnvelope(unallocated)
        manager.undo()
        await store.settle()
        #expect(try Self.envelope("latte", in: store) == spesa)
    }

    @Test("An envelope that may not go negative refuses the batch, and every row stays where it was")
    func refusedAsAWhole() async throws {
        let (store, manager, spesa, risparmi) = try await Self.ledger()
        store.selectAllRows()

        await store.setSelectionEnvelope(risparmi)

        #expect(store.presentedError?.code == "insufficient_funds")
        #expect(manager.undoActionName != String(localized: "Envelope Change"))
        store.presentedError = nil
        await store.reload()
        #expect(store.rows.allSatisfy { $0.flowId == spesa })
    }
}
