import Testing

@testable import Sparagne

/// The top bar's sync pill (`Model/SyncPillState.swift`): which state wins,
/// what it says and what its popover offers, from the engine's fields alone.
struct SyncPillStateTests {
    /// A logged-in, idle, empty engine unless a test says otherwise.
    private static func state(
        rejected: Int = 0,
        isLoggedIn: Bool = true,
        sessionExpired: Bool = false,
        status: SyncEngine.Status = .idle,
        pendingCount: Int = 0
    ) -> SyncPillState {
        SyncPillState.resolve(
            rejected: rejected,
            isLoggedIn: isLoggedIn,
            sessionExpired: sessionExpired,
            status: status,
            pendingCount: pendingCount
        )
    }

    // MARK: - Each state

    @Test("Logged in, idle and nothing waiting is synced")
    func synced() {
        #expect(Self.state() == .synced)
        #expect(Self.state().tone == .positive)
    }

    @Test("A running round is syncing")
    func syncing() {
        #expect(Self.state(status: .syncing) == .syncing)
    }

    @Test("Changes the server has not confirmed are pending, with their count")
    func pending() {
        #expect(Self.state(pendingCount: 3) == .pending(3))
    }

    @Test("An unreachable server is offline")
    func offline() {
        #expect(Self.state(status: .offline) == .offline)
        #expect(Self.state(status: .offline).tone == .muted)
    }

    @Test("A failed round is an error, carrying the engine's message")
    func error() {
        let state = Self.state(status: .error("boom"))
        #expect(state == .error("boom"))
        #expect(state.tone == .negative)
        #expect(state.detail == "boom")
        #expect(state.settingsText == "boom")
    }

    @Test("Refused changes are counted, in the warning tone")
    func refused() {
        let state = Self.state(rejected: 2)
        #expect(state == .refused(2))
        #expect(state.tone == .warning)
        #expect(state.actions.contains(.review))
    }

    @Test("Nobody logged in is local only, and offers to connect a server")
    func loggedOut() {
        let state = Self.state(isLoggedIn: false)
        #expect(state == .loggedOut)
        #expect(state.actions == [.connect])
        #expect(!state.showsAccount)
    }

    @Test("A session the server ended asks to log in again")
    func sessionExpired() {
        let state = Self.state(isLoggedIn: false, sessionExpired: true)
        #expect(state == .sessionExpired)
        #expect(state.tone == .warning)
        #expect(state.actions == [.settings])
    }

    @Test("No engine is the demo database: local only, nothing to do")
    func demo() {
        let state = SyncPillState(engine: nil)
        #expect(state == .demo)
        #expect(state.title == SyncPillState.loggedOut.title)
        #expect(state.detail != nil)
        #expect(state.actions.isEmpty)
        #expect(!state.showsAccount)
    }

    // MARK: - Precedence

    @Test("Refused beats every other state")
    func refusedWins() {
        #expect(Self.state(rejected: 1, isLoggedIn: false, sessionExpired: true) == .refused(1))
        #expect(Self.state(rejected: 1, status: .error("x"), pendingCount: 4) == .refused(1))
        #expect(Self.state(rejected: 1, status: .syncing) == .refused(1))
    }

    @Test("Session expired beats logged out, and anything the status says")
    func sessionExpiredWins() {
        #expect(Self.state(isLoggedIn: false, sessionExpired: true, status: .error("x")) == .sessionExpired)
        #expect(Self.state(isLoggedIn: false, sessionExpired: true, pendingCount: 2) == .sessionExpired)
    }

    @Test("Logged out beats the status and the pending count")
    func loggedOutWins() {
        #expect(Self.state(isLoggedIn: false, status: .error("x")) == .loggedOut)
        #expect(Self.state(isLoggedIn: false, status: .offline, pendingCount: 2) == .loggedOut)
    }

    @Test("Then error, offline, syncing and pending, in that order")
    func statusOrder() {
        #expect(Self.state(status: .error("x"), pendingCount: 2) == .error("x"))
        #expect(Self.state(status: .offline, pendingCount: 2) == .offline)
        #expect(Self.state(status: .syncing, pendingCount: 2) == .syncing)
        #expect(Self.state(status: .idle, pendingCount: 2) == .pending(2))
    }

    // MARK: - Wording

    @Test("Every state has a title and a headline")
    func wording() {
        let states: [SyncPillState] = [
            .synced, .syncing, .pending(2), .offline, .error("x"),
            .refused(1), .loggedOut, .sessionExpired, .demo,
        ]
        for state in states {
            #expect(!state.title.isEmpty)
            #expect(!state.headline.isEmpty)
        }
        // The Settings window's words are the pill's.
        #expect(SyncPillState.synced.settingsText == SyncPillState.synced.title)
    }

    @Test("Logged in, the popover can sync now and reach the account")
    func loggedInActions() {
        for state in [SyncPillState.synced, .syncing, .pending(1), .offline, .error("x")] {
            #expect(state.actions == [.settings, .syncNow])
            #expect(state.showsAccount)
        }
    }
}
