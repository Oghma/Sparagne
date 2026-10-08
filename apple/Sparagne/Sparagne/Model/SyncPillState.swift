import Foundation

/// Where the sync stands, as the top bar's pill and its popover say it
/// (`docs/v2/SYNC.md` §5, `docs/v2/UI.md` §2.4), and as the Settings window
/// words it: one place for the wording.
///
/// Pure: built from the few `SyncEngine` fields it reads, so every state and
/// the order they win in can be tested without an engine or a window
/// (`SparagneTests/SyncPillStateTests.swift`).
enum SyncPillState: Equatable, Sendable {
    /// Logged in, nothing waiting, the last round went through.
    case synced
    /// A round is running.
    case syncing
    /// Changes applied here that the server has not confirmed yet.
    case pending(Int)
    /// The server could not be reached; changes wait here.
    case offline
    /// The last round failed, with the engine's message.
    case error(String)
    /// The server refused changes the core is still holding: they need a
    /// decision, so this wins over everything else.
    case refused(Int)
    /// Nobody is logged in: the vaults live on this Mac only.
    case loggedOut
    /// The server ended the session; logging in again resumes the sync.
    case sessionExpired
    /// No engine at all: a demo database (`LaunchOptions.database`), which
    /// never syncs.
    case demo

    /// The order the states win in: refused, session expired, logged out,
    /// error, offline, syncing, pending, synced.
    static func resolve(
        rejected: Int,
        isLoggedIn: Bool,
        sessionExpired: Bool,
        status: SyncEngine.Status,
        pendingCount: Int
    ) -> SyncPillState {
        if rejected > 0 { return .refused(rejected) }
        if sessionExpired { return .sessionExpired }
        if !isLoggedIn { return .loggedOut }
        switch status {
        case .error(let message): return .error(message)
        case .offline: return .offline
        case .syncing: return .syncing
        case .idle: break
        }
        return pendingCount > 0 ? .pending(pendingCount) : .synced
    }

    /// The engine's state, or `.demo` without one.
    init(engine: SyncEngine?) {
        guard let engine else {
            self = .demo
            return
        }
        self = Self.resolve(
            rejected: engine.rejected.count,
            isLoggedIn: engine.isLoggedIn,
            sessionExpired: engine.account.sessionExpired,
            status: engine.status,
            pendingCount: engine.pendingCount
        )
    }

    // MARK: Wording

    /// The pill's text.
    var title: String {
        switch self {
        case .synced: String(localized: "Synced")
        case .syncing: String(localized: "Syncing…")
        case .pending(let count): String(localized: "\(count) pending")
        case .offline: String(localized: "Offline")
        case .error: String(localized: "Sync error")
        case .refused(let count): String(localized: "\(count) refused")
        case .loggedOut, .demo: String(localized: "Local only")
        case .sessionExpired: String(localized: "Session expired")
        }
    }

    /// The popover's first line, a little longer than the pill.
    var headline: String {
        switch self {
        case .synced: String(localized: "Everything is synced")
        case .syncing: String(localized: "Syncing with the server…")
        case .pending: String(localized: "Changes waiting to be sent")
        case .offline: String(localized: "The server can't be reached")
        case .error: String(localized: "The last sync failed")
        case .refused(let count): String(localized: "\(count) changes were refused")
        case .loggedOut: String(localized: "No server connected")
        case .sessionExpired: String(localized: "Session expired")
        case .demo: String(localized: "Demo database")
        }
    }

    /// What the popover says under the headline, when there is more to say.
    var detail: String? {
        switch self {
        case .synced, .syncing, .pending: nil
        case .offline: String(localized: "Changes wait on this Mac and go up when the server is back.")
        case .error(let message): message
        case .refused: String(localized: "The server did not accept them, so they are not in your balances.")
        case .loggedOut:
            String(localized: "The vault lives on this Mac only. Connect a server to have it on other devices or to share it.")
        case .sessionExpired: String(localized: "Log in again to resume syncing; your changes wait on this Mac.")
        case .demo: String(localized: "This database is for trying the app: it never syncs.")
        }
    }

    /// The Settings window's status line: the pill's words, or the engine's
    /// own message for a failed round.
    var settingsText: String {
        if case .error(let message) = self { return message }
        return title
    }

    // MARK: Look

    enum Tone: Equatable, Sendable {
        /// Green: all is well.
        case positive
        /// Accent: work in progress.
        case active
        /// Grey: nothing to sync with, or nothing reachable.
        case muted
        /// Red: a failure.
        case negative
        /// Amber-yellow: something needs the user.
        case warning
    }

    var tone: Tone {
        switch self {
        case .synced: .positive
        case .syncing, .pending: .active
        case .offline, .loggedOut, .demo: .muted
        case .error: .negative
        case .refused, .sessionExpired: .warning
        }
    }

    /// Whether the popover lists the server, the account and the counts:
    /// only with an account to describe.
    var showsAccount: Bool {
        switch self {
        case .synced, .syncing, .pending, .offline, .error, .refused: true
        case .loggedOut, .sessionExpired, .demo: false
        }
    }

    // MARK: Actions

    enum Action: Equatable, Sendable {
        /// `SyncEngine.syncNow`.
        case syncNow
        /// The Settings window, where the account and the server are.
        case settings
        /// The list of refused changes.
        case review
        /// The Settings window too, worded for a first connection.
        case connect
    }

    /// The popover's buttons, quiet ones first.
    var actions: [Action] {
        switch self {
        case .synced, .syncing, .pending, .offline, .error: [.settings, .syncNow]
        case .refused: [.settings, .review]
        case .loggedOut: [.connect]
        case .sessionExpired: [.settings]
        case .demo: []
        }
    }
}
