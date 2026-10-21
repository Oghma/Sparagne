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
