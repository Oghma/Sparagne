import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Counts the calls that reach the core, through `CoreActor`'s probe.
private actor CoreCalls {
    private(set) var count = 0

    func record() { count += 1 }
}

/// A vault this account only reads (a viewer's): the window offers nothing
/// that would write, and what gets through anyway is refused before it
/// reaches the core.
@MainActor
struct ReadOnlyLedgerTests {
    /// Vault `Main` with wallet `Cash`, marked read-only once it is set up,
    /// and the counter of every core call made from then on.
    private static func viewer() async throws -> (store: AppStore, calls: CoreCalls) {
        let calls = CoreCalls()
        let defaults = try #require(UserDefaults(suiteName: "sparagne.readonly.\(UUID().uuidString)"))
        let store = AppStore(
            core: try CoreActor.inMemory(author: "tester", probe: { await calls.record() }),
            defaults: defaults
        )
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Cash", openingBalance: 10_000)
        await store.submit(quickAdd: "-12.50 pizza #food")
        #expect(store.presentedError == nil)
        #expect(store.canWrite)
        store.setReadOnlyVaults([try #require(store.currentVault).id])
        return (store, calls)
    }

    @Test("A quick-add line is refused with a message, and the core never hears of it")
    func quickAddIsRefused() async throws {
        let (store, calls) = try await Self.viewer()
        let before = await calls.count
        store.quickAddText = "-5.00 hotel"

        await store.submit(quickAdd: store.quickAddText)

        #expect(store.presentedError?.code == "forbidden")
        #expect(await calls.count == before)
        // The line is left as typed, not swallowed.
        #expect(store.quickAddText == "-5.00 hotel")
        #expect(!store.rows.contains { $0.note == "hotel" })
    }

    @Test("The grid offers no empty line and the writes behind it are refused")
    func theGridOffersNoNewRow() async throws {
        let (store, calls) = try await Self.viewer()
        let before = await calls.count
        #expect(store.isReadOnly)
        #expect(!store.canWrite)

        let pizza = try #require(store.rows.first { $0.note == "pizza" })
        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "coop", amount: 1_000)
        #expect(store.presentedError?.code == "forbidden")
        store.presentedError = nil

        var patch = TransactionPatch()
        patch.note = "pizza margherita"
        await store.update(transactionId: pizza.id, patch: patch)
        #expect(store.presentedError?.code == "forbidden")

        // Void is not offered, and not taken either.
        await store.void(transactionId: pizza.id)
        #expect(store.pendingUndo == nil)
        #expect(await calls.count == before)
    }

    @Test("Another vault of the same account is writable again")
    func writableFollowsTheVaultOnScreen() async throws {
        let (store, _) = try await Self.viewer()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 0)

        #expect(store.presentedError == nil)
        #expect(store.currentVault?.name == "Casa")
        #expect(store.canWrite)
    }
}
