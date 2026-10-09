import Foundation
import SparagneCore

/// The app's single point of contact with the Rust core, and the only
/// isolation domain that ever touches it.
///
/// Everything the UI does is either a query on `CoreHandle` or a command
/// wrapped in an envelope minted by the core. Ids are
/// never invented in Swift: `newEnvelope` and `createVaultEnvelope` produce
/// them, and the created entity's id comes back on the `Receipt`.
///
/// `CoreHandle`'s calls are synchronous and hold a mutex (`core/src/ffi.rs`),
/// so a long scan would freeze the window if it ran on the main actor. It runs
/// here instead: `AppStore` and `SyncEngine` await this actor, which serializes
/// the two of them onto one executor and leaves the main actor free to draw.
/// Every value crossing back is `Sendable` — the
/// generated record and enum types already are.
///
/// Two FFI functions deliberately stay off it: `parseQuickAdd` and
/// `parseMoney`. They are free functions over a short string, with no `Core`,
/// no mutex and no database behind them, and they run while a cell or the ⌘K
/// line is being typed — a hop per keystroke would buy nothing and cost the
/// live preview a frame.
actor CoreActor {
    private let handle: CoreHandle

    /// Author recorded on every command in the log. It follows the account:
    /// logging in changes it and relabels whatever the outbox still holds.
    private var author: String

    /// The author the actor was built with. The store shows a name before it
    /// has awaited anything; `setAuthor` keeps the two in step afterwards.
    nonisolated let initialAuthor: String

    /// Called after every applied command, so `SyncEngine` can schedule a
    /// push. Set by the engine, never by the views.
    private var onExecuted: (@Sendable () -> Void)?

    /// Vaults the account may only read: a viewer's, or one no longer shared
    /// with it. Set by `SyncEngine` from the server's roles. The core itself
    /// would apply the command, and the server would refuse it at the next
    /// push; refusing here keeps it from ever entering the log.
    private var readOnlyVaults: Set<Uuid> = []

    /// Awaited before every call into the core. `nil` in the app; the tests
    /// pass one to hold a call in flight and prove that a caller on the main
    /// actor is not blocked while the core works.
    private let probe: (@Sendable () async -> Void)?

    init(
        handle: CoreHandle,
        author: String = NSUserName(),
        probe: (@Sendable () async -> Void)? = nil
    ) {
        self.handle = handle
        self.author = author
        self.initialAuthor = author
        self.probe = probe
    }

    /// The real database's file name.
    static let databaseName = "sparagne.sqlite"

    /// Opens `name` in `~/Library/Application Support/Sparagne`, which the
    /// sandbox resolves inside the app container: the real database, unless
    /// a launch option names another (`LaunchOptions`).
    static func onDisk(named name: String = databaseName, author: String = NSUserName()) throws -> CoreActor {
        let url = try databaseURL(named: name)
        return CoreActor(
            handle: try CoreHandle.open(path: url.path(percentEncoded: false)),
            author: author
        )
    }

    /// An in-memory database, for tests and previews.
    static func inMemory(
        author: String = "test",
        probe: (@Sendable () async -> Void)? = nil
    ) throws -> CoreActor {
        CoreActor(handle: try CoreHandle.openInMemory(), author: author, probe: probe)
    }

    /// `name` in the app's folder; a name that starts with `/` or `~` is a
    /// path, taken as it is.
    static func databaseURL(named name: String = databaseName) throws -> URL {
        if name.hasPrefix("/") || name.hasPrefix("~") {
            return URL(filePath: (name as NSString).expandingTildeInPath, directoryHint: .notDirectory)
        }
        let support = try FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let directory = support.appending(path: "Sparagne", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory.appending(path: name, directoryHint: .notDirectory)
    }

    // MARK: - The funnel
    //
    // Every method below reaches the handle through `visit`, so the probe and
    // the "nothing else touches `handle`" rule both hold in one place.

    private func visit<T: Sendable>(_ body: (CoreHandle) throws -> T) async rethrows -> T {
        await probe?()
        return try body(handle)
    }

    /// Whether the core is being called from the main thread. Always false in
    /// the app: the tests assert it to keep the core off the UI thread.
    func runsOnTheMainThread() async -> Bool {
        await visit { _ in Thread.isMainThread }
    }

    // MARK: - Author and notifications

    func setAuthor(_ name: String) {
        author = name
    }

    func setOnExecuted(_ hook: (@Sendable () -> Void)?) {
        onExecuted = hook
    }

    func setReadOnlyVaults(_ ids: Set<Uuid>) {
        readOnlyVaults = ids
    }

    /// The same refusal the server would send, before anything is written.
    private func ensureWritable(_ vaultId: Uuid) throws {
        guard !readOnlyVaults.contains(vaultId) else {
            throw DomainError.Forbidden(message: "this vault is read-only for your account")
        }
    }

    // MARK: - Writes

    /// Addresses a command to a vault and applies it.
    @discardableResult
    func execute(vaultId: Uuid, _ command: Command) async throws -> Receipt {
        try ensureWritable(vaultId)
        let receipt = try await visit { handle in
            try handle.execute(envelope: newEnvelope(vaultId: vaultId, author: author, command: command))
        }
        onExecuted?()
        return receipt
    }

    /// `CreateVault` lives outside any vault log, so it has its own envelope.
    @discardableResult
    func createVault(name: String, currency: Currency = .eur) async throws -> Receipt {
        let receipt = try await visit { handle in
            try handle.execute(envelope: createVaultEnvelope(author: author, name: name, currency: currency))
        }
        onExecuted?()
        return receipt
    }

    /// Several commands in one local transaction, all or none
    /// (`Core::execute_batch`). Each keeps its own log row and syncs on its
    /// own; `onExecuted` fires once for the whole batch.
    @discardableResult
    func executeBatch(vaultId: Uuid, _ commands: [Command]) async throws -> [Receipt] {
        guard !commands.isEmpty else { return [] }
        try ensureWritable(vaultId)
        let receipts = try await visit { handle in
            try handle.executeBatch(
                envelopes: commands.map { newEnvelope(vaultId: vaultId, author: author, command: $0) }
            )
        }
        onExecuted?()
        return receipts
    }

    /// An envelope signed with this actor's author, minted before it runs:
    /// for a caller that needs the command id (and so the id of what it
    /// creates) up front, like redo re-adding a row.
    func envelope(vaultId: Uuid, _ command: Command) -> CommandEnvelope {
        newEnvelope(vaultId: vaultId, author: author, command: command)
    }

    /// Applies an envelope minted with `envelope(vaultId:_:)`.
    @discardableResult
    func execute(envelope: CommandEnvelope) async throws -> Receipt {
        try ensureWritable(envelope.vaultId)
        let receipt = try await visit { try $0.execute(envelope: envelope) }
        onExecuted?()
        return receipt
    }

    /// Envelopes minted with `envelope(vaultId:_:)`, as one batch like
    /// `executeBatch(vaultId:_:)`: for commands that name what an earlier one
    /// in the same batch creates, like an alias and its new category. The
    /// core refuses a batch that spans two vaults.
    @discardableResult
    func executeBatch(envelopes: [CommandEnvelope]) async throws -> [Receipt] {
        guard let first = envelopes.first else { return [] }
        try ensureWritable(first.vaultId)
        let receipts = try await visit { try $0.executeBatch(envelopes: envelopes) }
        onExecuted?()
        return receipts
    }

    /// Imports a bank or card statement, one command per row
    /// (`core::statement`).
    @discardableResult
    func importStatement(
        vaultId: Uuid,
        text: String,
        mapping: StatementMapping,
        options: StatementOptions,
        overrides: [StatementRowOverride]
    ) async throws -> StatementReport {
        try ensureWritable(vaultId)
        let report = try await visit { handle in
            try handle.importStatement(
                vaultId: vaultId,
                author: author,
                text: text,
                mapping: mapping,
                options: options,
                overrides: overrides
            )
        }
        onExecuted?()
        return report
    }

    // MARK: - Reads

    func vaults() async throws -> [VaultView] {
        try await visit { try $0.vaults() }
    }

    /// The vaults a `DeleteVault` removed whose log is still here. Their
    /// outbox has to reach the server like any other, the deletion first of
    /// all.
    func deletedVaults() async throws -> [Uuid] {
        try await visit { try $0.deletedVaults() }
    }

    func snapshot(vaultId: Uuid) async throws -> VaultSnapshot {
        try await visit { try $0.snapshot(vaultId: vaultId) }
    }

    func categories(vaultId: Uuid, includeArchived: Bool = false) async throws -> [CategoryView] {
        try await visit { try $0.categories(vaultId: vaultId, includeArchived: includeArchived) }
    }

    func aliases(vaultId: Uuid) async throws -> [AliasView] {
        try await visit { try $0.aliases(vaultId: vaultId) }
    }

    /// Active categories whose name is close to `name`, for the "similar
    /// categories" hint while typing a new one: it suggests, it never
    /// blocks.
    func similarCategories(vaultId: Uuid, name: String) async throws -> [CategoryView] {
        try await visit { try $0.similarCategories(vaultId: vaultId, name: name) }
    }

    /// Ids of the categories, wallets and envelopes used most recently, most
    /// recent first: the "recent" tier of pickers and completions.
    func recentUsage(vaultId: Uuid, since: UtcDateTime, limit: UInt32) async throws -> RecentUsage {
        try await visit { try $0.recentUsage(vaultId: vaultId, since: since, limit: limit) }
    }

    /// Per-category totals over `[from, to)` for everybody, each with the
    /// number of rows behind it: the usage column of the SETUP categories.
    func categoryTotals(vaultId: Uuid, from: UtcDateTime, to: UtcDateTime) async throws -> [CategoryTotals] {
        try await visit { try $0.categoryTotals(vaultId: vaultId, from: from, to: to, person: nil) }
    }

    /// The category each note was filed under before, one entry per note.
    func suggestCategories(
        vaultId: Uuid,
        notes: [String],
        since: UtcDateTime
    ) async throws -> [CategorySuggestion?] {
        try await visit { try $0.suggestCategories(vaultId: vaultId, notes: notes, since: since) }
    }

    /// What importing a statement would do, row by row. Writes nothing.
    func previewStatement(
        vaultId: Uuid,
        text: String,
        mapping: StatementMapping,
        options: StatementOptions
    ) async throws -> StatementPreview {
        try await visit {
            try $0.previewStatement(vaultId: vaultId, text: text, mapping: mapping, options: options)
        }
    }

    /// What `MergeCategory` would refuse, without changing anything.
    func previewMerge(vaultId: Uuid, sourceId: Uuid, targetId: Uuid) async throws -> MergePreview {
        try await visit { try $0.previewMerge(vaultId: vaultId, sourceId: sourceId, targetId: targetId) }
    }

    func listRecurring(vaultId: Uuid, includeArchived: Bool) async throws -> [RecurringView] {
        try await visit { try $0.listRecurring(vaultId: vaultId, includeArchived: includeArchived) }
    }

    /// Templates with periods still waiting for a decision, as of `today`
    /// (the app passes "today" in the system timezone).
    func pendingRecurring(vaultId: Uuid, today: NaiveDate) async throws -> [PendingRecurring] {
        try await visit { try $0.pendingRecurring(vaultId: vaultId, today: today) }
    }

    /// The periods of one template already recorded or skipped, oldest first.
    func recurringRuns(vaultId: Uuid, recurringId: Uuid) async throws -> [RecurringRunView] {
        try await visit { try $0.recurringRuns(vaultId: vaultId, recurringId: recurringId) }
    }

    // MARK: - Allocation plan
    //
    // The plan, its period due and the base it would share out also come
    // with every `load`; these are for the screens that need them again.

    func allocationPlan(vaultId: Uuid) async throws -> AllocationPlanView? {
        try await visit { try $0.allocationPlan(vaultId: vaultId) }
    }

    func pendingAllocation(vaultId: Uuid, today: NaiveDate) async throws -> PendingAllocation? {
        try await visit { try $0.pendingAllocation(vaultId: vaultId, today: today) }
    }

    func allocationBase(vaultId: Uuid) async throws -> AllocationBase {
        try await visit { try $0.allocationBase(vaultId: vaultId) }
    }

    /// `lines` worked out on `total` against the envelopes as they are now;
    /// the lines may be a draft nobody saved.
    func previewAllocation(vaultId: Uuid, lines: [AllocationLine], total: Int64) async throws -> AllocationPreview {
        try await visit { try $0.previewAllocation(vaultId: vaultId, lines: lines, total: total) }
    }

    /// The plan's decided periods, the most recent first.
    func allocationRuns(vaultId: Uuid, limit: UInt32) async throws -> [AllocationRunView] {
        try await visit { try $0.allocationRuns(vaultId: vaultId, limit: limit) }
    }

    func transactions(
        vaultId: Uuid,
        filter: TransactionFilter,
        limit: UInt32,
        cursor: String?
    ) async throws -> Page {
        try await visit { try $0.listTransactions(vaultId: vaultId, filter: filter, limit: limit, cursor: cursor) }
    }

    /// `nil` bounds mean "open end": both nil is all time.
    func totals(vaultId: Uuid, from: UtcDateTime?, to: UtcDateTime?) async throws -> PeriodTotals {
        try await visit { try $0.periodTotals(vaultId: vaultId, from: from, to: to) }
    }

    /// The people who appear in the PERSONA column.
    func people(vaultId: Uuid) async throws -> [String] {
        try await visit { try $0.people(vaultId: vaultId) }
    }

    /// Turns a parsed quick-add line into a command plus the ids its names
    /// resolved to, against the vault's active entities and `people`, the
    /// names a `!name` may pick.
    func resolveQuickAdd(
        vaultId: Uuid,
        parsed: QuickAdd,
        now: Date,
        defaults: QuickAddDefaults,
        people: [String]
    ) async throws -> ResolvedQuickAdd {
        try await visit {
            try $0.resolveQuickAdd(
                vaultId: vaultId,
                parsed: parsed,
                now: CoreDate.offset(now),
                defaults: defaults,
                people: people
            )
        }
    }

    // MARK: - Composite loads
    //
    // The window needs a dozen queries to draw one month. Asking for them one
    // await at a time would cost a dozen hops and let a sync land in the
    // middle; one visit answers them all from the same view of the log.

    /// Everything `AppStore.reload()` draws, read in one visit.
    ///
    /// The month and the month before come from a single `bucket_totals` call
    /// with three boundaries, which is the only totals query that takes a
    /// person, so the whole window honours the PERSONA filter. The envelope x
    /// person matrix stays unfiltered on purpose: it *is* the per-person
    /// breakdown, and filtering it would blank every column but one.
    func load(_ request: VaultLoadRequest) async throws -> VaultLoad {
        try await visit { handle in
            let vaultId = request.vaultId
            return VaultLoad(
                snapshot: try handle.snapshot(vaultId: vaultId),
                categories: try handle.categories(vaultId: vaultId, includeArchived: false),
                page: try handle.listTransactions(
                    vaultId: vaultId,
                    filter: request.filter,
                    limit: request.limit,
                    cursor: nil
                ),
                people: try handle.people(vaultId: vaultId),
                pendingRecurring: try handle.pendingRecurring(vaultId: vaultId, today: request.today),
                allocationPlan: try handle.allocationPlan(vaultId: vaultId),
                pendingAllocation: try handle.pendingAllocation(vaultId: vaultId, today: request.today),
                allocationBase: try handle.allocationBase(vaultId: vaultId),
                flowPerson: try handle.flowPersonTotals(
                    vaultId: vaultId,
                    from: request.monthFrom,
                    to: request.monthTo
                ),
                categoryTotals: try handle.categoryTotals(
                    vaultId: vaultId,
                    from: request.monthFrom,
                    to: request.monthTo,
                    person: request.person
                ),
                monthPair: try handle.bucketTotals(
                    vaultId: vaultId,
                    bounds: [request.previousStart, request.monthFrom, request.monthTo],
                    person: request.person
                ),
                trailing: try handle.bucketTotals(
                    vaultId: vaultId,
                    bounds: request.trailingBounds,
                    person: request.person
                ),
                year: try handle.yearBreakdown(vaultId: vaultId, bounds: request.yearBounds)
            )
        }
    }

    /// The management list (archived included) and the aliases, together.
    func categoryManagement(vaultId: Uuid) async throws -> CategoryManagement {
        try await visit { handle in
            CategoryManagement(
                categories: try handle.categories(vaultId: vaultId, includeArchived: true),
                aliases: try handle.aliases(vaultId: vaultId)
            )
        }
    }

    // MARK: - Sync
    //
    // The core writes and reads every sync body; `SyncEngine` only carries
    // the strings to the server and back.

    func syncState(vaultId: Uuid) async throws -> SyncState {
        try await visit { try $0.syncState(vaultId: vaultId) }
    }

    /// The body of `POST /vaults/{id}/push`: at most `limit` commands of the
    /// outbox, in local order. The engine pushes again while the outbox is
    /// not empty.
    func pushRequestJson(vaultId: Uuid, limit: UInt32) async throws -> String {
        try await visit { try $0.pushRequestJson(vaultId: vaultId, limit: limit) }
    }

    @discardableResult
    func applyPushResponse(vaultId: Uuid, json: String) async throws -> SyncReport {
        try await visit { try $0.applyPushResponseJson(vaultId: vaultId, json: json) }
    }

    /// Folds a pull into the log; the core rebases when it has to, and
    /// creates the vault when this is a join.
    @discardableResult
    func integratePull(vaultId: Uuid, json: String) async throws -> SyncReport {
        try await visit { try $0.integratePullJson(vaultId: vaultId, json: json) }
    }

    /// After a login: every vault's outbox is re-signed with the account's
    /// username, the deleted vaults' included, or
    /// their deletion would be refused as another author's for ever.
    func relabelEveryOutbox(author name: String) async throws {
        try await visit { handle in
            for vaultId in try handle.vaults().map(\.id) + handle.deletedVaults() {
                try handle.relabelOutbox(vaultId: vaultId, author: name)
            }
        }
    }

    /// Refuses the whole outbox locally, as if the server had: what a push
    /// that came back `403` does, so the vault keeps pulling.
    @discardableResult
    func rejectOutbox(vaultId: Uuid, code: String, message: String) async throws -> SyncReport {
        try await visit { try $0.rejectOutbox(vaultId: vaultId, code: code, message: message) }
    }

    /// Drops a vault from this device without telling the server: after
    /// leaving it, or once a deletion has reached the server. Returns how
    /// many outbox commands were thrown away.
    @discardableResult
    func forgetVault(_ vaultId: Uuid) async throws -> UInt32 {
        try await visit { try $0.forgetVault(vaultId: vaultId) }
    }

    // MARK: - Maintenance

    /// A consistent copy of the whole database at `path`, which must not
    /// exist yet.
    func backup(to path: String) async throws {
        try await visit { try $0.backupTo(path: path) }
    }

    func rejectedCommands(vaultId: Uuid) async throws -> [RejectedCommand] {
        try await visit { try $0.rejectedCommands(vaultId: vaultId) }
    }

    func dismissRejected(vaultId: Uuid, commandId: Uuid) async throws {
        try await visit { try $0.dismissRejected(vaultId: vaultId, commandId: commandId) }
    }

    /// The outbox count over every vault and everything still held as
    /// rejected: what the top bar's sync pill and the rejections sheet show.
    ///
    /// Best effort, like the status line it feeds: a vault whose state cannot
    /// be read contributes nothing rather than blanking the whole count.
    func localState() async -> LocalSyncState {
        await visit { handle in
            var pending = 0
            var rejected: [RejectedEntry] = []
            // A deleted vault has no name left to show, but its outbox still
            // counts and its rejections are still the user's to read: a row
            // refused because the vault went away is exactly the case.
            var named = ((try? handle.vaults()) ?? []).map { ($0.id, $0.name) }
            named += ((try? handle.deletedVaults()) ?? []).map { ($0, Self.deletedVaultName) }
            for (vaultId, name) in named {
                if let state = try? handle.syncState(vaultId: vaultId) {
                    pending += Int(state.outbox)
                }
                for command in (try? handle.rejectedCommands(vaultId: vaultId)) ?? [] {
                    rejected.append(
                        RejectedEntry(vaultId: vaultId, vaultName: name, command: command)
                    )
                }
            }
            return LocalSyncState(pending: pending, rejected: rejected)
        }
    }

    /// What the rejections sheet shows in place of a name the vault no
    /// longer has.
    static let deletedVaultName = String(localized: "Deleted vault")

    /// Takes every rejected command out of the core, in one visit.
    func dismissAllRejected(_ entries: [RejectedEntry]) async {
        await visit { handle in
            for entry in entries {
                try? handle.dismissRejected(vaultId: entry.vaultId, commandId: entry.command.commandId)
            }
        }
    }
}

// MARK: - What a visit returns

/// What `CoreActor.load` needs to know about the window on screen. A plain
/// value so the whole request crosses to the actor in one piece.
struct VaultLoadRequest: Sendable {
    let vaultId: Uuid
    let filter: TransactionFilter
    let limit: UInt32
    /// `[from, to)` of the month on screen.
    let monthFrom: UtcDateTime
    let monthTo: UtcDateTime
    /// The start of the month before, for the "+4,2% vs luglio" line.
    let previousStart: UtcDateTime
    /// Thirteen boundaries: the twelve months ending with this one.
    let trailingBounds: [UtcDateTime]
    /// Fourteen boundaries: the epoch plus the thirteen month starts, so
    /// bucket 0 is everything before January.
    let yearBounds: [UtcDateTime]
    /// The PERSONA filter; `nil` is everybody.
    let person: String?
    /// Today in the system timezone, for the recurring periods that are due.
    let today: NaiveDate
}

/// Everything one visit to the core gives `AppStore.reload()`.
struct VaultLoad: Sendable {
    let snapshot: VaultSnapshot
    let categories: [CategoryView]
    let page: Page
    /// The persons of the live rows (`people()`), the PERSONA filter's segments.
    let people: [String]
    let pendingRecurring: [PendingRecurring]
    /// The vault's allocation plan, `nil` until one is made.
    let allocationPlan: AllocationPlanView?
    /// The plan's period waiting for a decision today, if any.
    let pendingAllocation: PendingAllocation?
    /// The incomes the next allocation would share out.
    let allocationBase: AllocationBase
    let flowPerson: [FlowPersonTotals]
    let categoryTotals: [CategoryTotals]
    /// `[the month before, the month]`, from the three-bound bucket query.
    let monthPair: [PeriodTotals]
    let trailing: [PeriodTotals]
    let year: [BucketPersonTotals]
}

/// The two lists the Categories table reads together.
struct CategoryManagement: Sendable {
    let categories: [CategoryView]
    let aliases: [AliasView]
}

/// One rejected command with the vault it belongs to, so the sheet can
/// dismiss it.
struct RejectedEntry: Sendable, Equatable {
    let vaultId: Uuid
    let vaultName: String
    let command: RejectedCommand
}

/// What the core says about the sync of every local vault.
struct LocalSyncState: Sendable {
    let pending: Int
    let rejected: [RejectedEntry]
}
