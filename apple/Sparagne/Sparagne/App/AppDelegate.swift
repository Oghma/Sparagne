import AppKit

/// Application-level hooks SwiftUI's `App` does not offer. The store is
/// handed over by `SparagneApp` once `ContentView` has opened it.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    /// `nil` until the database is open.
    var store: AppStore?
}
