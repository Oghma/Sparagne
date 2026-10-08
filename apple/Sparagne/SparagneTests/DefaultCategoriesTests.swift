import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The categories a new vault starts with (`DefaultCategories`): which list a
/// language gets, that the core takes each list as it is, and that the store
/// writes it together with the vault.
struct DefaultCategoriesTests {
    private static func store(_ categories: [DefaultCategory]) throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.defaults.\(UUID().uuidString)"))
        return AppStore(
            core: try CoreActor.inMemory(author: "matteo"),
            defaults: defaults,
            defaultCategories: categories
        )
    }

    @Test("Italian for an Italian window, English for any other language")
    func language() {
        #expect(DefaultCategories.list(for: "it") == DefaultCategories.italian)
        #expect(DefaultCategories.list(for: "it-CH") == DefaultCategories.italian)
        #expect(DefaultCategories.list(for: "en") == DefaultCategories.english)
        #expect(DefaultCategories.list(for: "de") == DefaultCategories.english)
    }

    @Test("Every name is one word, so the quick-add can reach it as #Name")
    func oneWordNames() {
        for category in DefaultCategories.italian + DefaultCategories.english {
            #expect(!category.name.contains(" "), "\(category.name)")
        }
    }

    @Test("A new vault holds every category and alias of the list", arguments: ["it", "en"])
    func createdWithTheVault(language: String) async throws {
        let list = DefaultCategories.list(for: language)
        let store = try Self.store(list)
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        await store.loadCategoryManagement()

        #expect(store.presentedError == nil)
        let own = store.windowCategories.filter { !$0.isSystem }
        #expect(Set(own.map(\.name)) == Set(list.map(\.name)))
        let names = Dictionary(uniqueKeysWithValues: own.map { ($0.id, $0.name) })
        let aliases = store.categoryAliases.map { "\(names[$0.categoryId] ?? "?"): \($0.alias)" }
        let expected = list.flatMap { category in category.aliases.map { "\(category.name): \($0)" } }
        #expect(Set(aliases) == Set(expected))
        #expect(aliases.count == expected.count)
        #expect(store.wallets.map(\.name) == ["Conto"])
    }

    @Test("The vault, its wallet and every category and alias wait in the outbox, one command each")
    func outbox() async throws {
        let list = DefaultCategories.italian
        let store = try Self.store(list)
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        let vault = try #require(store.currentVault)

        let aliases = list.map(\.aliases.count).reduce(0, +)
        #expect(try await store.core.syncState(vaultId: vault.id).outbox == UInt32(2 + list.count + aliases))
    }

    @Test("A vault made without a wallet still gets its categories")
    func withoutAWallet() async throws {
        let store = try Self.store(DefaultCategories.english)
        await store.bootstrap()
        await store.createVault(name: "Home", walletName: "  ", openingBalance: 0)
        await store.loadCategoryManagement()

        #expect(store.presentedError == nil)
        #expect(store.wallets.isEmpty)
        #expect(store.windowCategories.filter { !$0.isSystem }.count == DefaultCategories.english.count)
    }

    @Test("An alias files a quick-add under its category")
    func aliasInQuickAdd() async throws {
        let store = try Self.store(DefaultCategories.italian)
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)

        await store.submit(quickAdd: "-12.50 margherita #pizza")

        #expect(store.presentedError == nil)
        #expect(store.rows.map(\.category) == ["Ristoranti"])
    }

    @Test("A store given no list makes a vault with the system categories only")
    func noList() async throws {
        let store = try Self.store([])
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 0)
        await store.loadCategoryManagement()

        #expect(store.presentedError == nil)
        #expect(store.windowCategories.allSatisfy { $0.isSystem })
        #expect(store.categoryAliases.isEmpty)
    }
}
