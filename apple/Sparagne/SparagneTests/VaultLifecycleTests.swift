import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Renaming and deleting a vault from the store: what the window shows
/// afterwards, and what the core is left with. The sync side of the same
/// commands is in `SyncEngineTests`.
@MainActor
struct VaultLifecycleTests {
    private static func store() throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.vault.\(UUID().uuidString)"))
        return AppStore(core: try CoreActor.inMemory(author: "tester"), defaults: defaults, undoWindow: .seconds(60))
    }

    @Test("Renaming the vault renames it in the list, the selection and the snapshot")
    func rename() async throws {
        let store = try Self.store()
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Cash", openingBalance: 10_000)
        let vault = try #require(store.currentVault)

        await store.renameVault(vault.id, name: " Casa ")

        #expect(store.presentedError == nil)
        #expect(store.currentVault?.id == vault.id)
        #expect(store.currentVault?.name == "Casa")
        #expect(store.vaults.map(\.name) == ["Casa"])
        #expect(store.snapshot?.name == "Casa")
        // Nothing else moved.
        #expect(store.currentVault?.owner == "tester")
        #expect(store.wallets.first?.balance == 10_000)
    }

    @Test("A name another of my vaults already has is refused, and nothing changes")
    func renameClash() async throws {
        let store = try Self.store()
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Cash", openingBalance: 0)
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 0)
        let casa = try #require(store.currentVault)
        #expect(casa.name == "Casa")

        await store.renameVault(casa.id, name: "main")

        #expect(store.presentedError?.code == "already_exists")
        #expect(store.currentVault?.name == "Casa")
        #expect(store.vaults.map(\.name).sorted() == ["Casa", "Main"])
    }

    @Test("Deleting the last vault empties the window and asks for onboarding again")
    func deleteTheLastVault() async throws {
        let store = try Self.store()
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Cash", openingBalance: 10_000)
        await store.submit(quickAdd: "-12.50 pizza #food")
        let vault = try #require(store.currentVault)

        await store.deleteVault(vault.id)

        #expect(store.presentedError == nil)
        #expect(store.needsOnboarding)
        #expect(store.currentVault == nil)
        #expect(store.vaults.isEmpty)
        #expect(store.rows.isEmpty)
        #expect(store.snapshot == nil)
        // The log survives the projection, the deletion at its end: vault,
        // wallet, pizza, delete.
        #expect(try await store.core.deletedVaults() == [vault.id])
        #expect(try await store.core.syncState(vaultId: vault.id).outbox == 4)

        // Onboarding again works, and the old name is free.
        await store.createVault(name: "Main", walletName: "Cash", openingBalance: 0)
        #expect(store.presentedError == nil)
        #expect(!store.needsOnboarding)
        #expect(store.currentVault?.name == "Main")
        #expect(store.currentVault?.id != vault.id)
        #expect(store.rows.isEmpty)
    }

    @Test("Deleting one vault of two moves the window to the other")
    func deleteOneOfTwo() async throws {
        let store = try Self.store()
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Cash", openingBalance: 0)
        let main = try #require(store.currentVault)
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 0)
        let casa = try #require(store.currentVault)
        #expect(casa.id != main.id)

        await store.deleteVault(casa.id)

        #expect(store.presentedError == nil)
        #expect(!store.needsOnboarding)
        #expect(store.currentVault?.id == main.id)
        #expect(store.vaults.map(\.id) == [main.id])
        #expect(store.wallets.map(\.name) == ["Cash"])
    }

    @Test("A row waiting on the undo toast dies with its vault, without an error")
    func pendingUndoIsDropped() async throws {
        let store = try Self.store()
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Cash", openingBalance: 10_000)
        await store.submit(quickAdd: "-12.50 pizza #food")
        let row = try #require(store.rows.first { $0.note == "pizza" })
        await store.void(transactionId: row.id)
        #expect(store.pendingUndo != nil)
        let vault = try #require(store.currentVault)

        await store.deleteVault(vault.id)

        #expect(store.presentedError == nil)
        #expect(store.pendingUndo == nil)
        #expect(store.needsOnboarding)
    }

    @Test("Only the owner may delete; renaming is open to every writer")
    func onlyTheOwnerDeletes() async throws {
        let store = try Self.store()
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Cash", openingBalance: 0)
        let vault = try #require(store.currentVault)
        #expect(vault.owner == "tester")

        await store.setAuthor("intruder")
        await store.deleteVault(vault.id)

        #expect(store.presentedError?.code == "forbidden")
        #expect(store.currentVault?.id == vault.id)
        #expect(store.vaults.count == 1)
        #expect(try await store.core.deletedVaults().isEmpty)

        store.presentedError = nil
        await store.renameVault(vault.id, name: "Casa")
        #expect(store.presentedError == nil)
        #expect(store.currentVault?.name == "Casa")
        #expect(store.currentVault?.owner == "tester")
    }
}
