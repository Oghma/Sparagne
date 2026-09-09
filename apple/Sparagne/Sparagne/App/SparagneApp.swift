import SwiftUI

@main
struct SparagneApp: App {
    // Shared UserDefaults key with the "Voided" filter toggle in
    // DetailView's filter bar, so the menu item and the toolbar toggle stay
    // in sync without extra plumbing.
    @AppStorage("showVoided") private var showVoided = false
    @Environment(\.openWindow) private var openWindow

    /// Opened once, in `ContentView`, and shared with the Categories window
    /// so both windows see the same vault and the same in-process core
    /// (team-lead task 3).
    @State private var store: AppStore?
    @State private var launchFailure: String?

    var body: some Scene {
        WindowGroup {
            ContentView(store: $store, launchFailure: $launchFailure)
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
            CommandGroup(after: .newItem) {
                Button(String(localized: "Categories…")) { openWindow(id: "categories") }
                    .keyboardShortcut("c", modifiers: [.command, .shift])
            }
        }

        Window(String(localized: "Categories"), id: "categories") {
            Group {
                if let store {
                    CategoriesWindowView(store: store)
                } else {
                    ContentUnavailableView(
                        String(localized: "No vault yet"),
                        systemImage: "tag"
                    )
                }
            }
        }
        .defaultSize(width: 560, height: 420)

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
            Text(String(localized: "Sparagne stores everything locally, in Application Support."))
                .foregroundStyle(.secondary)
        }
        .padding()
        .frame(width: 360, height: 140)
    }
}
