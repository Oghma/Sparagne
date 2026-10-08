import SwiftUI
import SparagneCore

/// One entry of the ⌘K command palette (`docs/v2/UI.md` §6).
///
/// `run` is whatever the menu item of the same name already does: the palette
/// is a second way to reach the app's actions, never a second implementation
/// of them.
struct PaletteAction: Identifiable {
    /// Stable across rebuilds, so a list that is re-made while the field is
    /// open does not lose its identity mid-animation.
    let id: String
    /// Localized: the palette filters on what is on screen, not on a key.
    let title: String
    /// Extra words the entry can be found by, never displayed. Not localized
    /// for that reason: the title already carries the user's language.
    let keywords: [String]
    /// Asynchronous because most of what the app does now goes through the
    /// core actor (switching vault, say); the rest simply never suspends.
    let run: @MainActor () async -> Void

    init(id: String, title: String, keywords: [String] = [], run: @escaping @MainActor () async -> Void) {
        self.id = id
        self.title = title
        self.keywords = keywords
        self.run = run
    }
}

/// The state behind the palette: the actions, the text typed after `>`, and
/// which row the arrows are on.
///
/// Deliberately free of SwiftUI, so the filtering and the selection can be
/// tested without a window (`SparagneTests/CommandPaletteTests.swift`).
@MainActor
@Observable
final class CommandPaletteModel {
    /// Rebuilt every time the palette opens: the vault list and the state of
    /// the two toggles change what the entries say.
    var actions: [PaletteAction] {
        didSet { selection = 0 }
    }

    /// What was typed after the `>`, already stripped of the marker.
    var query: String = "" {
        didSet { if query != oldValue { selection = 0 } }
    }

    /// Index into `results`, not into `actions`.
    private(set) var selection = 0

    init(actions: [PaletteAction] = []) {
        self.actions = actions
    }

    // MARK: - The `>` marker

    /// The field is a palette when its first character is `>`. Quick-add uses
    /// `>` as its envelope marker, but never in first position: a line that
    /// starts with one has no amount and is not a transaction.
    static func isCommand(_ text: String) -> Bool { text.hasPrefix(">") }

    /// The text after the marker.
    static func query(in text: String) -> String {
        guard isCommand(text) else { return "" }
        return String(text.dropFirst()).trimmingCharacters(in: .whitespaces)
    }

    // MARK: - Filtering

    /// Case- and accent-insensitive: `citta` finds "Città", `MESE` finds
    /// "mese".
    static func fold(_ text: String) -> String {
        text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil)
    }

    /// The entries `query` matches, best first. Ties keep the order the
    /// actions were registered in, so an empty field always lists the same
    /// commands in the same places.
    static func filter(_ actions: [PaletteAction], query: String) -> [PaletteAction] {
        let needle = fold(query.trimmingCharacters(in: .whitespaces))
        guard !needle.isEmpty else { return actions }
        return actions.enumerated()
            .compactMap { index, action in
                rank(action, needle).map { (rank: $0, index: index, action: action) }
            }
            .sorted { ($0.rank, $0.index) < ($1.rank, $1.index) }
            .map(\.action)
    }

    /// Lower is better: the title from its start, then from the start of one
    /// of its words, then anywhere in it, then the hidden keywords.
    private static func rank(_ action: PaletteAction, _ needle: String) -> Int? {
        let title = fold(action.title)
        if title.hasPrefix(needle) { return 0 }
        if title.split(whereSeparator: { !$0.isLetter && !$0.isNumber }).contains(where: { $0.hasPrefix(needle) }) {
            return 1
        }
        if title.contains(needle) { return 2 }
        if action.keywords.contains(where: { fold($0).hasPrefix(needle) }) { return 3 }
        if action.keywords.contains(where: { fold($0).contains(needle) }) { return 4 }
        return nil
    }

    var results: [PaletteAction] { Self.filter(actions, query: query) }

    // MARK: - Selection

    var selected: PaletteAction? {
        let list = results
        return list.indices.contains(selection) ? list[selection] : nil
    }

    /// ↓ is `move(by: 1)`, ↑ is `move(by: -1)`; both wrap, so holding one
    /// arrow walks the list round instead of sticking at an end.
    func move(by delta: Int) {
        let count = results.count
        guard count > 0 else {
            selection = 0
            return
        }
        selection = ((selection + delta) % count + count) % count
    }

    /// The pointer picking a row.
    func select(_ index: Int) {
        guard results.indices.contains(index) else { return }
        selection = index
    }

    /// ↩: runs the highlighted entry. `false` when nothing matches, so the
    /// field can stay open instead of closing over a typo.
    @discardableResult
    func run() async -> Bool {
        guard let action = selected else { return false }
        await action.run()
        return true
    }
}

// MARK: - The app's own actions

extension CommandPaletteModel {
    /// Everything the palette can do, in the order it lists them.
    ///
    /// The entries that have a menu item post that item's notification, so
    /// the two ways in share one implementation; the rest write the same
    /// state the top bar and the tab bar write. The three view toggles write
    /// the store, which the window mirrors back into the menu's preference.
    static func ledgerActions(store: AppStore, engine: SyncEngine?) -> [PaletteAction] {
        var actions: [PaletteAction] = [
            PaletteAction(
                id: "month.previous",
                title: String(localized: "Previous Month"),
                keywords: ["month", "mese", "prev"]
            ) {
                NotificationCenter.default.post(name: .stepMonth, object: -1)
            },
            PaletteAction(
                id: "month.next",
                title: String(localized: "Next Month"),
                keywords: ["month", "mese"]
            ) {
                NotificationCenter.default.post(name: .stepMonth, object: 1)
            },
            PaletteAction(
                id: "month.today",
                title: String(localized: "Current Month"),
                keywords: ["today", "oggi", "month", "mese"]
            ) {
                // Through the same notification the arrows use, so the window
                // stays the one place that moves the month.
                let now = MonthKey(Date())
                let delta = (now.year - store.month.year) * 12 + (now.month - store.month.month)
                NotificationCenter.default.post(name: .stepMonth, object: delta)
            },
        ]

        // Every sheet of the tab bar, in its order (⌘1 to ⌘4 from the View
        // menu do the same).
        for tab in LedgerTab.allCases {
            actions.append(
                PaletteAction(
                    id: "tab.\(tab.rawValue)",
                    title: String(localized: "Go to \(tab.label)"),
                    keywords: [tab.rawValue, "tab", "vista"]
                ) {
                    store.tab = tab
                }
            )
        }

        // Two vaults may share a name (`VaultNaming`): the owner tells them
        // apart, and is a keyword too.
        for vault in store.vaults where vault.id != store.currentVault?.id {
            actions.append(
                PaletteAction(
                    id: "vault.\(vault.id)",
                    title: String(localized: "Vault: \(VaultNaming.label(for: vault, among: store.vaults))"),
                    keywords: [vault.name, vault.owner]
                ) {
                    await store.select(vault)
                }
            )
        }

        // The vault's own life cycle, through the Vault menu's notifications
        // so the sheets are seeded in one place (`MainWindow`).
        actions.append(
            PaletteAction(
                id: "vault.new",
                title: String(localized: "New Vault\u{2026}"),
                keywords: ["vault", "nuovo", "create"]
            ) {
                NotificationCenter.default.post(name: .newVault, object: nil)
            }
        )
        // The Vault menu's rules (`VaultPermissions`): no vault on screen,
        // none of these.
        let permissions = VaultPermissions(vault: store.currentVault, engine: engine)
        if permissions.mayRename {
            actions.append(
                PaletteAction(
                    id: "vault.rename",
                    title: String(localized: "Rename Vault\u{2026}"),
                    keywords: ["vault", "rinomina", "name"]
                ) {
                    NotificationCenter.default.post(name: .renameVault, object: nil)
                }
            )
        }
        if permissions.mayLeave {
            actions.append(
                PaletteAction(
                    id: "vault.leave",
                    title: String(localized: "Leave Vault\u{2026}"),
                    keywords: ["vault", "esci", "abbandona", "leave"]
                ) {
                    NotificationCenter.default.post(name: .leaveVault, object: nil)
                }
            )
        }
        if permissions.mayDelete {
            actions.append(
                PaletteAction(
                    id: "vault.delete",
                    title: String(localized: "Delete Vault\u{2026}"),
                    keywords: ["vault", "elimina", "cancella", "remove"]
                ) {
                    NotificationCenter.default.post(name: .deleteVault, object: nil)
                }
            )
        }

        actions.append(contentsOf: [
            PaletteAction(
                id: "open.setup",
                title: String(localized: "Wallets, Envelopes & Categories\u{2026}"),
                keywords: ["setup", "wallet", "flow", "buste", "categorie"]
            ) {
                NotificationCenter.default.post(name: .openSetup, object: nil)
            },
            PaletteAction(
                id: "open.management",
                title: String(localized: "Manage\u{2026}"),
                keywords: ["vault", "wallet", "gestione", "sharing"]
            ) {
                NotificationCenter.default.post(name: .openManagement, object: nil)
            },
        ])

        if store.canWrite {
            actions.append(
                PaletteAction(
                    id: "recurring.new",
                    title: String(localized: "New Recurring\u{2026}"),
                    keywords: ["recurring", "ricorrenza", "ricorrente", "abbonamento", "subscription"]
                ) {
                    NotificationCenter.default.post(name: .newRecurring, object: nil)
                }
            )
        }

        actions.append(contentsOf: [
            PaletteAction(
                id: "export.csv",
                title: String(localized: "Export CSV\u{2026}"),
                keywords: ["csv", "export", "esporta"]
            ) {
                NotificationCenter.default.post(name: .exportCSV, object: nil)
            },
            PaletteAction(
                id: "export.all",
                title: String(localized: "Export All Transactions\u{2026}"),
                keywords: ["csv", "export", "esporta", "tutto"]
            ) {
                NotificationCenter.default.post(name: .exportAllTransactions, object: nil)
            },
            PaletteAction(
                id: "import.statement",
                title: String(localized: "Import Statement\u{2026}"),
                keywords: ["import", "importa", "estratto", "csv", "banca"]
            ) {
                NotificationCenter.default.post(name: .importStatement, object: nil)
            },
            PaletteAction(
                id: "backup.database",
                title: String(localized: "Back Up Database\u{2026}"),
                keywords: ["backup", "copia", "salva"]
            ) {
                NotificationCenter.default.post(name: .backupDatabase, object: nil)
            },
        ])

        if let engine {
            actions.append(
                PaletteAction(
                    id: "sync.now",
                    title: String(localized: "Sync Now"),
                    keywords: ["sync", "sincronizza", "server"]
                ) {
                    await engine.syncNow()
                }
            )
        }

        actions.append(contentsOf: [
            PaletteAction(
                id: "toggle.voided",
                title: store.showVoided
                    ? String(localized: "Hide Deleted")
                    : String(localized: "Show Deleted"),
                keywords: ["deleted", "eliminate"]
            ) {
                store.showVoided.toggle()
            },
            PaletteAction(
                id: "toggle.transfers",
                title: store.showTransfers
                    ? String(localized: "Hide Transfers")
                    : String(localized: "Show Transfers"),
                keywords: ["transfers", "trasferimenti"]
            ) {
                store.showTransfers.toggle()
            },
            PaletteAction(
                id: "toggle.wallet",
                title: store.showWalletColumn
                    ? String(localized: "Hide Wallet Column")
                    : String(localized: "Show Wallet Column"),
                keywords: ["wallet", "column", "colonna"]
            ) {
                store.showWalletColumn.toggle()
            },
        ])

        return actions
    }
}

// MARK: - The list under the field

/// The rows of the ⌘K panel while its line starts with `>`
/// (`QuickAddOverlay`): a list of choices inside the same panel, not a second
/// dialog. The active row is tinted with the accent rather than filled with
/// it, as the completion list's is; the panel's footer carries the keys.
struct CommandPaletteList: View {
    let model: CommandPaletteModel
    /// Called after an entry ran, so the field can close itself.
    let onRun: () -> Void

    var body: some View {
        let results = model.results
        Group {
            if results.isEmpty {
                Text(String(localized: "No matching command"))
                    .font(Face.row)
                    .foregroundStyle(Ink.text3)
                    .padding(.horizontal, 10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .frame(height: Metrics.rowHeight)
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(results.enumerated()), id: \.element.id) { index, action in
                            row(index: index, action: action)
                        }
                    }
                }
                .frame(maxHeight: Metrics.rowHeight * 10)
                .scrollBounceBehavior(.basedOnSize)
            }
        }
        .padding(6)
    }

    private func row(index: Int, action: PaletteAction) -> some View {
        let active = index == model.selection
        return Text(action.title)
            .font(Face.row)
            .foregroundStyle(Ink.text)
            .lineLimit(1)
            .truncationMode(.tail)
            .padding(.horizontal, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .frame(height: Metrics.rowHeight)
            .background(active ? Ink.accent.opacity(0.14) : Color.clear, in: RoundedRectangle(cornerRadius: 5))
            .contentShape(Rectangle())
            .onHover { inside in
                if inside { model.select(index) }
            }
            .onTapGesture { activate(index) }
            .accessibilityAddTraits(active ? [.isButton, .isSelected] : .isButton)
            // A tap gesture is not an action VoiceOver can press: the row
            // says it is a button, so it has to act like one.
            .accessibilityAction { activate(index) }
    }

    /// A click on a row, or VoiceOver pressing it: the entry runs as ↩ would
    /// run it with the row highlighted.
    private func activate(_ index: Int) {
        model.select(index)
        Task { if await model.run() { onRun() } }
    }
}
