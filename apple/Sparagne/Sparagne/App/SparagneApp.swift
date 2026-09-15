import SwiftUI

@main
struct SparagneApp: App {
    // Shared UserDefaults keys with the store's own filters, so the menu
    // items and the ledger stay in sync without extra plumbing.
    @AppStorage("showVoided") private var showVoided = false
    @AppStorage("showTransfers") private var showTransfers = false
    /// The optional WALLET column of the grid (`docs/v2/UI.md` §3): off by
    /// default, remembered here.
    @AppStorage("showWalletColumn") private var showWalletColumn = false

    /// Opened once, in `ContentView`; the Settings scene shares the engine
    /// built with it.
    @State private var store: AppStore?
    /// Created with the store, and shared with the Settings scene so the
    /// account lives in one place.
    @State private var engine: SyncEngine?
    @State private var launchFailure: String?

    var body: some Scene {
        WindowGroup {
            ContentView(store: $store, engine: $engine, launchFailure: $launchFailure)
        }
        .commands {
            CommandGroup(replacing: .newItem) {
                Button(String(localized: "Quick Add\u{2026}")) {
                    NotificationCenter.default.post(name: .focusQuickAdd, object: nil)
                }
                .keyboardShortcut("k", modifiers: .command)

                Button(String(localized: "Duplicate Last Row")) {
                    NotificationCenter.default.post(name: .duplicateLastRow, object: nil)
                }
                .keyboardShortcut("d", modifiers: .command)
            }

            // The standard View menu, beside "Show Toolbar": the column is a
            // property of the view, not of the ledger's filters.
            CommandGroup(after: .toolbar) {
                Toggle(String(localized: "Show Wallet Column"), isOn: $showWalletColumn)
                    .keyboardShortcut("w", modifiers: [.command, .shift])
            }

            CommandGroup(after: .importExport) {
                Button(String(localized: "Export CSV\u{2026}")) {
                    NotificationCenter.default.post(name: .exportCSV, object: nil)
                }
                .keyboardShortcut("e", modifiers: .command)
            }

            CommandMenu(String(localized: "Ledger")) {
                Button(String(localized: "Previous Month")) {
                    NotificationCenter.default.post(name: .stepMonth, object: -1)
                }
                .keyboardShortcut(.leftArrow, modifiers: [.command, .option])

                Button(String(localized: "Next Month")) {
                    NotificationCenter.default.post(name: .stepMonth, object: 1)
                }
                .keyboardShortcut(.rightArrow, modifiers: [.command, .option])

                Divider()

                Button(String(localized: "Find\u{2026}")) {
                    NotificationCenter.default.post(name: .focusSearch, object: nil)
                }
                .keyboardShortcut("f", modifiers: .command)

                Divider()

                Toggle(String(localized: "Show Voided"), isOn: $showVoided)
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
                .disabled(store?.currentVault == nil)
                // Greyed out for a shared vault I do not own: the core and
                // the server would both refuse the command anyway.
                Button(String(localized: "Delete Vault\u{2026}")) {
                    NotificationCenter.default.post(name: .deleteVault, object: nil)
                }
                .disabled(!mayDeleteCurrentVault)

                Divider()

                Button(String(localized: "Manage\u{2026}")) {
                    NotificationCenter.default.post(name: .openManagement, object: nil)
                }
                .keyboardShortcut("m", modifiers: [.command, .shift])

                Button(String(localized: "Envelopes & Categories\u{2026}")) {
                    NotificationCenter.default.post(name: .openSetup, object: nil)
                }
                .keyboardShortcut("c", modifiers: [.command, .shift])
            }
        }

        Settings {
            SettingsView(engine: engine)
                .preferredColorScheme(.dark)
        }
    }

    /// A vault on screen that is mine, or not on the server at all
    /// (`SyncEngine.mayDeleteVault`).
    private var mayDeleteCurrentVault: Bool {
        guard let vault = store?.currentVault else { return false }
        return engine?.mayDeleteVault(vault.id) ?? true
    }
}

/// Menu commands reach the window through notifications rather than shared
/// state: the menu bar is a scene, the ledger is a view, and the only thing
/// they need to agree on is the name of the action.
extension Notification.Name {
    /// ⌘K: opens the quick-add line over the grid.
    static let focusQuickAdd = Notification.Name("it.oghma.sparagne.focusQuickAdd")
    /// ⌘F: focuses the search field of the ledger header.
    static let focusSearch = Notification.Name("it.oghma.sparagne.focusSearch")
    /// ⌘D: copies the last row of the month into the empty line.
    static let duplicateLastRow = Notification.Name("it.oghma.sparagne.duplicateLastRow")
    /// ⌥←/⌥→: steps the month; the object is the number of months, signed.
    static let stepMonth = Notification.Name("it.oghma.sparagne.stepMonth")
    /// ⌘⇧M: opens the management sheet.
    static let openManagement = Notification.Name("it.oghma.sparagne.openManagement")
    /// ⌘E: exports the rows on screen as CSV (`docs/v2/UI.md` §6).
    static let exportCSV = Notification.Name("it.oghma.sparagne.exportCSV")
    /// ⌘⇧C: switches to the setup tab, envelopes and categories (§2.3).
    static let openSetup = Notification.Name("it.oghma.sparagne.openSetup")
    /// Vault menu and palette: the onboarding sheet again, for another vault.
    static let newVault = Notification.Name("it.oghma.sparagne.newVault")
    /// Vault menu and palette: rename the vault on screen.
    static let renameVault = Notification.Name("it.oghma.sparagne.renameVault")
    /// Vault menu and palette: the confirmation that deletes the vault on
    /// screen.
    static let deleteVault = Notification.Name("it.oghma.sparagne.deleteVault")
}

private struct SettingsView: View {
    let engine: SyncEngine?

    var body: some View {
        if let engine {
            AccountSettingsView(engine: engine)
        } else {
            Form {
                Text(String(localized: "Sparagne stores everything locally, in Application Support."))
                    .foregroundStyle(.secondary)
            }
            .padding()
            .frame(width: 360, height: 140)
        }
    }
}
