import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The ⌘K field's second grammar: a line that starts
/// with `>` is a command palette. `CommandPaletteModel` holds the actions,
/// the filter and the selection, and none of it needs a window.
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

    /// Vault `Casa`, and the vaults `names` made after it.
    private static func store(also names: [String] = []) async throws -> AppStore {
        let store = try await TestStore.make(opening: 0)
        for name in names {
            await store.createVault(name: name, walletName: "Conto", openingBalance: 0)
        }
        return store
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
        #expect(model.list.selected == nil)
        #expect(await model.run() == false)
    }

    // MARK: - Selection

    @Test("Typing puts the selection back on the first result")
    func typingResetsSelection() {
        let model = CommandPaletteModel(actions: Self.sample())
        model.list.move(by: 2)
        #expect(model.list.selection == 2)
        model.query = "month"
        #expect(model.list.selection == 0)
        #expect(model.list.selected?.id == "a")
    }

    @Test("↩ runs the highlighted action, not the first one")
    func runsTheSelection() async {
        let box = Box()
        let model = CommandPaletteModel(actions: Self.sample(box))
        model.query = "month"
        model.list.move(by: 1)
        #expect(await model.run())
        #expect(box.value == "b")
    }

    // MARK: - The app's own actions

    @Test("Every other vault is an entry; the one on screen is not")
    func vaultEntries() async throws {
        let store = try await Self.store(also: ["Lavoro"])
        let current = try #require(store.currentVault)
        let other = try #require(store.vaults.first { $0.id != current.id })

        let ids = CommandPaletteModel.ledgerActions(store: store, engine: nil).map(\.id)
        #expect(ids.contains("vault.\(other.id)"))
        #expect(!ids.contains("vault.\(current.id)"))
    }

    @Test("Two vaults with the same name are told apart by their owner")
    func sameNamedVaultEntries() async throws {
        let store = try await Self.store(also: ["casa", "Lavoro"])
        let twins = store.vaults.filter { $0.name.lowercased() == "casa" }
        #expect(twins.count == 2)

        let titles = CommandPaletteModel.ledgerActions(store: store, engine: nil).map(\.title)
        for twin in twins {
            #expect(titles.contains(String(localized: "Vault: \("\(twin.name) (matteo)")")))
        }
        // A name nobody else has stays as it is.
        #expect(!titles.contains { $0.contains("Lavoro (") })
    }

    @Test("New Vault is always an entry; Rename and Delete only with a vault on screen")
    func vaultLifecycleEntries() async throws {
        let store = try TestStore.fresh()
        await store.bootstrap()
        let before = CommandPaletteModel.ledgerActions(store: store, engine: nil).map(\.id)
        #expect(before.contains("vault.new"))
        #expect(!before.contains("vault.rename"))
        #expect(!before.contains("vault.delete"))

        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 0)
        let after = CommandPaletteModel.ledgerActions(store: store, engine: nil).map(\.id)
        #expect(after.contains("vault.new"))
        #expect(after.contains("vault.rename"))
        #expect(after.contains("vault.delete"))
    }

    @Test("Running the vault entry switches the vault on screen")
    func vaultEntrySwitches() async throws {
        let store = try await Self.store(also: ["Lavoro"])
        let other = try #require(store.vaults.first { $0.id != store.currentVault?.id })

        let model = CommandPaletteModel(actions: CommandPaletteModel.ledgerActions(store: store, engine: nil))
        let entry = try #require(model.actions.first { $0.id == "vault.\(other.id)" })
        await entry.run()
        #expect(store.currentVault?.id == other.id)
    }

    @Test("Every sheet tab is an entry, in the tab bar's order, and switches to it")
    func tabEntries() async throws {
        let store = try await Self.store()

        let actions = CommandPaletteModel.ledgerActions(store: store, engine: nil)
        let tabIds = actions.map(\.id).filter { $0.hasPrefix("tab.") }
        #expect(tabIds == ["tab.summary", "tab.ledger", "tab.recurring", "tab.allocation", "tab.setup"])

        // From another tab each time, so the run is what moved it.
        for tab in LedgerTab.allCases {
            store.tab = tab == .summary ? .ledger : .summary
            let entry = try #require(actions.first { $0.id == "tab.\(tab.rawValue)" })
            await entry.run()
            #expect(store.tab == tab)
        }
    }

    @Test("The three view toggles are entries, and flip the state they name")
    func toggleEntries() async throws {
        let store = try await Self.store()

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
        let store = try await Self.store()
        #expect(!CommandPaletteModel.ledgerActions(store: store, engine: nil).contains { $0.id == "sync.now" })
    }

    // MARK: - Changing the kind

    @Test("Each kind the rows can become is an entry, in the order given, and running one changes to it")
    func kindEntries() async throws {
        let box = Box()
        let actions = CommandPaletteModel.kindActions(targets: [.income, .refund]) { box.value = "\($0)" }
        #expect(actions.map(\.id) == ["kind.income", "kind.refund"])
        #expect(actions.map(\.title) == [String(localized: "Change to Income"), String(localized: "Change to Refund")])

        await (try #require(actions.last)).run()
        #expect(box.value == "refund")
    }

    @Test("No kind to change into, no entry")
    func noKindEntries() {
        #expect(CommandPaletteModel.kindActions(targets: []) { _ in }.isEmpty)
    }

    @Test("Spesa finds the change to an expense, whose title says uscita")
    func kindEntryKeywords() {
        let actions = CommandPaletteModel.kindActions(targets: [.expense, .income, .refund]) { _ in }
        #expect(CommandPaletteModel.filter(actions, query: "spesa").map(\.id) == ["kind.expense"])
        #expect(CommandPaletteModel.filter(actions, query: "tipo").map(\.id) == ["kind.expense", "kind.income", "kind.refund"])
    }

    @Test("With nothing picked and no cursor, or on another tab, the palette changes no kind")
    func kindEntriesNeedARow() async throws {
        let store = try await Self.store()
        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "coop", amount: 1_250)
        store.tab = .ledger
        #expect(!CommandPaletteModel.ledgerActions(store: store, engine: nil).contains { $0.id.hasPrefix("kind.") })

        // A row picked, but the Mastro is not on screen.
        let row = try #require(store.rows.first)
        store.toggleSelection(row.id)
        store.tab = .summary
        #expect(!CommandPaletteModel.ledgerActions(store: store, engine: nil).contains { $0.id.hasPrefix("kind.") })
    }
}
