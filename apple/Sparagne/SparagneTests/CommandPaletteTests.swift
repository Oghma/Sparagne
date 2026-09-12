import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The ⌘K field's second grammar (`docs/v2/UI.md` §6): a line that starts
/// with `>` is a command palette. `CommandPaletteModel` holds the actions,
/// the filter and the selection, and none of it needs a window.
@MainActor
struct CommandPaletteTests {
    /// A box a test action can write to, so "it ran" is an observable fact.
    private final class Box {
        var value = ""
    }

    /// Three actions with titles chosen so prefix, word-prefix, substring and
    /// keyword matches can be told apart.
    private static func sample(_ box: Box = Box()) -> [PaletteAction] {
        [
            PaletteAction(id: "a", title: "Previous Month", keywords: ["indietro"]) { box.value = "a" },
            PaletteAction(id: "b", title: "Next Month", keywords: ["avanti"]) { box.value = "b" },
            PaletteAction(id: "c", title: "Città", keywords: ["export", "csv"]) { box.value = "c" },
        ]
    }

    private static func store(author: String = "matteo") throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.palette.\(UUID().uuidString)"))
        return AppStore(core: try CoreActor.inMemory(author: author), defaults: defaults)
    }

    // MARK: - The `>` marker

    @Test("Only a leading > turns the quick-add field into a palette")
    func commandDetection() {
        #expect(CommandPaletteModel.isCommand(">"))
        #expect(CommandPaletteModel.isCommand(">month"))
        #expect(!CommandPaletteModel.isCommand(""))
        // The quick-add grammar's own envelope marker, mid-line.
        #expect(!CommandPaletteModel.isCommand("-12.50 pizza >groceries"))
        #expect(!CommandPaletteModel.isCommand(" >month"))
    }

    @Test("The query is what follows the marker, trimmed")
    func queryExtraction() {
        #expect(CommandPaletteModel.query(in: ">") == "")
        #expect(CommandPaletteModel.query(in: ">  month ") == "month")
        // Not a command: nothing to query with.
        #expect(CommandPaletteModel.query(in: "12 pizza") == "")
    }

    // MARK: - Filtering

    @Test("An empty query lists every action, in the order they were registered")
    func emptyQueryKeepsOrder() {
        let model = CommandPaletteModel(actions: Self.sample())
        #expect(model.results.map(\.id) == ["a", "b", "c"])
    }

    @Test("A title match from the start outranks one from the middle")
    func prefixBeatsSubstring() {
        let actions = [
            PaletteAction(id: "middle", title: "Show Voided") {},
            PaletteAction(id: "start", title: "Void the row") {},
        ]
        #expect(CommandPaletteModel.filter(actions, query: "void").map(\.id) == ["start", "middle"])
    }

    @Test("A word inside the title matches, and ties keep the registration order")
    func wordPrefixMatches() {
        let model = CommandPaletteModel(actions: Self.sample())
        model.query = "month"
        #expect(model.results.map(\.id) == ["a", "b"])
    }

    @Test("Matching ignores case and accents")
    func foldedMatching() {
        let model = CommandPaletteModel(actions: Self.sample())
        model.query = "CITTA"
        #expect(model.results.map(\.id) == ["c"])
        model.query = "città"
        #expect(model.results.map(\.id) == ["c"])
    }

    @Test("Keywords find an action its title does not name, and rank below titles")
    func keywordMatching() {
        let actions = [
            PaletteAction(id: "hidden", title: "Manage\u{2026}", keywords: ["csv"]) {},
            PaletteAction(id: "titled", title: "Export CSV\u{2026}") {},
        ]
        #expect(CommandPaletteModel.filter(actions, query: "csv").map(\.id) == ["titled", "hidden"])
    }

    @Test("Nothing matching leaves no results and nothing to run")
    func noMatch() async {
        let model = CommandPaletteModel(actions: Self.sample())
        model.query = "zzz"
        #expect(model.results.isEmpty)
        #expect(model.selected == nil)
        #expect(await model.run() == false)
    }

    // MARK: - Selection

    @Test("The arrows wrap at both ends of the list")
    func selectionWraps() {
        let model = CommandPaletteModel(actions: Self.sample())
        #expect(model.selection == 0)
        model.move(by: 1)
        model.move(by: 1)
        #expect(model.selection == 2)
        model.move(by: 1)
        #expect(model.selection == 0)
        model.move(by: -1)
        #expect(model.selection == 2)
    }

    @Test("Typing puts the selection back on the first result")
    func typingResetsSelection() {
        let model = CommandPaletteModel(actions: Self.sample())
        model.move(by: 2)
        #expect(model.selection == 2)
        model.query = "month"
        #expect(model.selection == 0)
        #expect(model.selected?.id == "a")
    }

    @Test("↩ runs the highlighted action, not the first one")
    func runsTheSelection() async {
        let box = Box()
        let model = CommandPaletteModel(actions: Self.sample(box))
        model.query = "month"
        model.move(by: 1)
        #expect(await model.run())
        #expect(box.value == "b")
    }

    // MARK: - The app's own actions

    @Test("Every other vault is an entry; the one on screen is not")
    func vaultEntries() async throws {
        let store = try Self.store()
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 0)
        await store.createVault(name: "Lavoro", walletName: "Conto", openingBalance: 0)
        let current = try #require(store.currentVault)
        let other = try #require(store.vaults.first { $0.id != current.id })

        let ids = CommandPaletteModel.ledgerActions(store: store, engine: nil).map(\.id)
        #expect(ids.contains("vault.\(other.id)"))
        #expect(!ids.contains("vault.\(current.id)"))
    }

    @Test("Running the vault entry switches the vault on screen")
    func vaultEntrySwitches() async throws {
        let store = try Self.store()
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 0)
        await store.createVault(name: "Lavoro", walletName: "Conto", openingBalance: 0)
        let other = try #require(store.vaults.first { $0.id != store.currentVault?.id })

        let model = CommandPaletteModel(actions: CommandPaletteModel.ledgerActions(store: store, engine: nil))
        let entry = try #require(model.actions.first { $0.id == "vault.\(other.id)" })
        await entry.run()
        #expect(store.currentVault?.id == other.id)
    }

    @Test("The tab entries switch the view")
    func tabEntries() async throws {
        let store = try Self.store()
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 0)
        store.tab = .summary

        let actions = CommandPaletteModel.ledgerActions(store: store, engine: nil)
        let entry = try #require(actions.first { $0.id == "tab.ledger" })
        await entry.run()
        #expect(store.tab == .ledger)
    }

    @Test("The three view toggles are entries, and flip the state they name")
    func toggleEntries() async throws {
        let store = try Self.store()
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 0)

        let actions = CommandPaletteModel.ledgerActions(store: store, engine: nil)
        for id in ["toggle.voided", "toggle.transfers", "toggle.wallet"] {
            #expect(actions.contains { $0.id == id })
        }
        await (try #require(actions.first { $0.id == "toggle.wallet" })).run()
        #expect(store.showWalletColumn)

        // Rebuilt after the flip, the entry offers the opposite.
        let again = CommandPaletteModel.ledgerActions(store: store, engine: nil)
        let wallet = try #require(again.first { $0.id == "toggle.wallet" })
        #expect(wallet.title == String(localized: "Hide Wallet Column"))
    }

    @Test("Without a sync engine there is no sync entry")
    func syncEntryNeedsAnEngine() async throws {
        let store = try Self.store()
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 0)
        #expect(!CommandPaletteModel.ledgerActions(store: store, engine: nil).contains { $0.id == "sync.now" })
    }
}
