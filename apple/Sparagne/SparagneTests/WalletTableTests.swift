import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The draft behind the setup tab's wallet table (`docs/v2/UI.md` §2.3): what
/// a renamed row sends back and what the empty line creates. The draft is a
/// plain struct, so none of this needs a view.
struct WalletTableTests {
    private static func makeStore() throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.wallets.\(UUID().uuidString)"))
        return AppStore(core: try CoreActor.inMemory(author: "matteo"), defaults: defaults)
    }

    private static func vault() async throws -> AppStore {
        let store = try makeStore()
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        #expect(store.presentedError == nil)
        return store
    }

    private static func wallet(name: String = "Conto") -> WalletView {
        WalletView(id: UUID().uuidString, name: name, balance: 0, archived: false)
    }

    // MARK: - A renamed row

    @Test("A row nobody touched sends nothing")
    func unchangedDraft() {
        let wallet = Self.wallet()
        #expect(WalletDraft(wallet: wallet).rename(against: wallet) == nil)
    }

    @Test("A renamed row sends the trimmed name")
    func renamed() {
        let wallet = Self.wallet(name: "Conto")
        var draft = WalletDraft(wallet: wallet)
        draft.name = "  Conto BPER  "
        #expect(draft.rename(against: wallet) == "Conto BPER")
    }

    @Test("An emptied name leaves the name alone")
    func emptiedName() {
        let wallet = Self.wallet()
        var draft = WalletDraft(wallet: wallet)
        draft.name = "   "
        #expect(draft.rename(against: wallet) == nil)
    }

    // MARK: - The empty line

    @Test("An empty name is nothing to save, not an error")
    func blankLineCreatesNothing() throws {
        #expect(try WalletDraft().wallet(currency: .eur) == nil)
    }

    @Test("An empty SALDO opens the wallet at zero")
    func emptyOpeningIsZero() throws {
        var line = WalletDraft()
        line.name = "Cash"
        let entry = try #require(try line.wallet(currency: .eur))
        #expect(entry.name == "Cash")
        #expect(entry.opening == 0)
    }

    @Test("A SALDO that does not parse is an error, not a zero")
    func badOpening() {
        var line = WalletDraft()
        line.name = "Cash"
        line.opening = "dieci"
        #expect(throws: (any Error).self) { try line.wallet(currency: .eur) }
    }

    @Test("The empty line creates the wallet and its opening goes to Unallocated")
    func newLineCreates() async throws {
        let store = try await Self.vault()
        var line = WalletDraft()
        line.name = "  Carta  "
        line.opening = "-250,00"

        let entry = try #require(try line.wallet(currency: store.currency))
        #expect(entry.name == "Carta")
        #expect(entry.opening == -25_000)

        await store.createWallet(name: entry.name, openingBalance: entry.opening)
        #expect(store.presentedError == nil)

        let created = try #require(store.wallets.first { $0.name == "Carta" })
        #expect(created.balance == -25_000)
        // A card that starts in debt takes it out of Unallocated.
        let unallocated = try #require(store.flows.first { $0.isUnallocated })
        #expect(unallocated.balance == 100_000 - 25_000)
    }

    @Test("The rename reaches the core")
    func renameReachesTheStore() async throws {
        let store = try await Self.vault()
        let wallet = try #require(store.wallets.first { $0.name == "Conto" })
        var draft = WalletDraft(wallet: wallet)
        draft.name = "Conto BPER"

        let name = try #require(draft.rename(against: wallet))
        await store.renameWallet(wallet.id, name: name)
        #expect(store.presentedError == nil)
        #expect(store.wallets.first { $0.id == wallet.id }?.name == "Conto BPER")
    }
}
