import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The core lives on its own actor, off the main one.
///
/// These are the properties the move is for: the window is never blocked while
/// the core works, and two things asking at once are answered one at a time
/// rather than racing.
struct CoreActorTests {
    /// Vault `Main`, wallet `Cash` with 100.00, envelope `Food` with 50.00.
    private static func onboarded(probe: (@Sendable () async -> Void)? = nil) async throws -> AppStore {
        let store = try await TestStore.make(
            vault: "Main", wallet: "Cash", opening: 10_000, author: "tester", undoWindow: .seconds(60), probe: probe
        )
        await store.createEnvelope(name: "Food", mode: .unlimited, allowNegative: false, openingAllocation: 5_000)
        #expect(store.presentedError == nil)
        return store
    }

    @Test("The core is never called from the main thread")
    func theCoreRunsOffTheMainThread() async throws {
        let gate = TestGate()
        let core = try CoreActor.inMemory(author: "tester", probe: gate.probe)

        #expect(await core.runsOnTheMainThread() == false)

        // And the same for a real query made from the main actor.
        _ = try await core.vaults()
        #expect(await gate.calls > 0)
        #expect(await gate.sawMainThread == false)
    }

    @Test("A write in flight leaves the main actor free, instead of blocking it")
    func theMainActorIsFreeWhileTheCoreWorks() async throws {
        let gate = TestGate()
        let store = try await Self.onboarded(probe: gate.probe)
        let before = await gate.calls

        // From here on every core call parks at the gate.
        await gate.close()
        let write = Task { await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "pizza", amount: 1_250) }
        await gate.untilCalls(before + 1)

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
        #expect(store.presentedError?.message.contains("Food") == true)
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
