import AppKit

/// Application-level hooks SwiftUI's `App` does not offer. The store is
/// handed over by `SparagneApp` once `ContentView` has opened it.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    /// `nil` until the database is open.
    var store: AppStore?

    /// How a deferred answer to "may I quit?" reaches AppKit. The tests swap
    /// it, since replying to a termination nobody asked for would end the
    /// test host.
    var replyToTermination: (Bool) -> Void = { NSApp.reply(toApplicationShouldTerminate: $0) }

    /// A void on the undo toast is only a promise until its window elapses:
    /// quitting before that would leave the rows live on the next launch,
    /// after the user saw them go. So the quit waits for the void to be
    /// written, then goes ahead whatever the core answered.
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let store, store.pendingUndo != nil else { return .terminateNow }
        Task {
            await store.flushPendingUndo()
            replyToTermination(true)
        }
        return .terminateLater
    }
}
