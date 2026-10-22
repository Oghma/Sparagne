import SwiftUI
import SparagneCore

/// One entry of the ⌘K command palette.
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
/// the entries it matches, the arrows on one of them.
@Observable
final class CommandPaletteModel {
    /// Rebuilt every time the palette opens: the vault list and the state of
    /// the two toggles change what the entries say.
    var actions: [PaletteAction] {
        didSet { list.show(results) }
    }

    /// What was typed after the `>`, already stripped of the marker.
    var query: String = "" {
        didSet { if query != oldValue { list.show(results) } }
    }

    /// `results`, the highlight back on the first whenever they change.
    let list = FloatingListModel<PaletteAction>()

    init(actions: [PaletteAction] = []) {
        self.actions = actions
        list.show(results)
    }

    /// The field is a palette when its first character is `>`. Quick-add uses
    /// `>` as its envelope marker, but never in first position: a line that
    /// starts with one has no amount and is not a transaction.
    static func isCommand(_ text: String) -> Bool { text.hasPrefix(">") }

    /// The text after the marker.
    static func query(in text: String) -> String {
        guard isCommand(text) else { return "" }
        return String(text.dropFirst()).trimmingCharacters(in: .whitespaces)
    }

    /// The entries `query` matches, best first, case and accents aside. Ties
    /// keep the order the actions were registered in, so an empty field
    /// always lists the same commands in the same places.
    static func filter(_ actions: [PaletteAction], query: String) -> [PaletteAction] {
        let needle = FloatingList.fold(query.trimmingCharacters(in: .whitespaces))
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
        let title = FloatingList.fold(action.title)
        if title.hasPrefix(needle) { return 0 }
        if title.split(whereSeparator: { !$0.isLetter && !$0.isNumber }).contains(where: { $0.hasPrefix(needle) }) {
            return 1
        }
        if title.contains(needle) { return 2 }
        if action.keywords.contains(where: { FloatingList.fold($0).hasPrefix(needle) }) { return 3 }
        if action.keywords.contains(where: { FloatingList.fold($0).contains(needle) }) { return 4 }
        return nil
    }

    var results: [PaletteAction] { Self.filter(actions, query: query) }

    /// ↩: runs the highlighted entry. `false` when nothing matches, so the
    /// field can stay open instead of closing over a typo.
    @discardableResult
    func run() async -> Bool {
        guard let action = list.selected else { return false }
        await action.run()
        return true
    }
}

// MARK: - The app's own actions

extension CommandPaletteModel {
    /// Everything the palette can do, in the order it lists them.
    ///
    /// The entries that have a menu item post that item's notification, so
    /// the two ways in share one implementation (`MainWindow` seeds the
    /// sheets); the rest write the same state the top bar and the tab bar
    /// write. The three view toggles write the store, which the window
    /// mirrors back into the menu's preference.
    static func ledgerActions(store: AppStore, engine: SyncEngine?) -> [PaletteAction] {
        func post(_ name: Notification.Name, _ id: String, _ title: String, _ keywords: [String]) -> PaletteAction {
            PaletteAction(id: id, title: title, keywords: keywords) {
                NotificationCenter.default.post(name: name, object: nil)
            }
        }

        var actions: [PaletteAction] = [
            PaletteAction(
                id: "month.previous",
                title: String(localized: "Previous Month"),
                keywords: ["month", "mese", "prev"]
            ) {
                store.stepMonth(by: -1)
            },
            PaletteAction(
                id: "month.next",
                title: String(localized: "Next Month"),
                keywords: ["month", "mese"]
            ) {
                store.stepMonth(by: 1)
            },
            PaletteAction(
                id: "month.today",
                title: String(localized: "Current Month"),
                keywords: ["today", "oggi", "month", "mese"]
            ) {
                store.showCurrentMonth()
            },
        ]

        // Every sheet of the tab bar, in its order (⌘1 to ⌘5 from the View
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

        actions.append(post(.newVault, "vault.new", String(localized: "New Vault\u{2026}"), ["vault", "nuovo", "create"]))
        // The Vault menu's rules (`VaultPermissions`): no vault on screen,
        // none of these.
        let permissions = VaultPermissions(vault: store.currentVault, engine: engine)
        if permissions.mayRename {
            let keywords = ["vault", "rinomina", "name"]
            actions.append(post(.renameVault, "vault.rename", String(localized: "Rename Vault\u{2026}"), keywords))
        }
        if permissions.mayLeave {
            let keywords = ["vault", "esci", "abbandona", "leave"]
            actions.append(post(.leaveVault, "vault.leave", String(localized: "Leave Vault\u{2026}"), keywords))
        }
        if permissions.mayDelete {
            let keywords = ["vault", "elimina", "cancella", "remove"]
            actions.append(post(.deleteVault, "vault.delete", String(localized: "Delete Vault\u{2026}"), keywords))
        }

        actions.append(contentsOf: [
            PaletteAction(
                id: "open.setup",
                title: String(localized: "Wallets, Envelopes & Categories\u{2026}"),
                keywords: ["setup", "wallet", "flow", "buste", "categorie"]
            ) {
                store.tab = .setup
            },
            post(
                .openManagement,
                "open.management",
                String(localized: "Manage\u{2026}"),
                ["vault", "wallet", "gestione", "sharing"]
            ),
        ])

        if store.canWrite {
            actions.append(
                PaletteAction(
                    id: "recurring.new",
                    title: String(localized: "New Recurring\u{2026}"),
                    keywords: ["recurring", "ricorrenza", "ricorrente", "abbonamento", "subscription"]
                ) {
                    store.requestNewRecurring()
                }
            )
            actions.append(post(
                .moveBetweenEnvelopes,
                "envelopes.move",
                String(localized: "Move Between Envelopes\u{2026}"),
                ["move", "transfer", "sposta", "trasferisci", "buste", "envelope", "riparto", "allocation"]
            ))
            actions.append(contentsOf: kindActions(store: store))
        }

        actions.append(contentsOf: [
            post(.exportCSV, "export.csv", String(localized: "Export CSV\u{2026}"), ["csv", "export", "esporta"]),
            post(
                .exportAllTransactions,
                "export.all",
                String(localized: "Export All Transactions\u{2026}"),
                ["csv", "export", "esporta", "tutto"]
            ),
            post(
                .importStatement,
                "import.statement",
                String(localized: "Import Statement\u{2026}"),
                ["import", "importa", "estratto", "csv", "banca"]
            ),
            post(
                .backupDatabase,
                "backup.database",
                String(localized: "Back Up Database\u{2026}"),
                ["backup", "copia", "salva"]
            ),
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

    /// "Cambia in entrata", "in uscita", "in rimborso", on what the Mastro
    /// has in hand: the rows picked, or with none the row the cursor is on.
    /// None on another tab, where neither is on screen.
    private static func kindActions(store: AppStore) -> [PaletteAction] {
        guard store.tab == .ledger else { return [] }
        let picked = store.selectedRows.map(\.id)
        if !picked.isEmpty {
            return kindActions(targets: store.kindTargets(rows: picked)) { kind in
                await store.changeKind(rows: picked, to: kind)
            }
        }
        guard let rowId = store.ledgerEditor.cursor?.row else { return [] }
        return kindActions(targets: store.kindTargets(of: rowId)) { kind in
            await ChangeKindMenu.change(rowId, to: kind, in: store)
        }
    }

    /// An entry for each kind of `targets`, the kinds the rows can become, in
    /// their order; `change` turns the rows into the one run. No targets, no
    /// entries: a transfer, a deleted row or an opening has nothing to offer.
    static func kindActions(
        targets: [TransactionKind],
        change: @escaping @MainActor (TransactionKind) async -> Void
    ) -> [PaletteAction] {
        targets.compactMap { kind in
            let keywords: [String] = switch kind {
            case .income: ["income", "entrata", "kind", "tipo"]
            case .expense: ["expense", "spesa", "uscita", "kind", "tipo"]
            case .refund: ["refund", "rimborso", "kind", "tipo"]
            case .transferWallet, .transferFlow: []
            }
            guard !keywords.isEmpty else { return nil }
            return PaletteAction(id: "kind.\(kind)", title: kindTitle(kind), keywords: keywords) {
                await change(kind)
            }
        }
    }

    /// "Cambia in entrata": the entry for `kind`, and the row's VoiceOver
    /// action of the same name.
    static func kindTitle(_ kind: TransactionKind) -> String {
        switch kind {
        case .income: String(localized: "Change to Income")
        case .expense: String(localized: "Change to Expense")
        case .refund: String(localized: "Change to Refund")
        case .transferWallet, .transferFlow: QuickAddTokens.kindName(kind)
        }
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
        let results = model.list.items
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
        let active = index == model.list.selection
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
                if inside { model.list.highlight(index) }
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
        model.list.highlight(index)
        Task { if await model.run() { onRun() } }
    }
}
