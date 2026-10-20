import Foundation
import Testing

/// What `CoreActor`'s probe waits on in the tests: a gate every call into
/// the core passes, so a test can count the calls, or hold one in flight and
/// look at the window while it is parked.
///
/// Left open it only counts, which is how a test shows that a refused write
/// never reached the core. `close` parks every call until `open`; `arm`
/// parks only the next one, until `release`.
actor TestGate {
    /// How many calls have reached the gate, ever.
    private(set) var calls = 0
    /// Whether any of them arrived on the main thread.
    private(set) var sawMainThread = false

    private var closed = false
    private var armed = false
    private var parked: [CheckedContinuation<Void, Never>] = []
    /// Waiting for the next call to arrive (`untilCalls`).
    private var arrivalWatchers: [CheckedContinuation<Void, Never>] = []
    /// Waiting for a call to be parked (`holding`).
    private var holdWatchers: [CheckedContinuation<Void, Never>] = []

    /// What to hand `CoreActor.inMemory(probe:)` or `TestStore`.
    nonisolated var probe: @Sendable () async -> Void {
        { await self.pass() }
    }

    /// The calls parked at the gate.
    var held: Int { parked.count }

    /// What the probe runs: count the call, wake anybody waiting for it, and
    /// park while the gate is closed or armed.
    func pass() async {
        calls += 1
        sawMainThread = sawMainThread || Thread.isMainThread
        wake(&arrivalWatchers)
        guard closed || armed else { return }
        armed = false
        await withCheckedContinuation { continuation in
            parked.append(continuation)
            wake(&holdWatchers)
        }
    }

    /// Parks every call from now on.
    func close() { closed = true }

    /// Lets every call through again, the parked ones first.
    func open() {
        closed = false
        release()
    }

    /// Parks the next call only.
    func arm() { armed = true }

    /// Lets the parked calls go on; a closed gate stays closed.
    func release() {
        let released = parked
        parked = []
        for call in released { call.resume() }
    }

    /// Returns once a call is parked.
    func holding() async {
        while parked.isEmpty {
            await withCheckedContinuation { holdWatchers.append($0) }
        }
    }

    /// Returns once at least `count` calls have reached the gate.
    func untilCalls(_ count: Int) async {
        while calls < count {
            await withCheckedContinuation { arrivalWatchers.append($0) }
        }
    }

    private func wake(_ watchers: inout [CheckedContinuation<Void, Never>]) {
        let woken = watchers
        watchers = []
        for watcher in woken { watcher.resume() }
    }
}
