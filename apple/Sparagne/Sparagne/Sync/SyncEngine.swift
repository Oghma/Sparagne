import Foundation
import Observation
import SparagneCore

/// Runs the client half of the sync protocol (`docs/v2/SYNC.md` §4-§5).
///
/// For every local vault: push what the outbox holds, a batch per request,
/// then pull until the core says the server has nothing more; then join
/// whatever vault the server lists and the database does not have yet. The
/// core does all the reasoning — the engine only moves opaque JSON between
/// `CoreActor` and `ServerAPI` and keeps the status the window shows. It
/// shares that one actor with `AppStore`, so the core is never touched from
/// two isolation domains at once.
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
        let entry: RejectedEntry

        var vaultId: Uuid { entry.vaultId }
        var vaultName: String { entry.vaultName }
        var command: RejectedCommand { entry.command }

        var id: Uuid { command.commandId }
        /// The localized headline for the server's code.
        var summary: String { ErrorMessages.summary(for: command.code) }
    }

    static let pullLimit = 500
    /// How many commands go up in one `POST /push`. The core hands out the
    /// outbox in slices of this size (`docs/v2/SYNC.md` §4.1).
    static let pushLimit: UInt32 = 500
    /// A push or pull loop that neither finishes nor stalls is a bug; stop
    /// rather than hammer the server.
    private static let maxPages = 1_000
    static let debounce: Duration = .seconds(2)
    static let interval: Duration = .seconds(60)

    // MARK: Dependencies

    @ObservationIgnored private let core: CoreActor
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
        core: CoreActor,
        store: AppStore,
        account: AccountStore,
        makeTransport: @escaping @Sendable (URL) -> SyncTransport = { URLSessionTransport(baseURL: $0) }
    ) {
        self.core = core
        self.store = store
        self.account = account
        self.makeTransport = makeTransport
    }

    /// Adopts the account's author, subscribes to applied commands and reads
    /// the local sync state. `init` cannot await the core actor, so this runs
    /// once before the first command, ahead of `start()`.
    func prepare() async {
        await store.setAuthor(account.author)
        await core.setOnExecuted { [weak self] in
            Task { @MainActor in await self?.scheduleSync() }
        }
        await refreshLocalState()
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

    /// Whether a deletion from me would go through: the server refuses one
    /// from anyone but the owner, and so does the core (`docs/v2/SYNC.md`
    /// §3). `true` for a vault the server does not list, which is local-only
    /// or not pushed yet.
    func mayDeleteVault(_ vaultId: Uuid) -> Bool {
        role(forVault: vaultId).map { $0 == .owner } ?? true
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
    func scheduleSync() async {
        pendingCount = await core.localState().pending
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
            await refreshLocalState()
            return
        }
        let api: ServerAPI
        let token: String
        do {
            (api, token) = try authorized()
        } catch {
            await settle(failure: asServerError(error), changed: false, rejections: false)
            return
        }

        isSyncing = true
        status = .syncing
        var changed = false
        var rejections = false
        var failure: ServerError?

        do {
            for vault in try await core.vaults() {
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
            // A vault deleted here still owes the server its last commands,
            // the `DeleteVault` among them. Until they are confirmed it syncs
            // like a live one (so a deletion the server already holds, from
            // another device, is folded in as well); afterwards it costs
            // nothing (`docs/v2/SYNC.md` §4.6).
            for vaultId in try await core.deletedVaults() {
                guard try await core.syncState(vaultId: vaultId).outbox > 0 else { continue }
                do {
                    let outcome = try await syncVault(vaultId, api: api, token: token)
                    changed = changed || outcome.changed
                    rejections = rejections || outcome.rejected
                } catch let error as ServerError where !error.isOffline {
                    failure = failure ?? error
                }
            }
            serverVaults = try await api.vaults(token: token)
            // A deleted vault whose deletion is still on its way up may be
            // listed by the server; pulling it would only replay what is
            // already here, so it is not a join either.
            let live = try await core.vaults().map(\.id)
            let known = Set(live + (try await core.deletedVaults()))
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
        await settle(failure: failure, changed: changed, rejections: rejections)
    }

    /// One vault: push the whole outbox, a batch per request, then pull.
    ///
    /// A vault the server has never seen is created by the first push, whose
    /// first command is the `CreateVault` that minted it: there is no separate
    /// route for it (`docs/v2/SYNC.md` §3).
    private func syncVault(_ vaultId: Uuid, api: ServerAPI, token: String) async throws -> Outcome {
        var outcome = Outcome()
        outcome.merge(try await pushLoop(vaultId, api: api, token: token))
        outcome.merge(try await pullLoop(vaultId, api: api, token: token))
        return outcome
    }

    /// Pushes batches until the outbox is empty. A batch that leaves the
    /// outbox no shorter made no progress, so it stops instead of looping:
    /// confirmations and rejections both take commands out of it.
    private func pushLoop(_ vaultId: Uuid, api: ServerAPI, token: String) async throws -> Outcome {
        var outcome = Outcome()
        for _ in 0..<Self.maxPages {
            let pending = try await core.syncState(vaultId: vaultId).outbox
            guard pending > 0 else { break }
            let body = try await core.pushRequestJson(vaultId: vaultId, limit: Self.pushLimit)
            let response = try await api.push(token: token, vaultId: vaultId, body: body)
            outcome.absorb(try await core.applyPushResponse(vaultId: vaultId, json: response))
            if try await core.syncState(vaultId: vaultId).outbox >= pending { break }
        }
        return outcome
    }

    /// Pulls pages from the contiguous watermark while the core reports the
    /// server holds more, or until the watermark stops advancing.
    private func pullLoop(_ vaultId: Uuid, api: ServerAPI, token: String) async throws -> Outcome {
        var outcome = Outcome()
        for _ in 0..<Self.maxPages {
            let since = try await core.syncState(vaultId: vaultId).lastServerSeq
            let page = try await api.pull(
                token: token,
                vaultId: vaultId,
                since: since,
                limit: Self.pullLimit
            )
            let report = try await core.integratePull(vaultId: vaultId, json: page)
            outcome.absorb(report)
            let reached = try await core.syncState(vaultId: vaultId).lastServerSeq
            if !report.hasMore || reached <= since { break }
        }
        return outcome
    }

    /// Writes the result of a sync round into the observable state.
    private func settle(failure: ServerError?, changed: Bool, rejections: Bool) async {
        await refreshLocalState()
        if changed { await store.refreshAfterSync() }
        if let failure {
            if failure.isUnauthorized { account.signOut() }
            status = failure.isOffline ? .offline : .error(describe(failure))
        } else {
            status = .idle
            lastSyncAt = Date()
        }
        if rejections && !rejected.isEmpty { showsRejectedAlert = true }
    }

    /// Re-reads the outbox count and the rejected commands from the core, in
    /// one visit to the actor.
    func refreshLocalState() async {
        let state = await core.localState()
        pendingCount = state.pending
        rejected = state.rejected.map(RejectedChange.init(entry:))
    }

    // MARK: - Rejections

    func dismiss(_ change: RejectedChange) async {
        await core.dismissAllRejected([change.entry])
        await refreshLocalState()
    }

    func dismissAllRejected() async {
        await core.dismissAllRejected(rejected.map(\.entry))
        await refreshLocalState()
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
            await store.setAuthor(response.username)
            try await core.relabelEveryOutbox(author: response.username)
            await store.refreshAfterSync()
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
        await store.setAuthor(account.author)
        serverVaults = []
        status = .idle
        lastSyncAt = nil
    }

    /// Settings changed the name for the rows written while logged out. An
    /// account, when there is one, keeps precedence (`AccountStore.author`).
    func adoptLocalAuthor() async {
        await store.setAuthor(account.author)
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

}
