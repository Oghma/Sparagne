import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// A barrier the core actor's probe waits on, so a test can hold a core call
/// in flight and look at the main actor while it is parked.
private actor Gate {
    private var closed = false
    private var parked: [CheckedContinuation<Void, Never>] = []
    private var watchers: [CheckedContinuation<Void, Never>] = []

    /// How many core calls have reached the gate, ever.
    private(set) var arrived = 0
    /// Whether any of them arrived on the main thread.
    private(set) var sawMainThread = false

    /// What the probe runs: count the call, wake anybody waiting for it, and
    /// park while the gate is closed.
    func pass() async {
        arrived += 1
        sawMainThread = sawMainThread || Thread.isMainThread
        for watcher in watchers { watcher.resume() }
        watchers.removeAll()
        guard closed else { return }
        await withCheckedContinuation { parked.append($0) }
    }

    func close() { closed = true }

    func open() {
        closed = false
        for call in parked { call.resume() }
        parked.removeAll()
    }

    /// Returns once at least `count` calls have reached the gate.
    func untilArrived(_ count: Int) async {
        while arrived < count {
            await withCheckedContinuation { watchers.append($0) }
        }
    }
}

/// The core lives on its own actor, off the main one.
///
/// These are the properties the move is for: the window is never blocked while
/// the core works, and two things asking at once are answered one at a time
/// rather than racing.
struct CoreActorTests {
    private static func makeStore(probe: (@Sendable () async -> Void)? = nil) throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.actor.\(UUID().uuidString)"))
        return AppStore(
            core: try CoreActor.inMemory(author: "tester", probe: probe),
            defaults: defaults,
            undoWindow: .seconds(60)
        )
    }

    /// Vault `Main`, wallet `Cash` with 100.00, envelope `Food` with 50.00.
    private static func onboarded(probe: (@Sendable () async -> Void)? = nil) async throws -> AppStore {
        let store = try makeStore(probe: probe)
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Cash", openingBalance: 10_000)
        await store.createEnvelope(
            name: "Food",
            mode: .unlimited,
            allowNegative: false,
            openingAllocation: 5_000
        )
        #expect(store.presentedError == nil)
        return store
    }

    @Test("The core is never called from the main thread")
    func theCoreRunsOffTheMainThread() async throws {
        let gate = Gate()
        let core = try CoreActor.inMemory(author: "tester", probe: { await gate.pass() })

        #expect(await core.runsOnTheMainThread() == false)

        // And the same for a real query made from the main actor.
        _ = try await core.vaults()
        #expect(await gate.arrived > 0)
        #expect(await gate.sawMainThread == false)
    }

    @Test("A write in flight leaves the main actor free, instead of blocking it")
    func theMainActorIsFreeWhileTheCoreWorks() async throws {
        let gate = Gate()
        let store = try await Self.onboarded(probe: { await gate.pass() })
        let before = await gate.arrived

        // From here on every core call parks at the gate.
        await gate.close()
        let write = Task { await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "pizza", amount: 1_250) }
        await gate.untilArrived(before + 1)

        // The call is inside the actor and going nowhere, yet the test is
        // running on the main actor and can read the store: a synchronous
        // core call would never have given it back.
        #expect(!store.rows.contains { $0.note == "pizza" })
        #expect(store.wallets.first?.balance == 10_000)

        await gate.open()
        await write.value
        await store.settle()

        #expect(store.presentedError == nil)
        #expect(store.rows.contains { $0.note == "pizza" })
        #expect(store.wallets.first?.balance == 8_750)
    }

    @Test("Two quick-adds sent at once are applied one at a time, in the order they were sent")
    func concurrentQuickAddsAreSerialized() async throws {
        let store = try await Self.onboarded()

        // `Food` holds 50.00 and refuses to go negative, so only the first of
        // two 40.00 expenses can be applied. Both reading the envelope before
        // either wrote it would let them both through.
        let first = Task { await store.submit(quickAdd: "-40.00 first >Food") }
        let second = Task { await store.submit(quickAdd: "-40.00 second >Food") }
        await first.value
        await second.value
        await store.settle()

        #expect(store.rows.contains { $0.note == "first" })
        #expect(!store.rows.contains { $0.note == "second" })
        #expect(store.presentedError?.code == "insufficient_funds")
        #expect(store.flows.first { $0.name == "Food" }?.balance == 1_000)
    }

    @Test("Two writes sent at once both land, and the balances count each of them once")
    func concurrentWritesBothPersist() async throws {
        let store = try await Self.onboarded()

        let first = Task { await store.submit(quickAdd: "-10.00 bus >Food") }
        let second = Task { await store.submit(quickAdd: "-20.00 pizza >Food") }
        await first.value
        await second.value
        await store.settle()

        #expect(store.presentedError == nil)
        #expect(store.rows.contains { $0.note == "bus" })
        #expect(store.rows.contains { $0.note == "pizza" })
        #expect(store.rows.filter { $0.note == "bus" || $0.note == "pizza" }.count == 2)
        let wallet = try #require(store.wallets.first)
        let food = try #require(store.flows.first { $0.name == "Food" })
        #expect(wallet.balance == 7_000)
        #expect(food.balance == 2_000)
    }

    @Test("Changing the month queues its load instead of running it on the main actor")
    func changingTheMonthQueuesItsLoad() async throws {
        let store = try await Self.onboarded()
        await store.submit(quickAdd: "-10.00 bus >Food")
        await store.settle()
        #expect(store.rows.contains { $0.note == "bus" })

        let previous = store.month.adding(months: -1)
        store.month = previous
        // The setter cannot await, so nothing has been loaded yet: the rows on
        // screen are still the ones of the month that was left.
        #expect(store.rows.contains { $0.note == "bus" })

        await store.settle()
        #expect(store.month == previous)
        #expect(store.rows.isEmpty)
    }
}
