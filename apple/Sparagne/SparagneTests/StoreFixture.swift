import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// A store over a fresh in-memory core, the way most tests start. Each one
/// gets a preferences suite of its own, so the last vault, the column widths
/// and the remembered mappings never leak from one test into the next.
///
/// The undo window and the sleeper default to `AppStore`'s own, so a test
/// that does not name them runs as the app does.
enum TestStore {
    /// The sleeper the app's undo toast counts down with.
    typealias Sleeper = @Sendable (Duration) async throws -> Void

    /// A preferences suite nobody else writes to.
    static func defaults(sourceLocation: SourceLocation = #_sourceLocation) throws -> UserDefaults {
        try #require(UserDefaults(suiteName: "sparagne.tests.\(UUID().uuidString)"), sourceLocation: sourceLocation)
    }

    /// A store before `bootstrap`, for the tests that look at onboarding,
    /// make their own vaults, or launch again on the same `defaults`.
    /// `probe` hears every call that reaches the core (`TestGate`).
    static func fresh(
        author: String = "matteo",
        defaults: UserDefaults? = nil,
        undoWindow: Duration = .seconds(5),
        sleeper: @escaping Sleeper = { try await Task.sleep(for: $0) },
        probe: (@Sendable () async -> Void)? = nil,
        defaultCategories: [DefaultCategory] = [],
        sourceLocation: SourceLocation = #_sourceLocation
    ) throws -> AppStore {
        try fresh(
            on: CoreActor.inMemory(author: author, probe: probe),
            defaults: defaults,
            undoWindow: undoWindow,
            sleeper: sleeper,
            defaultCategories: defaultCategories,
            sourceLocation: sourceLocation
        )
    }

    /// Another window on `core`, before `bootstrap`: another member's Mac
    /// once synced, or the app relaunched on the same database.
    static func fresh(
        on core: CoreActor,
        defaults: UserDefaults? = nil,
        undoWindow: Duration = .seconds(5),
        sleeper: @escaping Sleeper = { try await Task.sleep(for: $0) },
        defaultCategories: [DefaultCategory] = [],
        sourceLocation: SourceLocation = #_sourceLocation
    ) throws -> AppStore {
        AppStore(
            core: core,
            defaults: try defaults ?? Self.defaults(sourceLocation: sourceLocation),
            undoWindow: undoWindow,
            sleeper: sleeper,
            defaultCategories: defaultCategories
        )
    }

    /// A store through onboarding: vault `vault` with the wallet `wallet`
    /// opened at `opening`, which sits in Unallocated. An empty or blank
    /// `wallet` makes the vault without one.
    static func make(
        vault: String = "Casa",
        wallet: String = "Conto",
        opening: Int64 = 100_000,
        author: String = "matteo",
        defaults: UserDefaults? = nil,
        undoWindow: Duration = .seconds(5),
        sleeper: @escaping Sleeper = { try await Task.sleep(for: $0) },
        probe: (@Sendable () async -> Void)? = nil,
        defaultCategories: [DefaultCategory] = [],
        sourceLocation: SourceLocation = #_sourceLocation
    ) async throws -> AppStore {
        let store = try fresh(
            author: author,
            defaults: defaults,
            undoWindow: undoWindow,
            sleeper: sleeper,
            probe: probe,
            defaultCategories: defaultCategories,
            sourceLocation: sourceLocation
        )
        await store.bootstrap()
        await store.createVault(name: vault, walletName: wallet, openingBalance: opening)
        #expect(store.presentedError == nil, sourceLocation: sourceLocation)
        return store
    }
}

// MARK: - Finding what the store holds

extension TestStore {
    /// The wallet named `name`; the test stops if there is none.
    static func wallet(
        _ name: String,
        in store: AppStore,
        sourceLocation: SourceLocation = #_sourceLocation
    ) throws -> WalletView {
        try #require(store.wallets.first { $0.name == name }, sourceLocation: sourceLocation)
    }

    /// The flow named `name`; the test stops if there is none.
    static func flow(
        _ name: String,
        in store: AppStore,
        sourceLocation: SourceLocation = #_sourceLocation
    ) throws -> FlowView {
        try #require(store.flows.first { $0.name == name }, sourceLocation: sourceLocation)
    }

    /// Unallocated.
    static func unallocated(in store: AppStore, sourceLocation: SourceLocation = #_sourceLocation) throws -> FlowView {
        try #require(store.flows.first { $0.isUnallocated }, sourceLocation: sourceLocation)
    }

    // MARK: - The Mastro's household

    /// `make`'s vault `Casa`, wallet `Conto` at 1,000.00 in Unallocated,
    /// with the flow `flow`, unlimited and allowed below zero, and on it two
    /// expenses of today: `mutuo` 950.00 under Casa, then `coop` 50.00 under
    /// Spesa. `probe` is `make`'s.
    static func household(
        flow name: String = "Cash",
        probe: (@Sendable () async -> Void)? = nil,
        sourceLocation: SourceLocation = #_sourceLocation
    ) async throws -> (store: AppStore, flow: FlowView) {
        let store = try await make(probe: probe, sourceLocation: sourceLocation)
        await store.createEnvelope(name: name, mode: .unlimited, allowNegative: true, openingAllocation: 0)
        let flow = try Self.flow(name, in: store, sourceLocation: sourceLocation)
        await store.addRow(day: Date(), flowId: flow.id, category: "Casa", note: "mutuo", amount: 95_000)
        await store.addRow(day: Date(), flowId: flow.id, category: "Spesa", note: "coop", amount: 5_000)
        #expect(store.presentedError == nil, sourceLocation: sourceLocation)
        #expect(store.rows.map(\.note) == ["Mutuo", "Coop"], sourceLocation: sourceLocation)
        return (store, flow)
    }
}

// MARK: - Templates on a store

extension RecurringFixture {
    /// Vault `Main` with wallet `Cash` holding 100.00, written by `tester`.
    static func store(sourceLocation: SourceLocation = #_sourceLocation) async throws -> AppStore {
        try await TestStore.make(vault: "Main", wallet: "Cash", opening: 10_000, author: "tester", sourceLocation: sourceLocation)
    }

    /// A daily expense under Bills that started the day before yesterday:
    /// three periods due, that day, yesterday and today. `until` ends it
    /// early.
    @discardableResult
    static func dailyExpense(
        _ store: AppStore,
        note: String = "Gym",
        amount: Int64 = 50,
        flowId: Uuid? = nil,
        until end: NaiveDate? = nil,
        owner: String? = nil,
        sourceLocation: SourceLocation = #_sourceLocation
    ) async throws -> RecurringView {
        let start = try #require(NaiveDay.adding(-2, to: CoreDate.day(Date())), sourceLocation: sourceLocation)
        let created = await store.createRecurring(
            kind: .expense,
            amount: amount,
            walletId: nil,
            flowId: flowId,
            category: "Bills",
            note: note,
            schedule: Schedule(frequency: .daily, interval: 1, startDate: start, endDate: end),
            owner: owner
        )
        #expect(store.presentedError == nil, sourceLocation: sourceLocation)
        let id = try #require(created, sourceLocation: sourceLocation)
        return try #require(store.recurringTemplates.first { $0.id == id }, sourceLocation: sourceLocation)
    }
}
