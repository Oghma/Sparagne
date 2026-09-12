import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Where the real server is, when there is one.
///
/// `scripts/e2e.sh` starts `sparagne-server` on a free port and passes the
/// address down as `TEST_RUNNER_SPARAGNE_E2E_SERVER`; `xcodebuild` hands it to
/// the test process with the prefix stripped, and the unprefixed name is read
/// too so the suite also runs under a server started by hand.
enum E2EServer {
    static let variable = "SPARAGNE_E2E_SERVER"

    static var url: URL? {
        let environment = ProcessInfo.processInfo.environment
        let raw = environment[variable] ?? environment["TEST_RUNNER_\(variable)"]
        guard let raw, !raw.trimmingCharacters(in: .whitespaces).isEmpty else { return nil }
        return URL(string: raw.trimmingCharacters(in: .whitespaces))
    }

    /// Accounts are never cleaned up, so every run invents its own names and
    /// the same server can be reused for as many runs as one likes. The server
    /// wants 3-32 characters of `[a-z0-9_.-]`.
    static func suffix() -> String {
        UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(12).lowercased()
    }
}

/// One app: its own SQLite file, its own preferences, its own account, and a
/// `SyncEngine` on the real `URLSessionTransport`.
///
/// This is `SyncEngineTests.Peer` pointed at a live server instead of a fake
/// one: same construction, same API, nothing stubbed below `ServerAPI`.
@MainActor
private struct E2EPeer {
    let client: CoreClient
    let store: AppStore
    let account: AccountStore
    let engine: SyncEngine
    let username: String

    static let password = "supersecret"

    init(server: URL, root: URL, name: String, username: String) throws {
        let directory = root.appending(path: name, directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let database = directory.appending(path: "sparagne.sqlite", directoryHint: .notDirectory)

        let defaults = try #require(UserDefaults(suiteName: "sparagne.e2e.\(UUID().uuidString)"))
        self.username = username
        client = CoreClient(
            handle: try CoreHandle.open(path: database.path(percentEncoded: false)),
            author: "local"
        )
        store = AppStore(client: client, defaults: defaults, undoWindow: .seconds(60))
        account = AccountStore(defaults: defaults, tokens: MemoryTokenStore())
        account.serverURLText = server.absoluteString
        // The default transport is `URLSessionTransport`: this is the path
        // under test.
        engine = SyncEngine(client: client, store: store, account: account)
        // No timer and no debounce: every sync here is explicit.
        engine.automaticSync = false
        store.bootstrap()
    }

    var vaultId: Uuid? { store.currentVault?.id }

    func register() async throws {
        await engine.register(username: username, password: Self.password)
        try #require(account.isLoggedIn, "register failed: \(engine.authMessage ?? "no message")")
    }

    func syncState() throws -> SyncState {
        try client.syncState(vaultId: #require(vaultId))
    }

    /// Every transaction in the vault, whatever month and direction the ledger
    /// window happens to be filtering on (`store.transactions` is one page of
    /// one month, expenses only).
    func allTransactions() throws -> [TransactionView] {
        try client.transactions(
            vaultId: #require(vaultId),
            filter: TransactionFilter(
                from: nil,
                to: nil,
                kinds: nil,
                includeVoided: true,
                includeTransfers: true,
                walletId: nil,
                flowId: nil,
                text: nil,
                author: nil,
                ascending: true
            ),
            limit: 500,
            cursor: nil
        ).items
    }

    func notes() throws -> [String] {
        try allTransactions().compactMap(\.note)
    }

    /// The role `GET /vaults` last reported for `vaultId`.
    func serverRole(_ vaultId: Uuid) -> MemberRole? {
        engine.serverVaults.first { $0.id == vaultId }?.role
    }
}

@Suite(
    "The app against a real server",
    .serialized,
    .enabled(if: E2EServer.url != nil, "SPARAGNE_E2E_SERVER is not set: run scripts/e2e.sh")
)
@MainActor
struct ServerE2ETests {
    /// A directory of throwaway SQLite files, one subdirectory per peer. The
    /// app's own database in the container is never opened.
    private static func temporaryRoot() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appending(path: "sparagne-e2e-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    /// A vault with a `Cash` wallet, a `Food` envelope and one income, pushed
    /// to the server by the very sync that logging in triggers.
    private static func owner(server: URL, root: URL, suffix: String) async throws -> E2EPeer {
        let peer = try E2EPeer(server: server, root: root, name: "owner", username: "a\(suffix)")
        peer.store.createVault(name: "Casa \(suffix)", walletName: "Cash", openingBalance: 10_000)
        peer.store.createEnvelope(
            name: "Food",
            mode: .unlimited,
            allowNegative: false,
            openingAllocation: 5_000
        )
        peer.store.submit(quickAdd: "+120.00 stipendio @Cash")
        #expect(peer.store.presentedError == nil)
        try await peer.register()
        return peer
    }

    @Test("Two accounts share a vault over HTTP and converge")
    func twoClientsConverge() async throws {
        let server = try #require(E2EServer.url)
        let root = try Self.temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let suffix = E2EServer.suffix()

        // B has to exist before the owner can name it as a member.
        let bob = try E2EPeer(server: server, root: root, name: "bob", username: "b\(suffix)")
        try await bob.register()
        #expect(bob.store.currentVault == nil)

        let alice = try await Self.owner(server: server, root: root, suffix: suffix)
        let vaultId = try #require(alice.vaultId)

        // The push claimed the vault: no route created it, the outbox is empty
        // and the server made the caller its owner.
        #expect(alice.engine.status == .idle)
        #expect(try alice.syncState().outbox == 0)
        #expect(try alice.syncState().lastServerSeq > 0)
        #expect(alice.serverRole(vaultId) == .owner)

        try await alice.engine.setMember(vaultId: vaultId, username: bob.username, role: .editor)
        let members = try await alice.engine.members(ofVault: vaultId)
        #expect(members.map(\.username).sorted() == [alice.username, bob.username].sorted())

        // B knows nothing of the vault: it arrives whole, from seq 0.
        await bob.engine.syncNow()
        #expect(bob.engine.status == .idle)
        #expect(bob.store.currentVault?.id == vaultId)
        #expect(bob.store.currentVault?.owner == alice.username)
        #expect(bob.serverRole(vaultId) == .editor)
        #expect(bob.store.snapshot == alice.store.snapshot)
        #expect(try bob.allTransactions() == (try alice.allTransactions()))

        bob.store.submit(quickAdd: "-12.50 pizza @Cash >Food")
        #expect(bob.store.presentedError == nil)
        await bob.engine.syncNow()
        await alice.engine.syncNow()

        #expect(alice.engine.status == .idle)
        #expect(bob.engine.status == .idle)
        #expect(try alice.syncState().outbox == 0)
        #expect(try bob.syncState().outbox == 0)
        #expect(try alice.syncState().rejected == 0)
        #expect(try bob.syncState().rejected == 0)
        #expect(try alice.syncState().lastServerSeq == (try bob.syncState().lastServerSeq))
        #expect(alice.store.snapshot == bob.store.snapshot)
        #expect(try alice.allTransactions() == (try bob.allTransactions()))
        #expect(try alice.notes().contains("pizza"))
        #expect(try alice.notes().contains("stipendio"))
        #expect(alice.engine.rejected.isEmpty)
        #expect(bob.engine.rejected.isEmpty)

        // `GET /vaults` agrees with both sides about who owns what.
        #expect(alice.engine.serverVaults.map(\.id) == [vaultId])
        #expect(bob.engine.serverVaults.map(\.id) == [vaultId])
        #expect(alice.serverRole(vaultId) == .owner)
        #expect(bob.serverRole(vaultId) == .editor)
        #expect(alice.engine.serverVaults.first?.owner == alice.username)
        #expect(bob.engine.serverVaults.first?.name == alice.store.currentVault?.name)
    }

    @Test("A viewer's push comes back refused and the change survives")
    func viewerCannotPush() async throws {
        let server = try #require(E2EServer.url)
        let root = try Self.temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let suffix = E2EServer.suffix()

        let carol = try E2EPeer(server: server, root: root, name: "carol", username: "c\(suffix)")
        try await carol.register()
        let alice = try await Self.owner(server: server, root: root, suffix: suffix)
        let vaultId = try #require(alice.vaultId)

        try await alice.engine.setMember(vaultId: vaultId, username: carol.username, role: .viewer)
        await carol.engine.syncNow()
        #expect(carol.store.currentVault?.id == vaultId)
        #expect(carol.serverRole(vaultId) == .viewer)

        carol.store.submit(quickAdd: "-1.00 gum @Cash >Food")
        #expect(carol.store.presentedError == nil)
        await carol.engine.syncNow()

        // The engine reports the refusal instead of crashing, and the command
        // stays in the outbox: nothing is lost by a 403.
        switch carol.engine.status {
        // The 403 the server really sent, mapped to its localized headline.
        case .error(let text): #expect(text.contains(ErrorMessages.summary(for: "forbidden")))
        default: Issue.record("expected an error status, got \(carol.engine.status)")
        }
        #expect(try carol.syncState().outbox == 1)
        #expect(carol.engine.pendingCount == 1)
        #expect(carol.engine.rejected.isEmpty)

        // The owner never sees the change.
        await alice.engine.syncNow()
        #expect(try alice.notes().contains("gum") == false)
        #expect(alice.engine.status == .idle)
    }
}
