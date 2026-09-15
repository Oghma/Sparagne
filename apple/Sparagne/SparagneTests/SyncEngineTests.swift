import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// A server made of a second core plus a Swift auth and membership table.
///
/// It implements exactly the routes `SyncEngine` calls, with the rules of
/// `docs/v2/SYNC.md` §3: a non-member gets 404, a viewer 403, an envelope
/// signed by someone else 403 `author_mismatch`. The command log itself is
/// the core's own `serve_push` and `serve_pull`, so the tests exercise the
/// real protocol rather than a mock of it.
actor FakeServerTransport: SyncTransport {
    private let core: CoreHandle
    private var passwords: [String: String] = [:]
    private var sessions: [String: String] = [:]
    private var memberships: [Uuid: [String: MemberRole]] = [:]
    private var issued = 0
    private var offline = false
    private var log: [String] = []

    init(core: CoreHandle) {
        self.core = core
    }

    /// Every request fails the way a missing network does.
    func setOffline(_ value: Bool) {
        offline = value
    }

    /// How many times a route was called, for "this happens only once".
    func callCount(method: String, path: String) -> Int {
        log.filter { $0 == "\(method) \(path)" }.count
    }

    func lastSeq(ofVault vaultId: Uuid) throws -> Int64 {
        try core.lastSeq(vaultId: vaultId)
    }

    func snapshot(ofVault vaultId: Uuid) throws -> VaultSnapshot {
        try core.snapshot(vaultId: vaultId)
    }

    /// The server's own row for a vault; `nil` once it has been deleted.
    func vault(ofVault vaultId: Uuid) throws -> VaultView? {
        try core.vault(vaultId: vaultId)
    }

    func send(_ request: SyncRequest) async throws -> SyncResponse {
        log.append("\(request.method) \(Self.route(request.path))")
        if offline { throw ServerError.offline(detail: "the fake server is unreachable") }
        let path = Self.route(request.path)
        let query = Self.query(request.path)
        let parts = path.split(separator: "/").map(String.init)

        if request.method == "POST", path == "/auth/register" { return try register(request) }
        if request.method == "POST", path == "/auth/login" { return try login(request) }
        if request.method == "POST", path == "/auth/logout" {
            if let bearer = request.bearer { sessions.removeValue(forKey: bearer) }
            return SyncResponse(status: 204)
        }
        if request.method == "GET", path == "/me" {
            guard let user = caller(request) else { return Self.failure(401, "unauthorized") }
            return try Self.encode(["username": user])
        }
        if request.method == "GET", path == "/vaults" { return try vaults(request) }

        if parts.count == 3, parts[0] == "vaults" {
            let vaultId = parts[1]
            if request.method == "POST", parts[2] == "push" { return try push(request, vaultId: vaultId) }
            if request.method == "GET", parts[2] == "pull" {
                return try pull(request, vaultId: vaultId, query: query)
            }
            if parts[2] == "members" {
                if request.method == "GET" { return try members(request, vaultId: vaultId) }
                if request.method == "PUT" { return try setMember(request, vaultId: vaultId) }
            }
        }
        if parts.count == 4, parts[0] == "vaults", parts[2] == "members", request.method == "DELETE" {
            return try removeMember(request, vaultId: parts[1], username: parts[3])
        }
        return Self.failure(404, "not_found")
    }

    // MARK: - Auth

    private func register(_ request: SyncRequest) throws -> SyncResponse {
        guard let credentials = Self.credentials(request) else { return Self.failure(400, "invalid_request") }
        guard passwords[credentials.username] == nil else { return Self.failure(409, "already_exists") }
        passwords[credentials.username] = credentials.password
        return try token(for: credentials.username, status: 201)
    }

    private func login(_ request: SyncRequest) throws -> SyncResponse {
        guard let credentials = Self.credentials(request),
            passwords[credentials.username] == credentials.password
        else { return Self.failure(401, "unauthorized") }
        return try token(for: credentials.username, status: 200)
    }

    private func token(for username: String, status: Int) throws -> SyncResponse {
        issued += 1
        let token = "token-\(username)-\(issued)"
        sessions[token] = username
        return try Self.encode(
            TokenResponse(token: token, expiresAt: 4_102_444_800, username: username),
            status: status
        )
    }

    private func caller(_ request: SyncRequest) -> String? {
        request.bearer.flatMap { sessions[$0] }
    }

    // MARK: - Vaults

    private func vaults(_ request: SyncRequest) throws -> SyncResponse {
        guard let user = caller(request) else { return Self.failure(401, "unauthorized") }
        var summaries: [VaultSummary] = []
        for vault in try core.vaults() {
            guard let role = memberships[vault.id]?[user] else { continue }
            summaries.append(
                VaultSummary(
                    id: vault.id,
                    name: vault.name,
                    currency: vault.currency.code,
                    owner: vault.owner,
                    role: role,
                    lastSeq: try core.lastSeq(vaultId: vault.id)
                )
            )
        }
        return try Self.encode(summaries)
    }

    /// `POST /vaults/{id}/push`. A vault the server has never seen is created
    /// by this very request when its first command is the `CreateVault` that
    /// minted it, together with the caller's `owner` membership
    /// (`docs/v2/SYNC.md` §3).
    private func push(_ request: SyncRequest, vaultId: Uuid) throws -> SyncResponse {
        guard let user = caller(request) else { return Self.failure(401, "unauthorized") }
        guard let body = request.body,
            let root = try JSONSerialization.jsonObject(with: body) as? [String: Any],
            let commands = root["commands"] as? [[String: Any]]
        else { return Self.failure(400, "invalid_request") }
        guard !commands.contains(where: { $0["author"] as? String != user }) else {
            return Self.failure(403, "author_mismatch")
        }

        switch memberships[vaultId]?[user] {
        case .none:
            guard let claim = Self.createVault(in: commands.first, of: vaultId) else {
                return Self.failure(404, "not_found")
            }
            // The log outlives the projection: a deleted vault's id stays
            // taken, as on the real server.
            guard try core.lastSeq(vaultId: vaultId) == 0 else {
                return Self.failure(404, "not_found")
            }
            let taken = try core.vaults()
                .contains { $0.owner == user && $0.name.lowercased() == claim.lowercased() }
            guard !taken else { return Self.failure(409, "already_exists") }
            memberships[vaultId] = [user: .owner]
        case .some(let role) where !role.canWrite:
            return Self.failure(403, "forbidden")
        default:
            break
        }

        let text = String(decoding: body, as: UTF8.self)
        return SyncResponse(
            status: 200,
            body: Data(try core.servePushJson(vaultId: vaultId, json: text).utf8)
        )
    }

    /// The name of the vault `envelope` creates, when it is the `CreateVault`
    /// that mints `vaultId`; `nil` for anything else.
    private static func createVault(in envelope: [String: Any]?, of vaultId: Uuid) -> String? {
        guard let envelope,
            envelope["id"] as? String == vaultId,
            let command = envelope["command"] as? [String: Any],
            command["kind"] as? String == "create_vault",
            let name = command["name"] as? String
        else { return nil }
        return name
    }

    private func pull(_ request: SyncRequest, vaultId: Uuid, query: [String: String]) throws -> SyncResponse {
        guard let user = caller(request) else { return Self.failure(401, "unauthorized") }
        guard memberships[vaultId]?[user] != nil else { return Self.failure(404, "not_found") }
        let since = Int64(query["since"] ?? "0") ?? 0
        let limit = UInt32(query["limit"] ?? "500") ?? 500
        return SyncResponse(
            status: 200,
            body: Data(try core.servePullJson(vaultId: vaultId, since: since, limit: limit).utf8)
        )
    }

    // MARK: - Members

    private func members(_ request: SyncRequest, vaultId: Uuid) throws -> SyncResponse {
        guard let user = caller(request) else { return Self.failure(401, "unauthorized") }
        guard let table = memberships[vaultId], table[user] != nil else {
            return Self.failure(404, "not_found")
        }
        let entries = table
            .map { MemberEntry(username: $0.key, role: $0.value) }
            .sorted { $0.username < $1.username }
        return try Self.encode(entries)
    }

    private func setMember(_ request: SyncRequest, vaultId: Uuid) throws -> SyncResponse {
        guard let user = caller(request) else { return Self.failure(401, "unauthorized") }
        guard memberships[vaultId]?[user] == .owner else { return Self.failure(403, "forbidden") }
        guard let body = request.body,
            let entry = try? JSONDecoder().decode(MemberEntry.self, from: body)
        else { return Self.failure(400, "invalid_request") }
        guard entry.role != .owner else { return Self.failure(400, "invalid_request") }
        guard passwords[entry.username] != nil else { return Self.failure(404, "not_found") }
        guard memberships[vaultId]?[entry.username] != .owner else {
            return Self.failure(403, "forbidden")
        }
        memberships[vaultId]?[entry.username] = entry.role
        return SyncResponse(status: 204)
    }

    private func removeMember(_ request: SyncRequest, vaultId: Uuid, username: String) throws -> SyncResponse {
        guard let user = caller(request) else { return Self.failure(401, "unauthorized") }
        guard memberships[vaultId]?[user] == .owner else { return Self.failure(403, "forbidden") }
        switch memberships[vaultId]?[username] {
        case .none: return Self.failure(404, "not_found")
        case .some(.owner): return Self.failure(403, "forbidden")
        case .some: memberships[vaultId]?.removeValue(forKey: username)
        }
        return SyncResponse(status: 204)
    }

    // MARK: - Plumbing

    private static func route(_ path: String) -> String {
        String(path.split(separator: "?", maxSplits: 1).first ?? "")
    }

    private static func query(_ path: String) -> [String: String] {
        let parts = path.split(separator: "?", maxSplits: 1)
        guard parts.count == 2 else { return [:] }
        var out: [String: String] = [:]
        for pair in parts[1].split(separator: "&") {
            let halves = pair.split(separator: "=", maxSplits: 1)
            if halves.count == 2 { out[String(halves[0])] = String(halves[1]) }
        }
        return out
    }

    private struct CredentialsBody: Decodable {
        let username: String
        let password: String
    }

    private static func credentials(_ request: SyncRequest) -> CredentialsBody? {
        request.body.flatMap { try? JSONDecoder().decode(CredentialsBody.self, from: $0) }
    }

    private static func encode(_ value: some Encodable, status: Int = 200) throws -> SyncResponse {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        return SyncResponse(status: status, body: try encoder.encode(value))
    }

    private static func failure(_ status: Int, _ code: String) -> SyncResponse {
        let body = #"{"error":{"code":"\#(code)","message":"\#(code)"}}"#
        return SyncResponse(status: status, body: Data(body.utf8))
    }
}

/// One app: its own in-memory core, store, account and engine, all pointed at
/// the same fake server.
@MainActor
private struct Peer {
    let core: CoreActor
    let store: AppStore
    let account: AccountStore
    let engine: SyncEngine

    init(server: FakeServerTransport) async throws {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.sync.\(UUID().uuidString)"))
        core = try CoreActor.inMemory(author: "local")
        store = AppStore(core: core, defaults: defaults, undoWindow: .seconds(60))
        account = AccountStore(defaults: defaults, tokens: MemoryTokenStore())
        account.serverURLText = "http://fake.test"
        engine = SyncEngine(
            core: core,
            store: store,
            account: account,
            makeTransport: { _ in server }
        )
        // No timer and no debounce: every sync in these tests is explicit.
        engine.automaticSync = false
        await engine.prepare()
        await store.bootstrap()
    }

    var vaultId: Uuid? { store.currentVault?.id }

    func syncState() async throws -> SyncState {
        try await core.syncState(vaultId: #require(vaultId))
    }

    /// Names and balances of the wallets and envelopes, for comparing two
    /// replicas.
    func balances() -> [String: Int64] {
        var out: [String: Int64] = [:]
        for wallet in store.snapshot?.wallets ?? [] { out["wallet:\(wallet.name)"] = wallet.balance }
        for flow in store.snapshot?.flows ?? [] { out["flow:\(flow.name)"] = flow.balance }
        return out
    }

    func notes() -> [String] {
        store.rows.map(\.note).sorted()
    }
}

@MainActor
struct SyncEngineTests {
    /// A vault with a `Cash` wallet holding 100.00 and a `Food` envelope
    /// holding 50.00, all still in the outbox.
    private static func seed(_ peer: Peer) async {
        await peer.store.createVault(name: "Main", walletName: "Cash", openingBalance: 10_000)
        await peer.store.createEnvelope(
            name: "Food",
            mode: .unlimited,
            allowNegative: false,
            openingAllocation: 5_000
        )
    }

    private static func alice(_ server: FakeServerTransport) async throws -> Peer {
        let peer = try await Peer(server: server)
        await seed(peer)
        await peer.engine.register(username: "alice", password: "supersecret")
        return peer
    }

    @Test("Logging in re-signs the outbox, so the server accepts it")
    func loginRelabelsTheOutbox() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let peer = try await Peer(server: server)
        await Self.seed(peer)

        #expect(peer.store.transactions.allSatisfy { $0.createdBy != "alice" })
        #expect(try await peer.syncState().outbox > 0)

        await peer.engine.register(username: "alice", password: "supersecret")

        #expect(peer.account.username == "alice")
        #expect(peer.store.currentVault?.owner == "alice")
        #expect(peer.store.transactions.allSatisfy { $0.createdBy == "alice" })
        // The push only goes through when the author matches the account:
        // an empty outbox is the proof the relabelling happened first.
        #expect(try await peer.syncState().outbox == 0)
        #expect(peer.engine.status == .idle)
    }

    @Test("The first sync creates the vault on the server and confirms every command")
    func firstSyncCreatesTheVault() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let peer = try await Self.alice(server)
        let vaultId = try #require(peer.vaultId)

        let state = try await peer.syncState()
        #expect(state.outbox == 0)
        #expect(state.rejected == 0)
        #expect(state.lastServerSeq == (try await server.lastSeq(ofVault: vaultId)))
        #expect(state.lastServerSeq > 0)
        #expect(peer.engine.pendingCount == 0)
        #expect(peer.engine.serverVaults.map(\.id) == [vaultId])
        #expect(peer.engine.isOwner(ofVault: vaultId))
        #expect(peer.engine.lastSyncAt != nil)
    }

    @Test("The vault is created by the first push, with no route of its own")
    func theFirstPushCreatesTheVault() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let peer = try await Self.alice(server)
        let vaultId = try #require(peer.vaultId)

        await peer.store.submit(quickAdd: "-10.00 coffee @Cash >Food")
        await peer.engine.syncNow()
        await peer.engine.syncNow()

        // `POST /vaults` is gone: everything went through the push route, and
        // the seeding sync plus these two are the only pushes.
        #expect(await server.callCount(method: "POST", path: "/vaults") == 0)
        #expect(await server.callCount(method: "POST", path: "/vaults/\(vaultId)/push") == 2)
        #expect(try await peer.syncState().outbox == 0)
        #expect(peer.engine.isOwner(ofVault: vaultId))
        #expect(peer.engine.status == .idle)
    }

    @Test("A vault name the account already used on the server comes back refused")
    func aDuplicateVaultNameIsRefused() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        _ = try await Self.alice(server)

        // A second machine, same account, a local vault with the same name:
        // the push that would create it is a conflict, not a silent success.
        let other = try await Peer(server: server)
        await Self.seed(other)
        let clash = try #require(other.vaultId)
        await other.engine.logIn(username: "alice", password: "supersecret")

        #expect(other.account.isLoggedIn)
        #expect(other.engine.status != .idle)
        #expect(try await other.core.syncState(vaultId: clash).outbox > 0)
        #expect(try await other.core.syncState(vaultId: clash).lastServerSeq == 0)
    }

    @Test("An editor joins a shared vault and gets the same snapshot")
    func editorJoinsSharedVault() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let vaultId = try #require(alice.vaultId)

        let bob = try await Peer(server: server)
        await bob.engine.register(username: "bob", password: "supersecret")
        #expect(bob.store.currentVault == nil)

        try await alice.engine.setMember(vaultId: vaultId, username: "bob", role: .editor)
        await bob.engine.syncNow()

        #expect(bob.store.currentVault?.id == vaultId)
        #expect(bob.store.currentVault?.owner == "alice")
        #expect(bob.balances() == alice.balances())
        #expect(bob.notes() == alice.notes())
        #expect(bob.engine.role(forVault: vaultId) == .editor)
        #expect(bob.engine.isOwner(ofVault: vaultId) == false)

        let members = try await alice.engine.members(ofVault: vaultId)
        #expect(members.map(\.username) == ["alice", "bob"])
        #expect(members.first { $0.username == "bob" }?.role == .editor)
    }

    @Test("Writes made on both sides converge after two syncs each")
    func interleavedWritesConverge() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let vaultId = try #require(alice.vaultId)
        let bob = try await Peer(server: server)
        await bob.engine.register(username: "bob", password: "supersecret")
        try await alice.engine.setMember(vaultId: vaultId, username: "bob", role: .editor)
        await bob.engine.syncNow()

        await alice.store.submit(quickAdd: "-12.50 pizza @Cash >Food")
        await bob.store.submit(quickAdd: "-3.00 milk @Cash >Food")

        await alice.engine.syncNow()
        await bob.engine.syncNow()
        await alice.engine.syncNow()
        await bob.engine.syncNow()

        #expect(alice.notes() == bob.notes())
        #expect(alice.notes().contains("pizza"))
        #expect(alice.notes().contains("milk"))
        #expect(alice.balances() == bob.balances())
        #expect(try await alice.syncState().lastServerSeq == (try await bob.syncState().lastServerSeq))
        #expect(try await alice.syncState().outbox == 0)
        #expect(try await bob.syncState().outbox == 0)
        // The server holds the same projection as both clients.
        let served = try await server.snapshot(ofVault: vaultId)
        let serverBalances = Dictionary(
            uniqueKeysWithValues: served.wallets.map { ("wallet:\($0.name)", $0.balance) }
                + served.flows.map { ("flow:\($0.name)", $0.balance) }
        )
        #expect(serverBalances == alice.balances())
    }

    @Test("A command the server refuses is listed, dropped from the table, and can be dismissed")
    func rejectedCommandSurfacesAndDismisses() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let vaultId = try #require(alice.vaultId)
        let bob = try await Peer(server: server)
        await bob.engine.register(username: "bob", password: "supersecret")
        try await alice.engine.setMember(vaultId: vaultId, username: "bob", role: .editor)
        await bob.engine.syncNow()

        // Both spend 40.00 out of an envelope holding 50.00: each is fine
        // locally, but the second one to reach the server is not.
        await alice.store.submit(quickAdd: "-40.00 alice-dinner @Cash >Food")
        await bob.store.submit(quickAdd: "-40.00 bob-dinner @Cash >Food")
        #expect(bob.notes().contains("bob-dinner"))

        await alice.engine.syncNow()
        await bob.engine.syncNow()

        let refused = try #require(bob.engine.rejected.first)
        #expect(bob.engine.rejected.count == 1)
        #expect(refused.command.code == "insufficient_funds")
        #expect(refused.vaultId == vaultId)
        #expect(refused.summary == ErrorMessages.summary(for: "insufficient_funds"))
        #expect(bob.engine.showsRejectedAlert)
        #expect(bob.notes().contains("bob-dinner") == false)
        #expect(bob.notes().contains("alice-dinner"))
        #expect(try await bob.syncState().rejected == 1)

        await bob.engine.dismiss(refused)
        #expect(bob.engine.rejected.isEmpty)
        #expect(try await bob.syncState().rejected == 0)

        // Alice never sees it: the server never logged it.
        #expect(alice.engine.rejected.isEmpty)
    }

    @Test("An unreachable server shows as offline and the next sync recovers")
    func offlineThenRecovers() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let peer = try await Self.alice(server)

        await server.setOffline(true)
        await peer.store.submit(quickAdd: "-5.00 bus @Cash >Food")
        await peer.engine.syncNow()

        #expect(peer.engine.status == .offline)
        #expect(peer.engine.pendingCount == 1)
        #expect(try await peer.syncState().outbox == 1)

        await server.setOffline(false)
        await peer.engine.syncNow()

        #expect(peer.engine.status == .idle)
        #expect(peer.engine.pendingCount == 0)
        #expect(try await peer.syncState().outbox == 0)
        #expect(peer.notes().contains("bus"))
    }

    @Test("A viewer cannot push, and the failure does not lose the change")
    func viewerCannotPush() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let vaultId = try #require(alice.vaultId)
        let carol = try await Peer(server: server)
        await carol.engine.register(username: "carol", password: "supersecret")
        try await alice.engine.setMember(vaultId: vaultId, username: "carol", role: .viewer)
        await carol.engine.syncNow()

        #expect(carol.store.currentVault?.id == vaultId)
        await carol.store.submit(quickAdd: "-1.00 gum @Cash >Food")
        await carol.engine.syncNow()

        #expect(carol.engine.status != .idle)
        #expect(try await carol.syncState().outbox == 1)
        #expect(carol.engine.pendingCount == 1)
    }

    @Test("Logging out drops the token and stops signing commands as the account")
    func logOutRestoresTheLocalAuthor() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let peer = try await Self.alice(server)

        await peer.engine.logOut()

        #expect(peer.account.isLoggedIn == false)
        #expect(peer.account.token == nil)
        #expect(peer.account.lastUsername == "alice")
        #expect(peer.engine.serverVaults.isEmpty)

        await peer.store.submit(quickAdd: "-2.00 water @Cash >Food")
        #expect(peer.store.transactions.first { $0.note == "water" }?.createdBy == AccountStore.systemAuthor)

        // A sync while logged out is a no-op, not an error.
        await peer.engine.syncNow()
        #expect(peer.engine.status == .idle)
    }

    @Test("Bad credentials are reported and nothing is signed in")
    func failedLoginIsReported() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let peer = try await Peer(server: server)
        await Self.seed(peer)

        await peer.engine.logIn(username: "ghost", password: "supersecret")

        #expect(peer.account.isLoggedIn == false)
        #expect(peer.engine.authMessage != nil)
        #expect(try await peer.syncState().outbox > 0)
    }

    // MARK: - Renaming and deleting a vault

    @Test("A rename reaches the editor and the server's listing at the next sync")
    func renameReachesTheEditor() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let vaultId = try #require(alice.vaultId)
        let bob = try await Peer(server: server)
        await bob.engine.register(username: "bob", password: "supersecret")
        try await alice.engine.setMember(vaultId: vaultId, username: "bob", role: .editor)
        await bob.engine.syncNow()

        await alice.store.renameVault(vaultId, name: "Casa")
        await alice.engine.syncNow()
        await bob.engine.syncNow()

        #expect(alice.store.currentVault?.name == "Casa")
        #expect(bob.store.currentVault?.name == "Casa")
        #expect(bob.engine.serverVaults.first?.name == "Casa")
        #expect(try await server.vault(ofVault: vaultId)?.name == "Casa")
        #expect(try await alice.syncState().outbox == 0)
    }

    @Test("Deleting a vault empties the window, and the next sync deletes it on the server too")
    func deletionReachesTheServer() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let peer = try await Self.alice(server)
        let vaultId = try #require(peer.vaultId)

        await peer.store.deleteVault(vaultId)
        #expect(peer.store.presentedError == nil)
        #expect(peer.store.needsOnboarding)
        #expect(peer.store.currentVault == nil)
        // The deletion waits in the outbox, and is counted even though the
        // vault is no longer listed.
        await peer.engine.refreshLocalState()
        #expect(peer.engine.pendingCount == 1)

        await peer.engine.syncNow()

        #expect(peer.engine.status == .idle)
        #expect(peer.engine.pendingCount == 0)
        #expect(try await peer.core.syncState(vaultId: vaultId).outbox == 0)
        #expect(try await server.vault(ofVault: vaultId) == nil)
        #expect(peer.engine.serverVaults.isEmpty)
        // Nothing came back: with no vault listed there is nothing to join.
        #expect(peer.store.currentVault == nil)
        #expect(peer.store.needsOnboarding)
    }

    @Test("The owner's deletion reaches the editor, whose pending row is refused")
    func deletionReachesTheEditor() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let vaultId = try #require(alice.vaultId)
        let bob = try await Peer(server: server)
        await bob.engine.register(username: "bob", password: "supersecret")
        try await alice.engine.setMember(vaultId: vaultId, username: "bob", role: .editor)
        await bob.engine.syncNow()
        #expect(bob.store.currentVault?.id == vaultId)

        // bob may not delete it himself: the app does not offer it, and the
        // core refuses before the server is even asked.
        #expect(bob.engine.mayDeleteVault(vaultId) == false)
        #expect(alice.engine.mayDeleteVault(vaultId))
        await bob.store.deleteVault(vaultId)
        #expect(bob.store.presentedError?.code == "forbidden")
        #expect(bob.store.currentVault?.id == vaultId)
        bob.store.presentedError = nil

        await bob.store.submit(quickAdd: "-3.00 milk @Cash >Food")
        await alice.store.deleteVault(vaultId)
        await alice.engine.syncNow()
        await bob.engine.syncNow()

        #expect(bob.store.currentVault == nil)
        #expect(bob.store.needsOnboarding)
        #expect(bob.store.vaults.isEmpty)
        #expect(bob.engine.status == .idle)
        #expect(try await bob.core.syncState(vaultId: vaultId).outbox == 0)
        // The milk was refused by a server that no longer had the vault, and
        // is still listed under the vault it can no longer name.
        let refused = try #require(bob.engine.rejected.first)
        #expect(bob.engine.rejected.count == 1)
        #expect(refused.command.code == "not_found")
        #expect(refused.vaultId == vaultId)
        #expect(refused.vaultName == CoreActor.deletedVaultName)
        #expect(bob.engine.showsRejectedAlert)

        await bob.engine.dismiss(refused)
        #expect(bob.engine.rejected.isEmpty)
    }

    @Test("A vault deleted while logged out is re-signed at login and deleted on the server")
    func deletedBeforeLoginStillSyncs() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let peer = try await Peer(server: server)
        await Self.seed(peer)
        let vaultId = try #require(peer.vaultId)
        await peer.store.deleteVault(vaultId)
        #expect(peer.store.needsOnboarding)
        #expect(try await peer.core.syncState(vaultId: vaultId).outbox == 4)

        await peer.engine.register(username: "alice", password: "supersecret")

        #expect(peer.engine.status == .idle)
        #expect(try await peer.core.syncState(vaultId: vaultId).outbox == 0)
        #expect(try await server.vault(ofVault: vaultId) == nil)
        #expect(try await server.lastSeq(ofVault: vaultId) == 4)
        #expect(peer.engine.serverVaults.isEmpty)
        #expect(peer.engine.pendingCount == 0)
    }
}
