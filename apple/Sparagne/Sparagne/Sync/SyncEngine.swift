import Foundation
import Observation
import SparagneCore

/// Runs the client half of the sync protocol (`docs/v2/SYNC.md` §4-§5).
///
/// A round first asks the server which vaults the account has and with what
/// role, then for every local vault pushes what the outbox holds, a batch per
/// request, and pulls until the core says the server has nothing more; then
/// it joins whatever vault the server lists and the database does not have
/// yet. The core does all the reasoning — the engine only moves opaque JSON
/// between `CoreActor` and `ServerAPI` and keeps the status the window shows.
/// It shares that one actor with `AppStore`, so the core is never touched
/// from two isolation domains at once.
///
/// Nothing here throws out of the UI: a failure becomes `status`. The account
/// actions the sheets run (leaving a vault, changing the password) throw, so
/// the sheet can show why.
@Observable
@MainActor
final class SyncEngine {
    /// Where the last round stands; the top bar's pill words it
    /// (`SyncPillState`).
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
        /// The localized headline for the server's code, naming the person
        /// when the row was put on someone outside the vault.
        var summary: String { ErrorMessages.summary(for: command.code, detail: command.detail) }
        /// What the refused command did, in words (`CommandKindNames`).
        var kindName: String { CommandKindNames.name(for: command.kind) }
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

    /// The reason stored with the commands of a vault the account may only
    /// read, refused here or by the server's `403`.
    static var readOnlyMessage: String {
        String(localized: "Your role in this vault only lets you read it.")
    }

    /// The reason stored with the commands of a vault the server no longer
    /// shares with the account.
    static var noLongerSharedMessage: String {
        String(localized: "This vault is no longer shared with you.")
    }

    // MARK: Dependencies

    @ObservationIgnored private let core: CoreActor
    /// Not private: the permission checks read the local vaults' owners.
    @ObservationIgnored let store: AppStore
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
    /// The vault list from the last `GET /vaults` of this session. The roles
    /// in it outlive the session in `AccountStore.vaultRoles`.
    private(set) var serverVaults: [VaultSummary] = []
    /// What went wrong in the last login, register or logout: a localized
    /// headline, and the server's own words when they add something.
    private(set) var authMessage: String?
    private(set) var authDetail: String?
    /// Raised once by a sync that produced new rejections; the window lowers
    /// it when it has shown its alert.
    var showsRejectedAlert = false

    /// A round, or an account action that must not overlap one, is running.
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

    /// Adopts the account's author, the read-only vaults and the members
    /// remembered from the last session, subscribes to applied commands and
    /// reads the local sync state. `init` cannot await the core actor, so
    /// this runs once before the first command, ahead of `start()`.
    func prepare() async {
        await store.setAuthor(account.author)
        await publishAccess()
        publishMembers()
        await core.setOnExecuted { [weak self] in
            Task { @MainActor in await self?.scheduleSync() }
        }
        await refreshLocalState()
    }

    // MARK: - Derived

    var isLoggedIn: Bool { account.isLoggedIn }

    /// The role the server gave me on a vault, from the last `GET /vaults`
    /// (remembered across launches).
    func role(forVault vaultId: Uuid) -> MemberRole? {
        account.vaultRoles[vaultId]
    }

    func isOwner(ofVault vaultId: Uuid) -> Bool {
        role(forVault: vaultId) == .owner
    }

    /// The server no longer shares this vault with the account; the copy
    /// here stays, read-only, until the user removes it.
    func hasLostAccess(to vaultId: Uuid) -> Bool {
        account.lostVaultIds.contains(vaultId)
    }

    /// The local vaults no longer shared with the account, in list order.
    var lostVaults: [VaultView] {
        store.vaults.filter { hasLostAccess(to: $0.id) }
    }

    /// Vaults nothing may be written to: a viewer's, and the lost ones.
    var readOnlyVaultIds: Set<Uuid> {
        Set(account.vaultRoles.filter { $0.value == .viewer }.keys).union(account.lostVaultIds)
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

    /// One full round: roles, push, pull, join. Never runs twice at the same
    /// time and never throws.
    func syncNow() async {
        guard !isSyncing else { return }
        guard account.isLoggedIn else {
            await refreshLocalState()
            return
        }
        let session: Session
        do {
            session = try authorized()
        } catch {
            var round = Round()
            round.note(asServerError(error))
            await settle(round)
            return
        }

        isSyncing = true
        status = .syncing
        let round = await run(session)
        isSyncing = false
        await settle(round)
    }

    /// The round itself. A vault that fails does not stop the others: only
    /// a failure that nothing after it could get past (offline, a session
    /// the server no longer accepts) ends the round early.
    private func run(_ session: Session) async -> Round {
        var round = Round()

        // Roles first: they decide which vault may push at all, and they
        // reach the store and the actor before anything else is written.
        // Without a listing the remembered roles stand in, and nothing is
        // concluded from a vault missing from it.
        var listing: [VaultSummary]?
        do {
            listing = try await adoptListing(session)
        } catch {
            guard absorb(error, into: &round) else { return round }
        }
        await publishAccess()
        let listed = listing.map { Set($0.map(\.id)) }

        let known: Set<Uuid>
        let live: [Uuid]
        do {
            live = try await core.vaults().map(\.id)
            known = Set(live + (try await core.deletedVaults()))
        } catch {
            _ = absorb(error, into: &round)
            return round
        }

        for vaultId in live {
            do {
                round.outcome.merge(try await syncVault(vaultId, session: session, listed: listed))
            } catch {
                guard absorb(error, into: &round) else { return round }
            }
        }

        // A vault deleted here still owes the server its last commands, the
        // `DeleteVault` among them. Until they are confirmed it syncs like a
        // live one (so a deletion the server already holds, from another
        // device, is folded in as well); afterwards its log goes too
        // (`docs/v2/SYNC.md` §4.6). Read again: a pull above may have just
        // deleted one.
        do {
            for vaultId in try await core.deletedVaults() {
                do {
                    round.outcome.merge(try await settleDeleted(vaultId, session: session, listed: listed))
                } catch {
                    guard absorb(error, into: &round) else { return round }
                }
            }
        } catch {
            guard absorb(error, into: &round) else { return round }
        }

        // Joins: listed by the server, unknown here. A vault this round
        // deleted or forgot was known at its start, so it is not joined
        // again from a listing taken before the deletion went up.
        for summary in listing ?? [] where !known.contains(summary.id) {
            do {
                round.outcome.merge(try await pullLoop(summary.id, session: session))
            } catch {
                guard absorb(error, into: &round) else { return round }
            }
        }

        // The first push of a vault created here made it a server vault:
        // list again, so its role is known now rather than a round later.
        if round.outcome.claimed {
            do {
                _ = try await adoptListing(session)
            } catch {
                _ = absorb(error, into: &round)
            }
        }
        await publishAccess()
        // Who may be named on a row changes with the sharing, which only the
        // server knows: asked again every round, for the vault on screen.
        await refreshMembers(ofVault: store.currentVault?.id)
        return round
    }

    /// One live vault: push, then pull.
    ///
    /// A viewer's vault never pushes: the server would refuse every command,
    /// so the outbox is refused here, as if it had. A push the server refuses
    /// with `403` is folded in the same way. Either way the pull still runs,
    /// so the owner's changes keep arriving. A vault the server has never
    /// seen is created by the first push, whose first command is the
    /// `CreateVault` that minted it: there is no separate route for it
    /// (`docs/v2/SYNC.md` §3).
    private func syncVault(_ vaultId: Uuid, session: Session, listed: Set<Uuid>?) async throws -> Outcome {
        let state = try await core.syncState(vaultId: vaultId)
        let unlisted = listed.map { !$0.contains(vaultId) } ?? false
        // Known to the server, yet not listed by it: deleted there (its
        // members still pull the deletion) or no longer shared with me (the
        // pull answers 404).
        let mayBeLost = unlisted && state.lastServerSeq > 0
        // Still lost: a listing that has it again took it off the lost list
        // before this, so there is nothing to ask the server.
        if hasLostAccess(to: vaultId) { return Outcome() }

        var outcome = Outcome()
        var pushFailure: ServerError?
        if role(forVault: vaultId) == .viewer {
            if state.outbox > 0 {
                outcome.absorb(
                    try await core.rejectOutbox(vaultId: vaultId, code: "forbidden", message: Self.readOnlyMessage)
                )
            }
        } else {
            do {
                let pushed = try await pushLoop(vaultId, session: session)
                outcome.merge(pushed)
                outcome.claimed = state.lastServerSeq == 0 && !(listed?.contains(vaultId) ?? false) && pushed.changed
            } catch let error as ServerError where error.isForbidden {
                // The role changed since `GET /vaults`, or the listing could
                // not be read.
                outcome.absorb(
                    try await core.rejectOutbox(vaultId: vaultId, code: "forbidden", message: Self.readOnlyMessage)
                )
            } catch let error as ServerError where error.isNotFound && mayBeLost {
                // The pull below tells a lost vault from anything else.
                pushFailure = error
            }
        }

        do {
            outcome.merge(try await pullLoop(vaultId, session: session))
        } catch let error as ServerError where error.isNotFound && mayBeLost {
            outcome.merge(try await loseAccess(to: vaultId))
            return outcome
        }
        if let pushFailure { throw pushFailure }
        return outcome
    }

    /// A deleted vault: its outbox goes up, then, once nothing is pending
    /// and no refused command is left to read, the vault leaves this device
    /// for good. Every device does the same, so none keeps the log of a
    /// vault that no longer exists.
    private func settleDeleted(_ vaultId: Uuid, session: Session, listed: Set<Uuid>?) async throws -> Outcome {
        var outcome = Outcome()
        if try await core.syncState(vaultId: vaultId).outbox > 0 {
            outcome = try await syncVault(vaultId, session: session, listed: listed)
        }
        let state = try await core.syncState(vaultId: vaultId)
        if state.outbox == 0 && state.rejected == 0 {
            try await core.forgetVault(vaultId)
            drop(vaultId)
        }
        return outcome
    }

    /// The server no longer lets the account read a vault it had. The copy
    /// stays, read-only, until the user removes it from the management
    /// sheet: never dropped behind the user's back. What was waiting to go
    /// up is refused, so it can be read instead of pending for ever.
    private func loseAccess(to vaultId: Uuid) async throws -> Outcome {
        account.markLost(vaultId)
        serverVaults.removeAll { $0.id == vaultId }
        var outcome = Outcome()
        if try await core.syncState(vaultId: vaultId).outbox > 0 {
            outcome.absorb(
                try await core.rejectOutbox(vaultId: vaultId, code: "not_found", message: Self.noLongerSharedMessage)
            )
        }
        return outcome
    }

    /// `GET /vaults`, remembered: the roles outlive the session.
    private func adoptListing(_ session: Session) async throws -> [VaultSummary] {
        let summaries = try await session.api.vaults(token: session.token)
        serverVaults = summaries
        account.adoptRoles(Dictionary(summaries.map { ($0.id, $0.role) }, uniquingKeysWith: { _, last in last }))
        return summaries
    }

    /// Pushes batches until the outbox is empty. A batch that leaves the
    /// outbox no shorter made no progress, so it stops instead of looping:
    /// confirmations and rejections both take commands out of it.
    private func pushLoop(_ vaultId: Uuid, session: Session) async throws -> Outcome {
        var outcome = Outcome()
        for _ in 0..<Self.maxPages {
            let pending = try await core.syncState(vaultId: vaultId).outbox
            guard pending > 0 else { break }
            let body = try await core.pushRequestJson(vaultId: vaultId, limit: Self.pushLimit)
            let response = try await session.api.push(token: session.token, vaultId: vaultId, body: body)
            outcome.absorb(try await core.applyPushResponse(vaultId: vaultId, json: response))
            if try await core.syncState(vaultId: vaultId).outbox >= pending { break }
        }
        return outcome
    }

    /// Pulls pages from the contiguous watermark while the core reports the
    /// server holds more, or until the watermark stops advancing.
    private func pullLoop(_ vaultId: Uuid, session: Session) async throws -> Outcome {
        var outcome = Outcome()
        for _ in 0..<Self.maxPages {
            let since = try await core.syncState(vaultId: vaultId).lastServerSeq
            let page = try await session.api.pull(
                token: session.token,
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

    /// Records a failure of the round; `false` when it ends the round.
    private func absorb(_ error: some Error, into round: inout Round) -> Bool {
        let failure = asServerError(error)
        round.note(failure)
        return !failure.endsRound
    }

    /// Writes the result of a sync round into the observable state.
    private func settle(_ round: Round) async {
        await refreshLocalState()
        if let failure = round.failure, failure.isUnauthorized {
            // The token expired, or a password change on another device
            // revoked it: the session is over, the name stays for the form.
            account.signOut(expired: true)
            await store.setAuthor(account.author)
            publishMembers()
            serverVaults = []
        }
        if round.outcome.changed { await store.refreshAfterSync() }
        if let failure = round.failure {
            if failure.isOffline {
                status = .offline
            } else {
                status = .error(failure.isUnauthorized ? ErrorMessages.sessionExpired : describe(failure))
            }
        } else {
            status = .idle
            lastSyncAt = Date()
        }
        if round.outcome.rejected && !rejected.isEmpty { showsRejectedAlert = true }
    }

    /// Re-reads the outbox count and the rejected commands from the core, in
    /// one visit to the actor.
    func refreshLocalState() async {
        let state = await core.localState()
        pendingCount = state.pending
        rejected = state.rejected.map(RejectedChange.init(entry:))
    }

    /// Hands the read-only vaults to the store, which hides what would
    /// write, and to the actor, which refuses it.
    private func publishAccess() async {
        let ids = readOnlyVaultIds
        store.setReadOnlyVaults(ids)
        await core.setReadOnlyVaults(ids)
    }

    /// Hands the store the members of each vault, the names its person
    /// cells and owner pickers offer. Only while logged in: the server
    /// checks a person against its usernames, and a logged-out window signs
    /// its rows with a local name that is none of them.
    private func publishMembers() {
        store.setVaultMembers(account.isLoggedIn ? account.vaultMembers : nil)
    }

    /// Asks the server who belongs to `vaultId`, remembers it and hands it
    /// to the store. The window calls it when a vault opens, a round when it
    /// ends. Only for a vault the server listed: one created here and not
    /// pushed yet has no members there, just its author.
    ///
    /// Silent on failure: the list is a convenience, and the one heard last
    /// stands in until the next round.
    func refreshMembers(ofVault vaultId: Uuid?) async {
        guard let vaultId, role(forVault: vaultId) != nil, let session = try? authorized() else {
            publishMembers()
            return
        }
        // Asked again after the answer: a round may have dropped the vault
        // (left, deleted, no longer shared) while the request was out, and
        // its members must not come back with no vault to belong to.
        if let members = try? await session.api.members(token: session.token, vaultId: vaultId),
           account.isLoggedIn, role(forVault: vaultId) != nil
        {
            account.adoptMembers(members.map(\.username), ofVault: vaultId)
        }
        publishMembers()
    }

    /// A vault left this device: nothing more to remember about it.
    private func drop(_ vaultId: Uuid) {
        account.forget(vault: vaultId)
        serverVaults.removeAll { $0.id == vaultId }
        publishMembers()
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
    /// be relabelled before it is pushed (`docs/v2/SYNC.md` §4.4). The
    /// server trims and lowercases the name; so does the app, before the
    /// name is sent, so both sides mean the same account.
    private func authenticate(username raw: String, password: String, registering: Bool) async {
        authMessage = nil
        authDetail = nil
        let username = AccountRules.normalize(username: raw)
        guard let url = account.baseURL else {
            authMessage = ServerError.notConfigured.summary
            return
        }
        let api = ServerAPI(transport: makeTransport(url))
        do {
            let response =
                registering
                ? try await api.register(username: username, password: password)
                : try await api.login(username: username, password: password)
            account.signIn(
                username: response.username,
                token: response.token,
                expiresAt: Date(timeIntervalSince1970: TimeInterval(response.expiresAt))
            )
            await store.setAuthor(response.username)
            try await core.relabelEveryOutbox(author: response.username)
            await store.refreshAfterSync()
            status = .idle
            await syncNow()
            start()
        } catch {
            let failure = asServerError(error)
            // On the login route a 401 is the credentials, not a session.
            authMessage = failure.isUnauthorized ? ErrorMessages.wrongCredentials : failure.summary
            authDetail = failure.isUnauthorized ? nil : failure.detail
        }
    }

    func logOut() async {
        authMessage = nil
        authDetail = nil
        if let session = try? authorized() {
            _ = try? await session.api.logout(token: session.token)
        }
        stop()
        account.signOut()
        await store.setAuthor(account.author)
        publishMembers()
        serverVaults = []
        status = .idle
        lastSyncAt = nil
    }

    /// Settings changed the name for the rows written while logged out. An
    /// account, when there is one, keeps precedence (`AccountStore.author`).
    func adoptLocalAuthor() async {
        await store.setAuthor(account.author)
    }

    /// `POST /auth/password`. This session stays; every other device of the
    /// account is signed out by the server and has to log in again.
    func changePassword(current: String, new: String) async throws {
        let session = try authorized()
        try await session.api.changePassword(token: session.token, currentPassword: current, newPassword: new)
    }

    // MARK: - Leaving and removing a vault

    /// Leaves a vault shared with me. What I wrote and did not send yet goes
    /// up first (a viewer has nothing to send); then the membership goes on
    /// the server, and the vault leaves this device. If the server refuses
    /// the removal, nothing here is forgotten.
    func leaveVault(_ vaultId: Uuid) async throws {
        let session = try authorized()
        guard let username = account.username else { throw ServerError.notLoggedIn }
        try await exclusively {
            if role(forVault: vaultId)?.canWrite ?? true, try await core.syncState(vaultId: vaultId).outbox > 0 {
                do {
                    _ = try await pushLoop(vaultId, session: session)
                } catch {
                    // A refusal means the role changed under me: those
                    // changes leave with the vault. Anything else stops.
                    guard let failure = error as? ServerError, failure.isForbidden else { throw error }
                }
            }
            try await session.api.removeMember(token: session.token, vaultId: vaultId, username: username)
            try await core.forgetVault(vaultId)
            drop(vaultId)
        }
        await publishAccess()
        await store.refreshAfterSync()
        await refreshLocalState()
    }

    /// Removes a vault no longer shared with me from this Mac. Nothing is
    /// sent: the server already let it go.
    func removeFromThisMac(_ vaultId: Uuid) async throws {
        try await exclusively {
            try await core.forgetVault(vaultId)
            drop(vaultId)
        }
        await publishAccess()
        await store.refreshAfterSync()
        await refreshLocalState()
    }

    /// Runs `work` with no round in flight and none starting until it is
    /// done: a round that pushed or pulled a vault being forgotten would
    /// bring it back.
    private func exclusively<T>(_ work: () async throws -> T) async rethrows -> T {
        while isSyncing {
            try? await Task.sleep(for: .milliseconds(50))
        }
        isSyncing = true
        defer { isSyncing = false }
        return try await work()
    }

    // MARK: - Sharing

    func members(ofVault vaultId: Uuid) async throws -> [MemberEntry] {
        let session = try authorized()
        return try await session.api.members(token: session.token, vaultId: vaultId)
    }

    /// The new member can be named on a row from now on, not from the next
    /// round.
    func setMember(vaultId: Uuid, username: String, role: MemberRole) async throws {
        let session = try authorized()
        try await session.api.setMember(
            token: session.token,
            vaultId: vaultId,
            username: AccountRules.normalize(username: username),
            role: role
        )
        await refreshMembers(ofVault: vaultId)
    }

    func removeMember(vaultId: Uuid, username: String) async throws {
        let session = try authorized()
        try await session.api.removeMember(token: session.token, vaultId: vaultId, username: username)
        await refreshMembers(ofVault: vaultId)
    }

    // MARK: - Plumbing

    /// The API and the token of the session.
    private struct Session {
        let api: ServerAPI
        let token: String
    }

    private func authorized() throws -> Session {
        guard let url = account.baseURL else { throw ServerError.notConfigured }
        guard let token = account.token else { throw ServerError.notLoggedIn }
        return Session(api: ServerAPI(transport: makeTransport(url)), token: token)
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
        error.detail.map { "\(error.summary): \($0)" } ?? error.summary
    }

    /// What one round of push or pull did to a vault.
    private struct Outcome {
        var changed = false
        var received = false
        var rejected = false
        /// A first push minted the vault on the server.
        var claimed = false

        mutating func absorb(_ report: SyncReport) {
            changed = changed || report.confirmed > 0 || report.received > 0 || report.rebased
            received = received || report.received > 0
            rejected = rejected || !report.rejected.isEmpty
        }

        mutating func merge(_ other: Outcome) {
            changed = changed || other.changed
            received = received || other.received
            rejected = rejected || other.rejected
            claimed = claimed || other.claimed
        }
    }

    /// A whole round: what it did, and what went wrong first.
    private struct Round {
        var outcome = Outcome()
        var failure: ServerError?

        /// The failure that ended the round is the one the status shows;
        /// otherwise the first one.
        mutating func note(_ failure: ServerError) {
            if failure.endsRound || self.failure == nil { self.failure = failure }
        }
    }
}
