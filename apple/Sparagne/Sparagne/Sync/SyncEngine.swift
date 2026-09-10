import Foundation
import Observation
import SparagneCore

/// Runs the client half of the sync protocol (`docs/v2/SYNC.md` §4-§5).
///
/// For every local vault: push what the outbox holds, then pull until the
/// local watermark reaches the server's last seq; then join whatever vault the
/// server lists and the database does not have yet. The core does all the
/// reasoning — the engine only moves opaque JSON between `CoreHandle` and
/// `ServerAPI` and keeps the status the window shows.
///
/// Nothing here throws out of the UI: a failure becomes `status`.
@Observable
@MainActor
final class SyncEngine {
    /// What the toolbar shows.
    enum Status: Equatable, Sendable {
        case idle
        case syncing
        case offline
        case error(String)
    }

    /// One rejected command with the vault it belongs to, so the sheet can
    /// dismiss it.
    struct RejectedChange: Identifiable, Equatable, Sendable {
        let vaultId: Uuid
        let vaultName: String
        let command: RejectedCommand

        var id: Uuid { command.commandId }
        /// The localized headline for the server's code.
        var summary: String { ErrorMessages.summary(for: command.code) }
    }

    static let pullLimit = 500
    /// A pull loop that neither finishes nor stalls is a bug; stop rather
    /// than hammer the server.
    private static let maxPullPages = 1_000
    static let debounce: Duration = .seconds(2)
    static let interval: Duration = .seconds(60)

    // MARK: Dependencies

    @ObservationIgnored private let client: CoreClient
    @ObservationIgnored private let store: AppStore
    @ObservationIgnored private let makeTransport: @Sendable (URL) -> SyncTransport

    let account: AccountStore

    /// Off in tests, so no timer or debounce fires behind an assertion.
    @ObservationIgnored var automaticSync = true

    // MARK: State

    private(set) var status: Status = .idle
    private(set) var lastSyncAt: Date?
    /// Commands applied locally and not yet confirmed, over every vault.
    private(set) var pendingCount = 0
    /// Everything the core is still holding as rejected, newest sync first.
    private(set) var rejected: [RejectedChange] = []
    /// The vault list from the last `GET /vaults`, which is what says whether
    /// I own a vault.
    private(set) var serverVaults: [VaultSummary] = []
    /// What went wrong in the last login, register or logout.
    private(set) var authMessage: String?
    /// Raised once by a sync that produced new rejections; the window lowers
    /// it when it has shown its alert.
    var showsRejectedAlert = false

    @ObservationIgnored private var isSyncing = false
    @ObservationIgnored private var debounceTask: Task<Void, Never>?
    @ObservationIgnored private var timerTask: Task<Void, Never>?

    init(
        client: CoreClient,
        store: AppStore,
        account: AccountStore,
        makeTransport: @escaping @Sendable (URL) -> SyncTransport = { URLSessionTransport(baseURL: $0) }
    ) {
        self.client = client
        self.store = store
        self.account = account
        self.makeTransport = makeTransport
        client.author = account.author
        client.onExecuted = { [weak self] in self?.scheduleSync() }
        refreshLocalState()
    }

    // MARK: - Derived

    var isLoggedIn: Bool { account.isLoggedIn }

    /// The role the server gave me on a vault, from the last `GET /vaults`.
    func role(forVault vaultId: Uuid) -> MemberRole? {
        serverVaults.first { $0.id == vaultId }?.role
    }

    func isOwner(ofVault vaultId: Uuid) -> Bool {
        role(forVault: vaultId) == .owner
    }

    // MARK: - Triggers

    /// The initial sync plus the 60 s loop. Safe to call more than once.
    func start() {
        guard automaticSync, timerTask == nil else { return }
        timerTask = Task { [weak self] in
            while !Task.isCancelled {
                guard let engine = self else { return }
                await engine.syncNow()
                try? await Task.sleep(for: Self.interval)
            }
        }
    }

    func stop() {
        timerTask?.cancel()
        timerTask = nil
        debounceTask?.cancel()
        debounceTask = nil
    }

    /// Called after every command; the last one in a burst wins.
    func scheduleSync() {
        pendingCount = localPendingCount()
        guard automaticSync, account.isLoggedIn else { return }
        debounceTask?.cancel()
        debounceTask = Task { [weak self] in
            try? await Task.sleep(for: Self.debounce)
            guard !Task.isCancelled else { return }
            await self?.syncNow()
        }
    }

    // MARK: - Sync

    /// One full round: push, pull, join. Never runs twice at the same time
    /// and never throws.
    func syncNow() async {
        guard !isSyncing else { return }
        guard account.isLoggedIn else {
            refreshLocalState()
            return
        }
        let api: ServerAPI
        let token: String
        do {
            (api, token) = try authorized()
        } catch {
            settle(failure: asServerError(error), changed: false, rejections: false)
            return
        }

        isSyncing = true
        status = .syncing
        var changed = false
        var rejections = false
        var failure: ServerError?

        do {
            for vault in try client.vaults() {
                do {
                    let outcome = try await syncVault(vault.id, api: api, token: token)
                    changed = changed || outcome.changed
                    rejections = rejections || outcome.rejected
                } catch let error as ServerError where !error.isOffline {
                    // A vault I may not write to, or one the server never
                    // heard of, must not stop the others.
                    failure = failure ?? error
                }
            }
            serverVaults = try await api.vaults(token: token)
            let known = Set(try client.vaults().map(\.id))
            for summary in serverVaults where !known.contains(summary.id) {
                do {
                    let outcome = try await pullLoop(summary.id, api: api, token: token)
                    changed = changed || outcome.changed || outcome.received
                    rejections = rejections || outcome.rejected
                } catch let error as ServerError where !error.isOffline {
                    failure = failure ?? error
                }
            }
        } catch let error {
            failure = asServerError(error)
        }

        isSyncing = false
        settle(failure: failure, changed: changed, rejections: rejections)
    }

    /// One vault: the `CreateVault` special case, the push, then the pull.
    private func syncVault(_ vaultId: Uuid, api: ServerAPI, token: String) async throws -> Outcome {
        var outcome = Outcome()
        var state = try client.syncState(vaultId: vaultId)

        // A vault the server has never seen starts with its own creation,
        // which goes to `POST /vaults` because that is what also writes the
        // owner membership (`docs/v2/SYNC.md` §3).
        if state.lastServerSeq == 0 {
            let body = try client.pushRequestJson(vaultId: vaultId)
            if let envelope = Self.createVaultEnvelope(inPushBody: body) {
                do {
                    let result = try await api.createVault(token: token, envelopeJSON: envelope)
                    outcome.absorb(
                        try client.applyPushResponse(vaultId: vaultId, json: result.pushResponse)
                    )
                } catch let error as ServerError where error.status == 409 {
                    // Already on the server: the ordinary push is idempotent
                    // and will confirm the command anyway.
                }
                state = try client.syncState(vaultId: vaultId)
            }
        }

        if state.outbox > 0 {
            let body = try client.pushRequestJson(vaultId: vaultId)
            let response = try await api.push(token: token, vaultId: vaultId, body: body)
            outcome.absorb(try client.applyPushResponse(vaultId: vaultId, json: response))
        }

        outcome.merge(try await pullLoop(vaultId, api: api, token: token))
        return outcome
    }

    /// Pulls pages from the contiguous watermark until it reaches the
    /// server's last seq, or stops advancing.
    private func pullLoop(_ vaultId: Uuid, api: ServerAPI, token: String) async throws -> Outcome {
        var outcome = Outcome()
        for _ in 0..<Self.maxPullPages {
            let since = try client.syncState(vaultId: vaultId).lastServerSeq
            let page = try await api.pull(
                token: token,
                vaultId: vaultId,
                since: since,
                limit: Self.pullLimit
            )
            outcome.absorb(try client.integratePull(vaultId: vaultId, json: page))
            let reached = try client.syncState(vaultId: vaultId).lastServerSeq
            if reached >= Self.lastSeq(inPullBody: page) || reached <= since { break }
        }
        return outcome
    }

    /// Writes the result of a sync round into the observable state.
    private func settle(failure: ServerError?, changed: Bool, rejections: Bool) {
        refreshLocalState()
        if changed { store.refreshAfterSync() }
        if let failure {
            if failure.isUnauthorized { account.signOut() }
            status = failure.isOffline ? .offline : .error(describe(failure))
        } else {
            status = .idle
            lastSyncAt = Date()
        }
        if rejections && !rejected.isEmpty { showsRejectedAlert = true }
    }

    /// Re-reads the outbox count and the rejected commands from the core.
    func refreshLocalState() {
        var pending = 0
        var list: [RejectedChange] = []
        for vault in (try? client.vaults()) ?? [] {
            if let state = try? client.syncState(vaultId: vault.id) {
                pending += Int(state.outbox)
            }
            for command in (try? client.rejectedCommands(vaultId: vault.id)) ?? [] {
                list.append(
                    RejectedChange(vaultId: vault.id, vaultName: vault.name, command: command)
                )
            }
        }
        pendingCount = pending
        rejected = list
    }

    private func localPendingCount() -> Int {
        ((try? client.vaults()) ?? []).reduce(0) { total, vault in
            total + Int((try? client.syncState(vaultId: vault.id).outbox) ?? 0)
        }
    }

    // MARK: - Rejections

    func dismiss(_ change: RejectedChange) {
        try? client.dismissRejected(vaultId: change.vaultId, commandId: change.command.commandId)
        refreshLocalState()
    }

    func dismissAllRejected() {
        for change in rejected {
            try? client.dismissRejected(
                vaultId: change.vaultId,
                commandId: change.command.commandId
            )
        }
        refreshLocalState()
    }

    // MARK: - Account

    func register(username: String, password: String) async {
        await authenticate(username: username, password: password, registering: true)
    }

    func logIn(username: String, password: String) async {
        await authenticate(username: username, password: password, registering: false)
    }

    /// The username becomes the author of every command, so the outbox has to
    /// be relabelled before it is pushed (`docs/v2/SYNC.md` §4.4).
    private func authenticate(username: String, password: String, registering: Bool) async {
        authMessage = nil
        guard let url = account.baseURL else {
            authMessage = describe(.notConfigured)
            return
        }
        let api = ServerAPI(transport: makeTransport(url))
        do {
            let response =
                registering
                ? try await api.register(username: username, password: password)
                : try await api.login(username: username, password: password)
            account.signIn(username: response.username, token: response.token)
            client.author = response.username
            for vault in try client.vaults() {
                try client.relabelOutbox(vaultId: vault.id, author: response.username)
            }
            store.refreshAfterSync()
            status = .idle
            await syncNow()
            start()
        } catch {
            authMessage = describe(asServerError(error))
        }
    }

    func logOut() async {
        authMessage = nil
        if let session = try? authorized() {
            _ = try? await session.api.logout(token: session.token)
        }
        stop()
        account.signOut()
        client.author = AccountStore.localAuthor
        serverVaults = []
        status = .idle
        lastSyncAt = nil
    }

    // MARK: - Sharing

    func members(ofVault vaultId: Uuid) async throws -> [MemberEntry] {
        let (api, token) = try authorized()
        return try await api.members(token: token, vaultId: vaultId)
    }

    func setMember(vaultId: Uuid, username: String, role: MemberRole) async throws {
        let (api, token) = try authorized()
        try await api.setMember(token: token, vaultId: vaultId, username: username, role: role)
    }

    func removeMember(vaultId: Uuid, username: String) async throws {
        let (api, token) = try authorized()
        try await api.removeMember(token: token, vaultId: vaultId, username: username)
    }

    // MARK: - Plumbing

    private func authorized() throws -> (api: ServerAPI, token: String) {
        guard let url = account.baseURL else { throw ServerError.notConfigured }
        guard let token = account.token else { throw ServerError.notLoggedIn }
        return (ServerAPI(transport: makeTransport(url)), token)
    }

    /// Anything that is not already a `ServerError` (a `DomainError` from the
    /// core, say) still has to reach the status line.
    private func asServerError(_ error: some Error) -> ServerError {
        switch error {
        case let error as ServerError: error
        case let error as DomainError: ServerError(status: 0, code: error.code, message: error.message)
        default: ServerError(status: 0, code: "unexpected", message: error.localizedDescription)
        }
    }

    /// Headline plus the server's own words, when it wrote any.
    private func describe(_ error: ServerError) -> String {
        error.message.isEmpty ? error.summary : "\(error.summary): \(error.message)"
    }

    /// What one round of push or pull did to a vault.
    private struct Outcome {
        var changed = false
        var received = false
        var rejected = false

        mutating func absorb(_ report: SyncReport) {
            changed = changed || report.confirmed > 0 || report.received > 0 || report.rebased
            received = received || report.received > 0
            rejected = rejected || !report.rejected.isEmpty
        }

        mutating func merge(_ other: Outcome) {
            changed = changed || other.changed
            received = received || other.received
            rejected = rejected || other.rejected
        }
    }

    // MARK: - Reading the opaque JSON

    /// The first command of a push body when it is the vault's own creation,
    /// as the envelope `POST /vaults` wants. Read with `JSONSerialization`:
    /// Swift never decodes a command.
    static func createVaultEnvelope(inPushBody body: String) -> Data? {
        guard
            let root = try? JSONSerialization.jsonObject(with: Data(body.utf8)) as? [String: Any],
            let commands = root["commands"] as? [[String: Any]],
            let first = commands.first,
            let command = first["command"] as? [String: Any],
            command["kind"] as? String == "create_vault"
        else { return nil }
        return try? JSONSerialization.data(withJSONObject: first)
    }

    /// The `last_seq` of a pull response, which says whether another page is
    /// waiting.
    static func lastSeq(inPullBody body: String) -> Int64 {
        guard
            let root = try? JSONSerialization.jsonObject(with: Data(body.utf8)) as? [String: Any],
            let last = root["last_seq"] as? NSNumber
        else { return 0 }
        return last.int64Value
    }
}
