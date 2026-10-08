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
nonisolated enum E2EServer {
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
private struct E2EPeer {
    let core: CoreActor
    let store: AppStore
    let account: AccountStore
    let engine: SyncEngine
    let username: String

    static let password = "supersecret"

    init(server: URL, root: URL, name: String, username: String) async throws {
        let directory = root.appending(path: name, directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let database = directory.appending(path: "sparagne.sqlite", directoryHint: .notDirectory)

        let defaults = try #require(UserDefaults(suiteName: "sparagne.e2e.\(UUID().uuidString)"))
        self.username = username
        core = CoreActor(
            handle: try CoreHandle.open(path: database.path(percentEncoded: false)),
            author: "local"
        )
        store = AppStore(core: core, defaults: defaults, undoWindow: .seconds(60))
        account = AccountStore(defaults: defaults, tokens: MemoryTokenStore())
        account.serverURLText = server.absoluteString
        // The default transport is `URLSessionTransport`: this is the path
        // under test.
        engine = SyncEngine(core: core, store: store, account: account)
        // No timer and no debounce: every sync here is explicit.
        engine.automaticSync = false
        await engine.prepare()
        await store.bootstrap()
    }

    var vaultId: Uuid? { store.currentVault?.id }

    func register() async throws {
        await engine.register(username: username, password: Self.password)
        try #require(account.isLoggedIn, "register failed: \(engine.authMessage ?? "no message")")
    }

    func syncState() async throws -> SyncState {
        try await core.syncState(vaultId: #require(vaultId))
    }

    /// Every transaction in the vault, whatever month and direction the ledger
    /// window happens to be filtering on (`store.transactions` is one page of
    /// one month, expenses only).
    func allTransactions() async throws -> [TransactionView] {
        try await core.transactions(
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
                person: nil,
                ascending: true
            ),
            limit: 500,
            cursor: nil
        ).items
    }

    func notes() async throws -> [String] {
        try await allTransactions().compactMap(\.note)
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
        let peer = try await E2EPeer(server: server, root: root, name: "owner", username: "a\(suffix)")
        await peer.store.createVault(name: "Casa \(suffix)", walletName: "Cash", openingBalance: 10_000)
        await peer.store.createEnvelope(
            name: "Food",
            mode: .unlimited,
            allowNegative: false,
            openingAllocation: 5_000
        )
        await peer.store.submit(quickAdd: "+120.00 stipendio @Cash")
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
        let bob = try await E2EPeer(server: server, root: root, name: "bob", username: "b\(suffix)")
        try await bob.register()
        #expect(bob.store.currentVault == nil)

        let alice = try await Self.owner(server: server, root: root, suffix: suffix)
        let vaultId = try #require(alice.vaultId)

        // The push claimed the vault: no route created it, the outbox is empty
        // and the server made the caller its owner.
        #expect(alice.engine.status == .idle)
        #expect(try await alice.syncState().outbox == 0)
        #expect(try await alice.syncState().lastServerSeq > 0)
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
        #expect(try await bob.allTransactions() == (try await alice.allTransactions()))

        await bob.store.submit(quickAdd: "-12.50 pizza @Cash >Food")
        #expect(bob.store.presentedError == nil)
        await bob.engine.syncNow()
        await alice.engine.syncNow()

        #expect(alice.engine.status == .idle)
        #expect(bob.engine.status == .idle)
        #expect(try await alice.syncState().outbox == 0)
        #expect(try await bob.syncState().outbox == 0)
        #expect(try await alice.syncState().rejected == 0)
        #expect(try await bob.syncState().rejected == 0)
        #expect(try await alice.syncState().lastServerSeq == (try await bob.syncState().lastServerSeq))
        #expect(alice.store.snapshot == bob.store.snapshot)
        #expect(try await alice.allTransactions() == (try await bob.allTransactions()))
        #expect(try await alice.notes().contains("pizza"))
        #expect(try await alice.notes().contains("stipendio"))
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

    @Test("A viewer writes nothing, a demoted editor's change is refused, and the owner's changes still arrive")
    func viewerCannotPush() async throws {
        let server = try #require(E2EServer.url)
        let root = try Self.temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let suffix = E2EServer.suffix()

        let carol = try await E2EPeer(server: server, root: root, name: "carol", username: "c\(suffix)")
        try await carol.register()
        let alice = try await Self.owner(server: server, root: root, suffix: suffix)
        let vaultId = try #require(alice.vaultId)

        try await alice.engine.setMember(vaultId: vaultId, username: carol.username, role: .editor)
        await carol.engine.syncNow()
        #expect(carol.store.currentVault?.id == vaultId)
        #expect(carol.serverRole(vaultId) == .editor)

        // Written as an editor, still unsent when the owner demotes carol and
        // renames the vault.
        await carol.store.submit(quickAdd: "-1.00 gum @Cash >Food")
        #expect(carol.store.presentedError == nil)
        try await alice.engine.setMember(vaultId: vaultId, username: carol.username, role: .viewer)
        await alice.store.renameVault(vaultId, name: "Renamed \(suffix)")
        await alice.engine.syncNow()

        await carol.engine.syncNow()

        // The roles came first: no push, the change is refused here as the
        // server would, and the pull still brought the rename.
        #expect(carol.engine.status == .idle)
        #expect(carol.serverRole(vaultId) == .viewer)
        #expect(try await carol.syncState().outbox == 0)
        #expect(try await carol.syncState().rejected == 1)
        #expect(carol.engine.pendingCount == 0)
        #expect(carol.engine.rejected.first?.command.code == "forbidden")
        #expect(carol.store.currentVault?.name == "Renamed \(suffix)")
        #expect(try await carol.notes().contains("gum") == false)

        // From now on the vault is read-only on carol's side.
        #expect(carol.store.isReadOnly)
        await carol.store.submit(quickAdd: "-2.00 mints @Cash >Food")
        #expect(carol.store.presentedError?.code == "forbidden")
        #expect(try await carol.syncState().outbox == 0)

        // The owner never sees either.
        await alice.engine.syncNow()
        #expect(try await alice.notes().contains("gum") == false)
        #expect(alice.engine.status == .idle)
        #expect(alice.store.snapshot == carol.store.snapshot)
    }

    @Test("A member leaves: the pending change goes up first, then the vault leaves the device")
    func memberLeaves() async throws {
        let server = try #require(E2EServer.url)
        let root = try Self.temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let suffix = E2EServer.suffix()

        let bob = try await E2EPeer(server: server, root: root, name: "bob", username: "b\(suffix)")
        try await bob.register()
        let alice = try await Self.owner(server: server, root: root, suffix: suffix)
        let vaultId = try #require(alice.vaultId)
        try await alice.engine.setMember(vaultId: vaultId, username: bob.username, role: .editor)
        await bob.engine.syncNow()
        #expect(bob.store.currentVault?.id == vaultId)
        #expect(bob.engine.mayLeaveVault(vaultId))
        #expect(!alice.engine.mayLeaveVault(vaultId))

        await bob.store.submit(quickAdd: "-12.50 pizza @Cash >Food")
        try await bob.engine.leaveVault(vaultId)

        #expect(bob.store.vaults.isEmpty)
        #expect(try await bob.core.vaults().isEmpty)
        #expect(bob.engine.pendingCount == 0)
        let members = try await alice.engine.members(ofVault: vaultId)
        #expect(members.map(\.username) == [alice.username])
        await alice.engine.syncNow()
        #expect(try await alice.notes().contains("pizza"))

        // Nothing brings it back, and the owner cannot leave their own.
        await bob.engine.syncNow()
        #expect(bob.engine.status == .idle)
        #expect(bob.store.vaults.isEmpty)
        do {
            try await alice.engine.leaveVault(vaultId)
            Issue.record("the owner should not be able to leave")
        } catch let error as ServerError {
            #expect(error.status == 403)
        }
        #expect(alice.store.currentVault?.id == vaultId)
    }

    @Test("Changing the password keeps this session and revokes the other device's")
    func changePasswordRevokesOtherDevices() async throws {
        let server = try #require(E2EServer.url)
        let root = try Self.temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let suffix = E2EServer.suffix()

        let alice = try await Self.owner(server: server, root: root, suffix: suffix)
        let vaultId = try #require(alice.vaultId)
        let laptop = try await E2EPeer(server: server, root: root, name: "laptop", username: alice.username)
        await laptop.engine.logIn(username: alice.username, password: E2EPeer.password)
        #expect(laptop.account.isLoggedIn)
        #expect(laptop.store.currentVault?.id == vaultId)
        #expect(laptop.account.expiresAt.map { $0 > Date() } == true)

        do {
            try await alice.engine.changePassword(current: "not-the-password", new: "brandnewsecret")
            Issue.record("a wrong current password should be refused")
        } catch let error as ServerError {
            #expect(error.status == 401)
        }
        #expect(alice.account.isLoggedIn)

        try await alice.engine.changePassword(current: E2EPeer.password, new: "brandnewsecret")
        await alice.engine.syncNow()
        #expect(alice.account.isLoggedIn)
        #expect(alice.engine.status == .idle)

        await laptop.engine.syncNow()
        #expect(!laptop.account.isLoggedIn)
        #expect(laptop.account.lastUsername == alice.username)
        #expect(laptop.engine.status == .error(ErrorMessages.sessionExpired))

        await laptop.engine.logIn(username: alice.username, password: "brandnewsecret")
        #expect(laptop.account.isLoggedIn)
        #expect(laptop.engine.status == .idle)
    }
}
