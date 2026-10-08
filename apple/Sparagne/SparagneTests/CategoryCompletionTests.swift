import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The list under a CATEGORY cell: the ranking on its own, then the model the
/// cell drives, over a store with real categories.
struct CategoryCompletionTests {
    private static func category(_ name: String, system: Bool = false, archived: Bool = false) -> CategoryView {
        CategoryView(id: "id-\(name)", name: name, isSystem: system, archived: archived)
    }

    private static func alias(_ alias: String, of name: String) -> AliasView {
        AliasView(id: "alias-\(alias)", categoryId: "id-\(name)", alias: alias)
    }

    @Test("A name from its start beats an alias from its start, which beats a name or an alias anywhere")
    func tiers() {
        let categories = ["Casa", "Macchina", "Spesa", "Bar", "Vacanze", "Regali"].map { Self.category($0) }
        let aliases = [
            Self.alias("caffè", of: "Bar"),
            Self.alias("carburante", of: "Macchina"),
            Self.alias("bancarella", of: "Regali"),
            // Casa already matches by name: it shows once, as itself.
            Self.alias("casetta", of: "Casa"),
        ]

        let found = CategoryCompletion.candidates(for: "ca", categories: categories, aliases: aliases, recent: [])

        #expect(found.map(\.name) == ["Casa", "Bar", "Macchina", "Vacanze", "Regali"])
        #expect(found.map(\.alias) == [nil, "caffè", "carburante", nil, "bancarella"])
    }

    @Test("Within a tier the recent categories come first, most recent first, then the rest alphabetically")
    func recentFirst() {
        let categories = ["Cane", "Casa", "Calcio", "Caffè"].map { Self.category($0) }
        let found = CategoryCompletion.candidates(
            for: "ca",
            categories: categories,
            aliases: [],
            recent: ["id-Casa", "id-Cane"]
        )
        #expect(found.map(\.name) == ["Casa", "Cane", "Caffè", "Calcio"])
    }

    @Test("Case and accents never count, in names and in aliases")
    func folding() {
        let categories = [Self.category("Città"), Self.category("Bar")]
        let aliases = [Self.alias("caffè", of: "Bar")]

        #expect(CategoryCompletion.candidates(for: "CITTA", categories: categories, aliases: aliases, recent: []).map(\.name) == ["Città"])
        #expect(CategoryCompletion.candidates(for: "caffe", categories: categories, aliases: aliases, recent: []).map(\.name) == ["Bar"])
        #expect(CategoryCompletion.candidates(for: "  ", categories: categories, aliases: aliases, recent: []).isEmpty)
    }

    @Test("System and archived categories are never offered, and the list stops at eight")
    func exclusionsAndLimit() {
        var categories = (1...10).map { Self.category("Casa \($0)") }
        categories.append(Self.category("Uncategorized", system: true))
        categories.append(Self.category("Carta", archived: true))

        let found = CategoryCompletion.candidates(for: "ca", categories: categories, aliases: [], recent: [])
        #expect(found.count == CategoryCompletion.limit)
        #expect(!found.contains { $0.name == "Carta" })
        #expect(CategoryCompletion.candidates(for: "unc", categories: categories, aliases: [], recent: []).isEmpty)
    }

    // MARK: - The model, over a store

    /// Vault `Casa` with rows filed under three categories, the most recent
    /// last, and the completion data loaded as a cell taking the caret would.
    private static func store() async throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.completion.\(UUID().uuidString)"))
        let store = AppStore(core: try CoreActor.inMemory(author: "matteo"), defaults: defaults)
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        for category in ["Carburante", "Casa", "Cassa comune"] {
            await store.addRow(day: Date(), flowId: nil, category: category, note: category.lowercased(), amount: 100)
        }
        await store.addCategoryAliasForTests("bollette", to: "Casa")
        await store.loadCategoryCompletion()
        #expect(store.presentedError == nil)
        return store
    }

    @Test("The model ranks by what was used last, moves with wrap-around, and a pick writes the name without reopening")
    func modelPicks() async throws {
        let store = try await Self.store()
        #expect(store.recentCategoryIds.count == 3)
        let model = CategoryCompletionModel()
        var written: String?

        model.textChanged("ca", owner: "cell", store: store) { written = $0 }
        // The newest row's category first.
        #expect(model.candidates.map(\.name) == ["Cassa comune", "Casa", "Carburante"])
        #expect(model.owner == AnyHashable("cell"))

        model.move(by: -1)
        #expect(model.selected?.name == "Carburante")
        model.move(by: 1)
        #expect(model.selected?.name == "Cassa comune")
        model.move(by: 1)
        #expect(model.picksOnReturn(over: "ca"))
        #expect(model.pick())
        #expect(written == "Casa")
        #expect(!model.isOpen)

        // The cell changing to the picked name does not open the list again.
        model.textChanged("Casa", owner: "cell", store: store) { written = $0 }
        #expect(!model.isOpen)

        // Typing on does, and an entry equal to the text lets ↩ save the row.
        model.textChanged("casa", owner: "cell", store: store) { written = $0 }
        #expect(model.selected?.name == "Casa")
        #expect(!model.picksOnReturn(over: "casa"))

        // An alias leads to its category, and the cell gets the category.
        model.textChanged("boll", owner: "cell", store: store) { written = $0 }
        #expect(model.candidates.first?.alias == "bollette")
        model.pick()
        #expect(written == "Casa")
    }
}

extension AppStore {
    /// An alias through the same command the SETUP tab sends.
    fileprivate func addCategoryAliasForTests(_ alias: String, to name: String) async {
        guard let category = categories.first(where: { $0.name == name }) else { return }
        await addAlias(categoryId: category.id, alias: alias)
    }
}
