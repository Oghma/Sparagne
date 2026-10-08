import SwiftUI

@main
struct SparagneApp: App {
    // Shared UserDefaults keys with the store's own filters, so the menu
    // items and the ledger stay in sync without extra plumbing.
    @AppStorage("showVoided") private var showVoided = false
    @AppStorage("showTransfers") private var showTransfers = false
    /// The optional WALLET column of the grid: off by
    /// default, remembered here.
    @AppStorage("showWalletColumn") private var showWalletColumn = false

    /// Opened once, in `ContentView`; the Settings scene shares the engine
    /// built with it.
    @State private var store: AppStore?
    /// Created with the store, and shared with the Settings scene so the
    /// account lives in one place.
    @State private var engine: SyncEngine?
    @State private var launchFailure: String?
    /// Quitting waits for the pending void to be applied (`AppDelegate`).
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    var body: some Scene {
        WindowGroup {
            ContentView(store: $store, engine: $engine, launchFailure: $launchFailure)
                .onChange(of: store.map(ObjectIdentifier.init)) {
                    appDelegate.store = store
                }
        }
        // No system title bar and no toolbar: the window draws its own top
        // bar (`TopBar`), and `WindowChrome` moves the traffic lights into it.
        .windowStyle(.hiddenTitleBar)
        .commands {
            CommandGroup(replacing: .newItem) {
                // ⌘K is the only way into the command palette, so a vault
                // that is only read keeps it, under the name of what the
                // panel then is (`QuickAddOverlay.commandsOnly`).
                Button(
                    store?.canWrite == false
                        ? String(localized: "Command Palette\u{2026}")
                        : String(localized: "Quick Add\u{2026}")
                ) {
                    NotificationCenter.default.post(name: .focusQuickAdd, object: nil)
                }
                .keyboardShortcut("k", modifiers: .command)

                Button(String(localized: "Duplicate Last Row")) {
                    NotificationCenter.default.post(name: .duplicateLastRow, object: nil)
                }
                .keyboardShortcut("d", modifiers: .command)
                .disabled(store?.canWrite == false)
            }

            // The standard View menu: the column is a property of the view,
            // not of the ledger's filters.
            CommandGroup(after: .toolbar) {
                Toggle(String(localized: "Show Wallet Column"), isOn: $showWalletColumn)
                    .keyboardShortcut("w", modifiers: [.command, .shift])

                Divider()

                // The sheet tabs, ⌘1 to ⌘4 in the tab bar's order, as a
                // browser's tabs.
                ForEach(LedgerTab.allCases) { tab in
                    Button(tab.label) { store?.tab = tab }
                    .keyboardShortcut(KeyEquivalent(Character(String(tab.shortcut))), modifiers: .command)
                }
            }

            CommandGroup(after: .importExport) {
                Button(String(localized: "Import Statement\u{2026}")) {
                    NotificationCenter.default.post(name: .importStatement, object: nil)
                }
                .keyboardShortcut("i", modifiers: [.command, .shift])
                .disabled(store?.currentVault == nil || store?.isReadOnly == true)

                Button(String(localized: "Export CSV\u{2026}")) {
                    NotificationCenter.default.post(name: .exportCSV, object: nil)
                }
                .keyboardShortcut("e", modifiers: .command)

                Button(String(localized: "Export All Transactions\u{2026}")) {
                    NotificationCenter.default.post(name: .exportAllTransactions, object: nil)
                }
                .disabled(store?.currentVault == nil)

                Divider()

                Button(String(localized: "Back Up Database\u{2026}")) {
                    NotificationCenter.default.post(name: .backupDatabase, object: nil)
                }
                .disabled(store == nil)
            }

            CommandMenu(String(localized: "Ledger")) {
                Button(String(localized: "Previous Month")) { store?.stepMonth(by: -1) }
                .keyboardShortcut(.leftArrow, modifiers: [.command, .option])

                Button(String(localized: "Next Month")) { store?.stepMonth(by: 1) }
                .keyboardShortcut(.rightArrow, modifiers: [.command, .option])

                Divider()

                Button(String(localized: "Find\u{2026}")) {
                    NotificationCenter.default.post(name: .focusSearch, object: nil)
                }
                .keyboardShortcut("f", modifiers: .command)

                Divider()

                Toggle(String(localized: "Show Deleted"), isOn: $showVoided)
                    .keyboardShortcut("v", modifiers: [.command, .shift])
                Toggle(String(localized: "Show Transfers"), isOn: $showTransfers)
                    .keyboardShortcut("t", modifiers: [.command, .shift])
            }

            CommandMenu(String(localized: "Vault")) {
                Button(String(localized: "New Vault\u{2026}")) {
                    NotificationCenter.default.post(name: .newVault, object: nil)
                }
                Button(String(localized: "Rename Vault\u{2026}")) {
                    NotificationCenter.default.post(name: .renameVault, object: nil)
                }
                .disabled(!permissions.mayRename)
                // Greyed out for a shared vault I do not own: the core and
                // the server would both refuse the command anyway.
                Button(String(localized: "Delete Vault\u{2026}")) {
                    NotificationCenter.default.post(name: .deleteVault, object: nil)
                }
                .disabled(!permissions.mayDelete)
                // A member's way out of a shared vault; the owner deletes it.
                Button(String(localized: "Leave Vault\u{2026}")) {
                    NotificationCenter.default.post(name: .leaveVault, object: nil)
                }
                .disabled(!permissions.mayLeave)

                Divider()

                Button(String(localized: "Manage\u{2026}")) {
                    NotificationCenter.default.post(name: .openManagement, object: nil)
                }
                .keyboardShortcut("m", modifiers: [.command, .shift])

                Button(String(localized: "Wallets, Envelopes & Categories\u{2026}")) { store?.tab = .setup }
                .keyboardShortcut("c", modifiers: [.command, .shift])

                Button(String(localized: "New Recurring\u{2026}")) { store?.requestNewRecurring() }
                .disabled(store?.canWrite != true)
            }
        }

        Settings {
            SettingsView(engine: engine)
                .preferredColorScheme(.dark)
        }
    }

    /// The same rules as the top bar's vault selector and the palette
    /// (`VaultPermissions`).
    private var permissions: VaultPermissions {
        VaultPermissions(vault: store?.currentVault, engine: engine)
    }
}

/// Menu commands that act on state of the window's own views (a focus, a
/// sheet, a file panel) reach them through notifications: the menu bar is a
/// scene, the ledger is a view, and the only thing they need to agree on is
/// the name of the action. A command that only changes the store (the tab,
/// the month, Nuova ricorrenza…) calls it directly.
extension Notification.Name {
    /// ⌘K: opens the quick-add line over the grid.
    static let focusQuickAdd = Notification.Name("it.oghma.sparagne.focusQuickAdd")
    /// ⌘F: focuses the search field of the ledger header.
    static let focusSearch = Notification.Name("it.oghma.sparagne.focusSearch")
    /// ⌘D: copies the last row of the month into the empty line.
    static let duplicateLastRow = Notification.Name("it.oghma.sparagne.duplicateLastRow")
    /// ⌘⇧M: opens the management sheet.
    static let openManagement = Notification.Name("it.oghma.sparagne.openManagement")
    /// ⌘E: exports the rows on screen as CSV.
    static let exportCSV = Notification.Name("it.oghma.sparagne.exportCSV")
    /// Vault menu and palette: the onboarding sheet again, for another vault.
    static let newVault = Notification.Name("it.oghma.sparagne.newVault")
    /// Vault menu and palette: rename the vault on screen.
    static let renameVault = Notification.Name("it.oghma.sparagne.renameVault")
    /// Vault menu and palette: the confirmation that deletes the vault on
    /// screen.
    static let deleteVault = Notification.Name("it.oghma.sparagne.deleteVault")
    /// Vault menu and palette: a member leaves the shared vault on screen.
    static let leaveVault = Notification.Name("it.oghma.sparagne.leaveVault")
    /// ⌘⇧I: imports a bank or card statement into the vault on screen.
    static let importStatement = Notification.Name("it.oghma.sparagne.importStatement")
    /// File menu: every transaction of the vault on screen as CSV.
    static let exportAllTransactions = Notification.Name("it.oghma.sparagne.exportAllTransactions")
    /// File menu: a copy of the whole database file.
    static let backupDatabase = Notification.Name("it.oghma.sparagne.backupDatabase")
}

/// The Settings window: the account when the app has a sync engine, a line
/// about where the data lives when it has none (a demo database). Painted on
/// the window's sheet ground, title bar included, like the main window.
private struct SettingsView: View {
    let engine: SyncEngine?

    var body: some View {
        Group {
            if let engine {
                AccountSettingsView(engine: engine)
            } else {
                FormGroup(String(localized: "Status")) {
                    Text(String(localized: "Sparagne stores everything locally, in Application Support."))
                        .font(Face.ui(13))
                        .foregroundStyle(Ink.text2)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(20)
                .frame(width: 460, alignment: .topLeading)
                .environment(\.formMetrics, .sheet)
            }
        }
        .background(Ink.sheet)
        .containerBackground(Ink.sheet, for: .window)
    }
}
