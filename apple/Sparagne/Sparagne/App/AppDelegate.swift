import AppKit

/// Application-level hooks SwiftUI's `App` does not offer. The store is
/// handed over by `SparagneApp` once `ContentView` has opened it.
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
    ///
    /// The Mastro's cell being typed into is left as a click outside the
    /// grid leaves it: saved, and the new line registered if it has an
    /// amount (`LedgerEditor.finish`). The quit waits for those writes too.
    /// One refused is lost with the app, as it would be on any other way out.
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let store else { return .terminateNow }
        store.ledgerEditor.finish()
        guard store.pendingUndo != nil || store.ledgerEditor.hasPendingWrites else { return .terminateNow }
        Task {
            await store.ledgerEditor.settle()
            await store.flushPendingUndo()
            replyToTermination(true)
        }
        return .terminateLater
    }
}
