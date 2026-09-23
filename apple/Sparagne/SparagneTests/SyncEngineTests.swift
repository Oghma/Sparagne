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
    /// Vaults whose push answers `403 forbidden` whatever the role: a role
    /// changed between `GET /vaults` and the push.
    private var refusedPushes: Set<Uuid> = []
    /// Vaults whose pull answers a body the core cannot read.
    private var garbledPulls: Set<Uuid> = []
    /// Seconds of `Retry-After` on every login and register, when set.
    private var rateLimit: Int?
    /// Membership removals answer `500`.
    private var failingRemovals = false

    init(core: CoreHandle) {
        self.core = core
    }

    /// Every request fails the way a missing network does.
    func setOffline(_ value: Bool) {
        offline = value
    }

    func refusePushes(to vaultId: Uuid) {
        refusedPushes.insert(vaultId)
    }

    func garblePulls(of vaultId: Uuid) {
        garbledPulls.insert(vaultId)
    }

    func setRateLimit(retryAfter seconds: Int?) {
        rateLimit = seconds
    }

    func setFailingRemovals(_ value: Bool) {
        failingRemovals = value
    }

    /// Every token of `username` stops working, as when it expires.
    func revokeTokens(of username: String) {
        sessions = sessions.filter { $0.value != username }
    }

    /// How many times a route was called, for "this happens only once".
    func callCount(method: String, path: String) -> Int {
        log.filter { $0 == "\(method) \(path)" }.count
    }

    /// Every request so far, as `METHOD /path`, oldest first.
    func requests() -> [String] {
        log
    }

    func role(of username: String, inVault vaultId: Uuid) -> MemberRole? {
        memberships[vaultId]?[username]
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

        if request.method == "POST", path == "/auth/register" || path == "/auth/login", let rateLimit {
            return Self.failure(429, "too_many_requests", headers: ["Retry-After": "\(rateLimit)"])
        }
        if request.method == "POST", path == "/auth/register" { return try register(request) }
        if request.method == "POST", path == "/auth/login" { return try login(request) }
        if request.method == "POST", path == "/auth/password" { return changePassword(request) }
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

    /// Usernames are trimmed and lowercased, as the real server does.
    private func register(_ request: SyncRequest) throws -> SyncResponse {
        guard let credentials = Self.credentials(request) else { return Self.failure(400, "invalid_request") }
        let username = AccountRules.normalize(username: credentials.username)
        guard AccountRules.isValidUsername(username), AccountRules.isValidPassword(credentials.password) else {
            return Self.failure(400, "invalid_request")
        }
        guard passwords[username] == nil else { return Self.failure(409, "already_exists") }
        passwords[username] = credentials.password
        return try token(for: username, status: 201)
    }

    private func login(_ request: SyncRequest) throws -> SyncResponse {
        guard let credentials = Self.credentials(request) else { return Self.failure(400, "invalid_request") }
        let username = AccountRules.normalize(username: credentials.username)
        guard passwords[username] == credentials.password else { return Self.failure(401, "unauthorized") }
        return try token(for: username, status: 200)
    }

    private struct PasswordBody: Decodable {
        let currentPassword: String
        let newPassword: String
    }

    /// `POST /auth/password`: the asking token stays, every other token of
    /// the account goes.
    private func changePassword(_ request: SyncRequest) -> SyncResponse {
        guard let user = caller(request), let bearer = request.bearer else {
            return Self.failure(401, "unauthorized")
        }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let body = request.body, let change = try? decoder.decode(PasswordBody.self, from: body) else {
            return Self.failure(400, "invalid_request")
        }
        guard passwords[user] == change.currentPassword else { return Self.failure(401, "unauthorized") }
        guard AccountRules.isValidPassword(change.newPassword) else { return Self.failure(400, "invalid_request") }
        passwords[user] = change.newPassword
        sessions = sessions.filter { $0.value != user || $0.key == bearer }
        return SyncResponse(status: 204)
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
            guard Self.createVault(in: commands.first, of: vaultId) != nil else {
                return Self.failure(404, "not_found")
            }
            // The log outlives the projection: a deleted vault's id stays
            // taken, as on the real server.
            guard try core.lastSeq(vaultId: vaultId) == 0 else {
                return Self.failure(404, "not_found")
            }
            memberships[vaultId] = [user: .owner]
        case .some(let role) where !role.canWrite:
            return Self.failure(403, "forbidden")
        default:
            break
        }
        if refusedPushes.contains(vaultId) { return Self.failure(403, "forbidden") }

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
        if garbledPulls.contains(vaultId) { return SyncResponse(status: 200, body: Data(#"{"garbled":true}"#.utf8)) }
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

    /// The owner removes a member; a member other than the owner removes
    /// themself, which is leaving.
    private func removeMember(_ request: SyncRequest, vaultId: Uuid, username: String) throws -> SyncResponse {
        guard let user = caller(request) else { return Self.failure(401, "unauthorized") }
        if failingRemovals { return Self.failure(500, "storage_error") }
        guard let mine = memberships[vaultId]?[user] else { return Self.failure(404, "not_found") }
        guard mine == .owner || username == user else { return Self.failure(403, "forbidden") }
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

    private static func failure(_ status: Int, _ code: String, headers: [String: String] = [:]) -> SyncResponse {
        let body = #"{"error":{"code":"\#(code)","message":"\#(code)"}}"#
        return SyncResponse(status: status, body: Data(body.utf8), headers: headers)
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
    /// The peer's own preferences, where the account keeps the roles.
    let defaults: UserDefaults

    init(server: FakeServerTransport) async throws {
        defaults = try #require(UserDefaults(suiteName: "sparagne.sync.\(UUID().uuidString)"))
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

    @Test("A vault name the account already used on the server is accepted: names are labels")
    func aDuplicateVaultNameIsAccepted() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        _ = try await Self.alice(server)

        // A second machine, same account, a local vault with the same name:
        // the push creates a second vault, not a conflict.
        let other = try await Peer(server: server)
        await Self.seed(other)
        let twin = try #require(other.vaultId)
        await other.engine.logIn(username: "alice", password: "supersecret")

        #expect(other.account.isLoggedIn)
        #expect(try await other.core.syncState(vaultId: twin).outbox == 0)
        #expect(try await other.core.syncState(vaultId: twin).lastServerSeq > 0)
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

    @Test("A viewer's vault is read-only here too: the write is refused before it enters the log")
    func viewerCannotWrite() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let vaultId = try #require(alice.vaultId)
        let carol = try await Peer(server: server)
        await carol.engine.register(username: "carol", password: "supersecret")
        try await alice.engine.setMember(vaultId: vaultId, username: "carol", role: .viewer)
        await carol.engine.syncNow()

        #expect(carol.store.currentVault?.id == vaultId)
        #expect(carol.store.isReadOnly)
        await carol.store.submit(quickAdd: "-1.00 gum @Cash >Food")

        #expect(carol.store.presentedError?.code == "forbidden")
        #expect(try await carol.syncState().outbox == 0)
        await carol.engine.syncNow()
        #expect(carol.engine.status == .idle)
        #expect(carol.engine.pendingCount == 0)
        #expect(await server.callCount(method: "POST", path: "/vaults/\(vaultId)/push") == 1)
    }

    @Test("A vault whose role became viewer skips the push, refuses its outbox and keeps pulling")
    func viewerSkipsThePush() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let vaultId = try #require(alice.vaultId)
        let carol = try await Peer(server: server)
        await carol.engine.register(username: "carol", password: "supersecret")
        try await alice.engine.setMember(vaultId: vaultId, username: "carol", role: .editor)
        await carol.engine.syncNow()

        // Written while still an editor, not sent before the demotion.
        await carol.store.submit(quickAdd: "-1.00 gum @Cash >Food")
        #expect(try await carol.syncState().outbox == 1)
        try await alice.engine.setMember(vaultId: vaultId, username: "carol", role: .viewer)
        await alice.store.renameVault(vaultId, name: "Casa")
        await alice.engine.syncNow()
        let pushes = await server.callCount(method: "POST", path: "/vaults/\(vaultId)/push")

        await carol.engine.syncNow()

        #expect(await server.callCount(method: "POST", path: "/vaults/\(vaultId)/push") == pushes)
        #expect(try await carol.syncState().outbox == 0)
        #expect(try await carol.syncState().rejected == 1)
        #expect(carol.engine.rejected.first?.command.code == "forbidden")
        #expect(carol.engine.showsRejectedAlert)
        #expect(carol.engine.status == .idle)
        // The pull still ran: the owner's rename is here, the gum is not.
        #expect(carol.store.currentVault?.name == "Casa")
        #expect(carol.notes().contains("gum") == false)
        #expect(carol.store.isReadOnly)
    }

    @Test("A push refused with 403 refuses the outbox locally, and the vault still pulls")
    func refusedPushStillPulls() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let vaultId = try #require(alice.vaultId)
        let bob = try await Peer(server: server)
        await bob.engine.register(username: "bob", password: "supersecret")
        try await alice.engine.setMember(vaultId: vaultId, username: "bob", role: .editor)
        await bob.engine.syncNow()

        await bob.store.submit(quickAdd: "-3.00 milk @Cash >Food")
        await alice.store.renameVault(vaultId, name: "Casa")
        await alice.engine.syncNow()
        await server.refusePushes(to: vaultId)
        await bob.engine.syncNow()

        #expect(try await bob.syncState().outbox == 0)
        #expect(try await bob.syncState().rejected == 1)
        let refused = try #require(bob.engine.rejected.first)
        #expect(refused.command.code == "forbidden")
        #expect(refused.command.message == SyncEngine.readOnlyMessage)
        #expect(refused.kindName == CommandKindNames.name(for: "expense"))
        #expect(bob.engine.showsRejectedAlert)
        #expect(bob.engine.status == .idle)
        #expect(bob.store.currentVault?.name == "Casa")
        #expect(bob.notes().contains("milk") == false)

        // Nothing is retried: the next round pushes nothing.
        let pushes = await server.callCount(method: "POST", path: "/vaults/\(vaultId)/push")
        await bob.engine.syncNow()
        #expect(await server.callCount(method: "POST", path: "/vaults/\(vaultId)/push") == pushes)
    }

    @Test("A core error in one vault does not stop the round for the others")
    func roundContinuesPastAFailingVault() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let peer = try await Peer(server: server)
        await peer.store.createVault(name: "Alpha", walletName: "Cash", openingBalance: 1_000)
        let alpha = try #require(peer.vaultId)
        await peer.store.createVault(name: "Beta", walletName: "Cash", openingBalance: 2_000)
        let beta = try #require(peer.vaultId)
        await peer.engine.register(username: "alice", password: "supersecret")
        #expect(peer.engine.status == .idle)

        // Alpha comes first in the round; its pull now breaks in the core.
        await server.garblePulls(of: alpha)
        await peer.store.submit(quickAdd: "-1.00 tea @Cash")
        #expect(peer.store.currentVault?.id == beta)
        await peer.engine.syncNow()

        #expect(try await peer.core.syncState(vaultId: beta).outbox == 0)
        switch peer.engine.status {
        case .error(let text): #expect(text.contains(ErrorMessages.summary(for: "invalid_command")))
        default: Issue.record("expected an error status, got \(peer.engine.status)")
        }
    }

    @Test("The round asks for the roles first and hands the read-only vaults to the store and the actor")
    func rolesComeFirst() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let vaultId = try #require(alice.vaultId)
        let carol = try await Peer(server: server)
        await carol.engine.register(username: "carol", password: "supersecret")
        try await alice.engine.setMember(vaultId: vaultId, username: "carol", role: .viewer)

        let before = await server.requests().count
        await carol.engine.syncNow()
        let round = Array(await server.requests().dropFirst(before))

        #expect(round.first == "GET /vaults")
        #expect(carol.engine.role(forVault: vaultId) == .viewer)
        #expect(carol.engine.readOnlyVaultIds == [vaultId])
        #expect(carol.store.readOnlyVaultIds == [vaultId])
        // The actor has the same set: a write that skips the store is refused.
        await #expect(throws: DomainError.self) {
            try await carol.core.execute(vaultId: vaultId, .renameVault(name: "Mine"))
        }

        // Remembered for the next launch, offline included.
        let relaunched = AccountStore(defaults: carol.defaults, tokens: MemoryTokenStore())
        #expect(relaunched.vaultRoles[vaultId] == .viewer)
    }

    @Test("Rename, delete and leave are offered exactly when they would go through")
    func permissionTruthTable() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        // Logged out, a local vault: mine to rename and delete, nothing to
        // leave.
        let alice = try await Peer(server: server)
        await Self.seed(alice)
        let vaultId = try #require(alice.vaultId)
        #expect(alice.engine.mayRenameVault(vaultId))
        #expect(alice.engine.mayDeleteVault(vaultId))
        #expect(!alice.engine.mayLeaveVault(vaultId))

        // Logged in and listed as the owner.
        await alice.engine.register(username: "alice", password: "supersecret")
        #expect(alice.engine.role(forVault: vaultId) == .owner)
        #expect(alice.engine.mayRenameVault(vaultId))
        #expect(alice.engine.mayDeleteVault(vaultId))
        #expect(!alice.engine.mayLeaveVault(vaultId))

        // Logged in, not listed yet: a vault created since the last sync.
        await alice.store.createVault(name: "Draft", walletName: "Cash", openingBalance: 0)
        let draft = try #require(alice.vaultId)
        #expect(alice.engine.role(forVault: draft) == nil)
        #expect(alice.engine.mayRenameVault(draft))
        #expect(alice.engine.mayDeleteVault(draft))
        #expect(!alice.engine.mayLeaveVault(draft))

        // An editor and a viewer of alice's vault.
        let bob = try await Peer(server: server)
        await bob.engine.register(username: "bob", password: "supersecret")
        let carol = try await Peer(server: server)
        await carol.engine.register(username: "carol", password: "supersecret")
        try await alice.engine.setMember(vaultId: vaultId, username: "bob", role: .editor)
        try await alice.engine.setMember(vaultId: vaultId, username: "carol", role: .viewer)
        await bob.engine.syncNow()
        await carol.engine.syncNow()

        #expect(bob.engine.mayRenameVault(vaultId))
        #expect(!bob.engine.mayDeleteVault(vaultId))
        #expect(bob.engine.mayLeaveVault(vaultId))
        #expect(!carol.engine.mayRenameVault(vaultId))
        #expect(!carol.engine.mayDeleteVault(vaultId))
        #expect(carol.engine.mayLeaveVault(vaultId))

        // Logged out, the viewer's vault stays read-only and there is no
        // membership to leave; the editor's vault is not his to delete.
        await carol.engine.logOut()
        await bob.engine.logOut()
        #expect(!carol.engine.mayRenameVault(vaultId))
        #expect(!carol.engine.mayDeleteVault(vaultId))
        #expect(!carol.engine.mayLeaveVault(vaultId))
        #expect(bob.engine.mayRenameVault(vaultId))
        #expect(!bob.engine.mayDeleteVault(vaultId))
        #expect(!bob.engine.mayLeaveVault(vaultId))
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
        // With nothing left to send or to read, the next round lets the
        // deleted vault's log go.
        #expect(try await bob.core.deletedVaults() == [vaultId])
        await bob.engine.syncNow()
        #expect(try await bob.core.deletedVaults().isEmpty)
        #expect(bob.store.vaults.isEmpty)
    }

    @Test("A deleted vault leaves the device once its deletion is confirmed")
    func deletedVaultIsForgotten() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let vaultId = try #require(alice.vaultId)
        let bob = try await Peer(server: server)
        await bob.engine.register(username: "bob", password: "supersecret")
        try await alice.engine.setMember(vaultId: vaultId, username: "bob", role: .editor)
        await bob.engine.syncNow()

        await alice.store.deleteVault(vaultId)
        #expect(try await alice.core.deletedVaults() == [vaultId])
        await alice.engine.syncNow()
        await bob.engine.syncNow()

        // Both logs are gone, and nothing joins the vault back.
        for peer in [alice, bob] {
            #expect(try await peer.core.deletedVaults().isEmpty)
            #expect(try await peer.core.syncState(vaultId: vaultId).lastServerSeq == 0)
            #expect(peer.engine.role(forVault: vaultId) == nil)
            #expect(peer.engine.status == .idle)
        }
        await alice.engine.syncNow()
        #expect(alice.store.vaults.isEmpty)
        #expect(try await alice.core.syncState(vaultId: vaultId).lastServerSeq == 0)
    }

    @Test("A vault no longer shared is kept read-only and marked, never dropped behind the user's back")
    func lostAccessIsMarked() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let vaultId = try #require(alice.vaultId)
        let bob = try await Peer(server: server)
        await bob.engine.register(username: "bob", password: "supersecret")
        try await alice.engine.setMember(vaultId: vaultId, username: "bob", role: .editor)
        await bob.engine.syncNow()

        await bob.store.submit(quickAdd: "-3.00 milk @Cash >Food")
        try await alice.engine.removeMember(vaultId: vaultId, username: "bob")
        await bob.engine.syncNow()

        #expect(bob.store.vaults.map(\.id) == [vaultId])
        #expect(bob.engine.hasLostAccess(to: vaultId))
        #expect(bob.engine.lostVaults.map(\.id) == [vaultId])
        #expect(bob.store.readOnlyVaultIds.contains(vaultId))
        #expect(bob.store.isReadOnly)
        #expect(!bob.engine.mayRenameVault(vaultId))
        #expect(!bob.engine.mayDeleteVault(vaultId))
        #expect(!bob.engine.mayLeaveVault(vaultId))
        #expect(bob.engine.status == .idle)
        // The milk could never go up: it is refused, readable, not pending.
        #expect(try await bob.syncState().outbox == 0)
        #expect(bob.engine.rejected.first?.command.code == "not_found")
        #expect(bob.engine.rejected.first?.command.message == SyncEngine.noLongerSharedMessage)

        // Later rounds leave it alone.
        let pulls = await server.callCount(method: "GET", path: "/vaults/\(vaultId)/pull")
        await bob.engine.syncNow()
        #expect(await server.callCount(method: "GET", path: "/vaults/\(vaultId)/pull") == pulls)
        #expect(bob.store.vaults.map(\.id) == [vaultId])

        try await bob.engine.removeFromThisMac(vaultId)
        #expect(bob.store.vaults.isEmpty)
        #expect(bob.store.needsOnboarding)
        #expect(try await bob.core.vaults().isEmpty)
        #expect(!bob.engine.hasLostAccess(to: vaultId))
        #expect(bob.store.readOnlyVaultIds.isEmpty)
    }

    @Test("Shared again, a lost vault is writable again")
    func regainedAccess() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let vaultId = try #require(alice.vaultId)
        let bob = try await Peer(server: server)
        await bob.engine.register(username: "bob", password: "supersecret")
        try await alice.engine.setMember(vaultId: vaultId, username: "bob", role: .editor)
        await bob.engine.syncNow()
        try await alice.engine.removeMember(vaultId: vaultId, username: "bob")
        await bob.engine.syncNow()
        #expect(bob.engine.hasLostAccess(to: vaultId))

        try await alice.engine.setMember(vaultId: vaultId, username: "bob", role: .editor)
        await bob.engine.syncNow()

        #expect(!bob.engine.hasLostAccess(to: vaultId))
        #expect(!bob.store.isReadOnly)
        await bob.store.submit(quickAdd: "-3.00 milk @Cash >Food")
        await bob.engine.syncNow()
        #expect(try await bob.syncState().outbox == 0)
        await alice.engine.syncNow()
        #expect(alice.notes().contains("milk"))
    }

    // MARK: - Leaving a vault

    @Test("Leaving sends what is pending, drops the membership and the local copy")
    func leaveVault() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let vaultId = try #require(alice.vaultId)
        let bob = try await Peer(server: server)
        await bob.engine.register(username: "bob", password: "supersecret")
        try await alice.engine.setMember(vaultId: vaultId, username: "bob", role: .editor)
        await bob.engine.syncNow()
        #expect(bob.engine.mayLeaveVault(vaultId))

        await bob.store.submit(quickAdd: "-3.00 milk @Cash >Food")
        try await bob.engine.leaveVault(vaultId)

        #expect(await server.role(of: "bob", inVault: vaultId) == nil)
        #expect(bob.store.vaults.isEmpty)
        #expect(bob.store.needsOnboarding)
        #expect(try await bob.core.vaults().isEmpty)
        #expect(try await bob.core.syncState(vaultId: vaultId).lastServerSeq == 0)
        #expect(bob.engine.role(forVault: vaultId) == nil)
        #expect(bob.engine.serverVaults.isEmpty)
        #expect(bob.engine.pendingCount == 0)

        // The milk went up before bob left; the vault does not come back.
        await alice.engine.syncNow()
        #expect(alice.notes().contains("milk"))
        await bob.engine.syncNow()
        #expect(bob.store.vaults.isEmpty)
        #expect(bob.engine.status == .idle)
    }

    @Test("A leave the server refuses forgets nothing and says why")
    func failedLeaveKeepsTheVault() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let vaultId = try #require(alice.vaultId)
        let bob = try await Peer(server: server)
        await bob.engine.register(username: "bob", password: "supersecret")
        try await alice.engine.setMember(vaultId: vaultId, username: "bob", role: .viewer)
        await bob.engine.syncNow()

        await server.setFailingRemovals(true)
        await #expect(throws: ServerError.self) {
            try await bob.engine.leaveVault(vaultId)
        }

        #expect(await server.role(of: "bob", inVault: vaultId) == .viewer)
        #expect(bob.store.vaults.map(\.id) == [vaultId])
        #expect(bob.engine.role(forVault: vaultId) == .viewer)
        #expect(try await bob.syncState().lastServerSeq > 0)

        // The owner cannot leave their own vault: the server says so.
        await server.setFailingRemovals(false)
        #expect(!alice.engine.mayLeaveVault(vaultId))
        await #expect(throws: ServerError.self) {
            try await alice.engine.leaveVault(vaultId)
        }
        #expect(alice.store.vaults.map(\.id) == [vaultId])
    }

    // MARK: - Password and session

    @Test("Changing the password keeps this session and signs the other devices out")
    func changePassword() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        let laptop = try await Peer(server: server)
        await laptop.engine.logIn(username: "alice", password: "supersecret")
        #expect(laptop.account.isLoggedIn)

        // A wrong current password is refused and ends nothing.
        await #expect(throws: ServerError.self) {
            try await alice.engine.changePassword(current: "wrong-one", new: "brandnewsecret")
        }
        #expect(alice.account.isLoggedIn)

        try await alice.engine.changePassword(current: "supersecret", new: "brandnewsecret")
        await alice.engine.syncNow()
        #expect(alice.account.isLoggedIn)
        #expect(alice.engine.status == .idle)

        await laptop.engine.syncNow()
        #expect(!laptop.account.isLoggedIn)
        #expect(laptop.account.lastUsername == "alice")
        #expect(laptop.account.sessionExpired)
        #expect(laptop.engine.status == .error(ErrorMessages.sessionExpired))

        await laptop.engine.logIn(username: "alice", password: "supersecret")
        #expect(!laptop.account.isLoggedIn)
        #expect(laptop.engine.authMessage == ErrorMessages.wrongCredentials)
        await laptop.engine.logIn(username: "alice", password: "brandnewsecret")
        #expect(laptop.account.isLoggedIn)
        #expect(!laptop.account.sessionExpired)
    }

    @Test("A 401 during a round signs out, keeps the name and says the session expired")
    func expiredSessionDuringSync() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let alice = try await Self.alice(server)
        #expect(alice.account.expiresAt == Date(timeIntervalSince1970: 4_102_444_800))

        await server.revokeTokens(of: "alice")
        await alice.store.submit(quickAdd: "-5.00 bus @Cash >Food")
        await alice.engine.syncNow()

        #expect(!alice.account.isLoggedIn)
        #expect(alice.account.lastUsername == "alice")
        #expect(alice.engine.status == .error(ErrorMessages.sessionExpired))
        #expect(try await alice.syncState().outbox == 1)
    }

    @Test("A rate-limited login says how many minutes to wait")
    func rateLimitedLogin() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let peer = try await Peer(server: server)
        await server.setRateLimit(retryAfter: 125)

        await peer.engine.logIn(username: "alice", password: "supersecret")

        #expect(!peer.account.isLoggedIn)
        #expect(peer.engine.authMessage == ErrorMessages.tooManyAttempts(retryAfter: 125))
        #expect(peer.engine.authMessage == String(localized: "Too many attempts. Try again in \(3) minutes."))
        #expect(peer.engine.authDetail == nil)
    }

    @Test("Register trims and lowercases the name the server gets")
    func registerNormalizesTheName() async throws {
        let server = FakeServerTransport(core: try CoreHandle.openInMemory())
        let peer = try await Peer(server: server)

        await peer.engine.register(username: "  Alice ", password: "supersecret")

        #expect(peer.account.username == "alice")
        let other = try await Peer(server: server)
        await other.engine.logIn(username: "ALICE", password: "supersecret")
        #expect(other.account.username == "alice")
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
