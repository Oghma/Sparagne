import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The rules of the CATEGORIE table: what a typed row
/// sends to the core, and which rows can be typed at all. The diff lives in
/// `CategoryDraft`, so none of this needs a view.
struct CategoryTableTests {
    private static func makeStore() throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.categories.\(UUID().uuidString)"))
        return AppStore(core: try CoreActor.inMemory(author: "matteo"), defaults: defaults)
    }

    /// A vault with its system categories and one of the user's own.
    private static func vault() async throws -> AppStore {
        let store = try makeStore()
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        await store.loadCategoryManagement()
        #expect(store.presentedError == nil)
        return store
    }

    // MARK: - The alias line

    @Test("A typed alias line sends one removal per name gone and one addition per new one")
    func aliasDiff() {
        var draft = CategoryDraft(name: "Casa", aliases: ["mutuo", "affitto"])
        draft.aliases = "mutuo, bolletta"

        let changes = draft.aliasChanges(from: ["mutuo", "affitto"])
        #expect(changes.removed == ["affitto"])
        #expect(changes.added == ["bolletta"])
    }

    @Test("Case and spacing alone are not a change, so an untouched line sends nothing")
    func aliasDiffIgnoresCaseAndSpacing() {
        var draft = CategoryDraft(name: "Casa", aliases: ["Mutuo", "affitto"])
        draft.aliases = "  MUTUO ,affitto  "

        let changes = draft.aliasChanges(from: ["Mutuo", "affitto"])
        #expect(changes.removed.isEmpty)
        #expect(changes.added.isEmpty)
    }

    @Test("Empty entries are dropped and a name repeated in another case counts once")
    func aliasListIsCleaned() {
        var draft = CategoryDraft()
        draft.aliases = "mutuo, , affitto,, MUTUO ,"
        #expect(draft.aliasList == ["mutuo", "affitto"])

        draft.aliases = " , ,"
        #expect(draft.aliasList.isEmpty)

        // Clearing the line removes every alias the category had.
        let changes = draft.aliasChanges(from: ["mutuo"])
        #expect(changes.removed == ["mutuo"])
        #expect(changes.added.isEmpty)
    }

    // MARK: - The name cell

    @Test("The name cell renames only when it says something new")
    func nameDiff() {
        var draft = CategoryDraft(name: "Casa", aliases: [])
        #expect(draft.rename(from: "Casa") == nil)

        draft.name = "  Casa  "
        #expect(draft.rename(from: "Casa") == nil)

        draft.name = " Abitazione "
        #expect(draft.rename(from: "Casa") == "Abitazione")

        // An emptied cell leaves the name alone, as the ledger's amount does.
        draft.name = "   "
        #expect(draft.rename(from: "Casa") == nil)
    }

    // MARK: - The empty line

    @Test("The empty line creates a category, and a blank name creates nothing")
    func emptyLineCreates() async throws {
        let store = try await Self.vault()
        #expect(CategoryDraft.creation(from: "   ") == nil)

        let name = try #require(CategoryDraft.creation(from: "  Spesa  "))
        await store.createCategory(name: name)
        #expect(store.presentedError == nil)
        #expect(store.windowCategories.contains { $0.name == "Spesa" })
    }

    // MARK: - System categories

    @Test("System categories are read-only, and are shown under their localized name")
    func systemCategoriesAreReadOnly() async throws {
        let store = try await Self.vault()
        let system = try #require(store.windowCategories.first { $0.isSystem })
        #expect(!CategoryDraft.isEditable(system))
        #expect(!CategoryDraft.label(system).isEmpty)

        await store.createCategory(name: "Spesa")
        let own = try #require(store.windowCategories.first { $0.name == "Spesa" })
        #expect(CategoryDraft.isEditable(own))
        #expect(CategoryDraft.label(own) == "Spesa")

        // An archived row is restored from the menu, not retyped.
        await store.archiveCategory(own.id)
        let archived = try #require(store.windowCategories.first { $0.id == own.id })
        #expect(archived.archived)
        #expect(!CategoryDraft.isEditable(archived))
    }
}
