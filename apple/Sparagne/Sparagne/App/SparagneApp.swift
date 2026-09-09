import SwiftUI

@main
struct SparagneApp: App {
    // Shared UserDefaults key with the "Voided" filter toggle in
    // DetailView's filter bar, so the menu item and the toolbar toggle stay
    // in sync without extra plumbing.
    @AppStorage("showVoided") private var showVoided = false

    var body: some Scene {
        WindowGroup {
            ContentView()
        }
        .commands {
            CommandMenu(String(localized: "Transaction")) {
                Button(String(localized: "Quick Add")) {
                    NotificationCenter.default.post(name: .focusQuickAdd, object: nil)
                }
                .keyboardShortcut("n", modifiers: .command)

                Divider()

                Toggle(String(localized: "Show Voided"), isOn: $showVoided)
                    .keyboardShortcut("v", modifiers: [.command, .shift])
            }
        }

        Settings {
            SettingsView()
        }
    }
}

extension Notification.Name {
    /// Posted by the "Quick Add" (⌘N) menu command; ContentView focuses the
    /// quick-add field in response instead of the app owning UI state.
    static let focusQuickAdd = Notification.Name("it.oghma.sparagne.focusQuickAdd")
}

private struct SettingsView: View {
    var body: some View {
        Form {
            Text(String(localized: "Sparagne settings will live here once the core is wired up."))
                .foregroundStyle(.secondary)
        }
        .padding()
        .frame(width: 360, height: 140)
    }
}
