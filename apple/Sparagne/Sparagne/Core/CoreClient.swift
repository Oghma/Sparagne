import Foundation
import SparagneCore

/// The app's single point of contact with the Rust core.
///
/// Everything the UI does is either a query on `CoreHandle` or a command
/// wrapped in an envelope minted by the core (`docs/v2/ARCH.md` §2.2). Ids are
/// never invented in Swift: `newEnvelope` and `createVaultEnvelope` produce
/// them, and the created entity's id comes back on the `Receipt`.
///
/// The core is synchronous and fast enough to call from the main actor for
/// now; keeping every call site in this type and `AppStore` means moving it to
/// a background actor later is a local change.
@MainActor
final class CoreClient {
    let handle: CoreHandle

    /// Author recorded on every command in the log. It follows the account:
    /// logging in changes it and relabels whatever the outbox still holds
    /// (`docs/v2/SYNC.md` §1).
    var author: String

    /// Called after every applied command, so `SyncEngine` can schedule a
    /// push. Set by the engine, never by the views.
    var onExecuted: (() -> Void)?

    init(handle: CoreHandle, author: String = NSUserName()) {
        self.handle = handle
        self.author = author
    }

    /// Opens the database in `~/Library/Application Support/Sparagne`, which
    /// the sandbox resolves inside the app container.
    static func onDisk(author: String = NSUserName()) throws -> CoreClient {
        let url = try databaseURL()
        return CoreClient(handle: try CoreHandle.open(path: url.path(percentEncoded: false)), author: author)
    }

    /// An in-memory database, for tests and previews.
    static func inMemory(author: String = "test") throws -> CoreClient {
        CoreClient(handle: try CoreHandle.openInMemory(), author: author)
    }

    static func databaseURL() throws -> URL {
        let support = try FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let directory = support.appending(path: "Sparagne", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory.appending(path: "sparagne.sqlite", directoryHint: .notDirectory)
    }

    // MARK: - Writes

    /// Addresses a command to a vault and applies it.
    @discardableResult
    func execute(vaultId: Uuid, _ command: Command) throws -> Receipt {
        let receipt = try handle.execute(
            envelope: newEnvelope(vaultId: vaultId, author: author, command: command)
        )
        onExecuted?()
        return receipt
    }

    /// `CreateVault` lives outside any vault log, so it has its own envelope.
    @discardableResult
    func createVault(name: String, currency: Currency = .eur) throws -> Receipt {
        let receipt = try handle.execute(
            envelope: createVaultEnvelope(author: author, name: name, currency: currency)
        )
        onExecuted?()
        return receipt
    }

    // MARK: - Reads

    func vaults() throws -> [VaultView] {
        try handle.vaults()
    }

    func snapshot(vaultId: Uuid) throws -> VaultSnapshot {
        try handle.snapshot(vaultId: vaultId)
    }

    func categories(vaultId: Uuid, includeArchived: Bool = false) throws -> [CategoryView] {
        try handle.categories(vaultId: vaultId, includeArchived: includeArchived)
    }

    func aliases(vaultId: Uuid) throws -> [AliasView] {
        try handle.aliases(vaultId: vaultId)
    }

    /// Active categories whose name is close to `name`, for the "similar
    /// categories" hint while typing a new one (docs task 3: suggest, never
    /// block, `docs/v2/DISTILLATO_V1.md` §2.1).
    func similarCategories(vaultId: Uuid, name: String) throws -> [CategoryView] {
        try handle.similarCategories(vaultId: vaultId, name: name)
    }

    /// What `MergeCategory` would refuse, without changing anything.
    func previewMerge(vaultId: Uuid, sourceId: Uuid, targetId: Uuid) throws -> MergePreview {
        try handle.previewMerge(vaultId: vaultId, sourceId: sourceId, targetId: targetId)
    }

    func listRecurring(vaultId: Uuid, includeArchived: Bool) throws -> [RecurringView] {
        try handle.listRecurring(vaultId: vaultId, includeArchived: includeArchived)
    }

    /// Templates with periods still waiting for a decision, as of `today`
    /// (`docs/v2/ARCH.md` §4: the app passes "today" in the system timezone).
    func pendingRecurring(vaultId: Uuid, today: NaiveDate) throws -> [PendingRecurring] {
        try handle.pendingRecurring(vaultId: vaultId, today: today)
    }

    func transactions(
        vaultId: Uuid,
        filter: TransactionFilter,
        limit: UInt32,
        cursor: String?
    ) throws -> Page {
        try handle.listTransactions(vaultId: vaultId, filter: filter, limit: limit, cursor: cursor)
    }

    /// `nil` bounds mean "open end": both nil is all time.
    func totals(vaultId: Uuid, from: UtcDateTime?, to: UtcDateTime?) throws -> PeriodTotals {
        try handle.periodTotals(vaultId: vaultId, from: from, to: to)
    }

    // MARK: - Analytics
    //
    // The summary panel and the summary views (`docs/v2/UI.md` §4). Every
    // range is half-open `[from, to)` in UTC; the app computes the month
    // boundaries with the system calendar so the core never sees one.

    /// The people who appear in the PERSONA column.
    func authors(vaultId: Uuid) throws -> [String] {
        try handle.authors(vaultId: vaultId)
    }

    func flowPersonTotals(vaultId: Uuid, from: UtcDateTime, to: UtcDateTime) throws -> [FlowPersonTotals] {
        try handle.flowPersonTotals(vaultId: vaultId, from: from, to: to)
    }

    func categoryTotals(
        vaultId: Uuid,
        from: UtcDateTime,
        to: UtcDateTime,
        person: String? = nil
    ) throws -> [CategoryTotals] {
        try handle.categoryTotals(vaultId: vaultId, from: from, to: to, person: person)
    }

    /// One `PeriodTotals` per gap between consecutive `bounds`.
    func bucketTotals(vaultId: Uuid, bounds: [UtcDateTime], person: String? = nil) throws -> [PeriodTotals] {
        try handle.bucketTotals(vaultId: vaultId, bounds: bounds, person: person)
    }

    /// One row per bucket and person for the RIEPILOGO: the epoch plus the
    /// thirteen month starts give thirteen buckets.
    func yearBreakdown(vaultId: Uuid, bounds: [UtcDateTime]) throws -> [BucketPersonTotals] {
        try handle.yearBreakdown(vaultId: vaultId, bounds: bounds)
    }

    func topExpenses(
        vaultId: Uuid,
        from: UtcDateTime,
        to: UtcDateTime,
        person: String? = nil,
        limit: UInt32
    ) throws -> [TopExpense] {
        try handle.topExpenses(vaultId: vaultId, from: from, to: to, person: person, limit: limit)
    }

    /// Turns a parsed quick-add line into a command plus the ids its names
    /// resolved to, against the vault's active entities.
    func resolveQuickAdd(
        vaultId: Uuid,
        parsed: QuickAdd,
        now: Date,
        defaults: QuickAddDefaults
    ) throws -> ResolvedQuickAdd {
        try handle.resolveQuickAdd(
            vaultId: vaultId,
            parsed: parsed,
            now: CoreDate.offset(now),
            defaults: defaults
        )
    }

    // MARK: - Sync
    //
    // The core writes and reads every sync body; `SyncEngine` only carries
    // the strings to the server and back (`docs/v2/SYNC.md` §1).

    func syncState(vaultId: Uuid) throws -> SyncState {
        try handle.syncState(vaultId: vaultId)
    }

    /// The body of `POST /vaults/{id}/push`: at most `limit` commands of the
    /// outbox, in local order. The engine pushes again while the outbox is
    /// not empty (`docs/v2/SYNC.md` §4.1).
    func pushRequestJson(vaultId: Uuid, limit: UInt32) throws -> String {
        try handle.pushRequestJson(vaultId: vaultId, limit: limit)
    }

    @discardableResult
    func applyPushResponse(vaultId: Uuid, json: String) throws -> SyncReport {
        try handle.applyPushResponseJson(vaultId: vaultId, json: json)
    }

    /// Folds a pull into the log; the core rebases when it has to, and
    /// creates the vault when this is a join.
    @discardableResult
    func integratePull(vaultId: Uuid, json: String) throws -> SyncReport {
        try handle.integratePullJson(vaultId: vaultId, json: json)
    }

    /// After a login: the outbox is re-signed with the account's username.
    func relabelOutbox(vaultId: Uuid, author: String) throws {
        try handle.relabelOutbox(vaultId: vaultId, author: author)
    }

    func rejectedCommands(vaultId: Uuid) throws -> [RejectedCommand] {
        try handle.rejectedCommands(vaultId: vaultId)
    }

    func dismissRejected(vaultId: Uuid, commandId: Uuid) throws {
        try handle.dismissRejected(vaultId: vaultId, commandId: commandId)
    }
}

// MARK: - Dates on the wire

/// Conversions between `Date` and the string scalars the core expects
/// (`core/src/ffi.rs`): `OffsetDateTime` is RFC 3339 with the system offset,
/// `UtcDateTime` is RFC 3339 in UTC, `NaiveDate` is a local `yyyy-MM-dd`.
enum CoreDate {
    private static func style(_ timeZone: TimeZone) -> Date.ISO8601FormatStyle {
        Date.ISO8601FormatStyle(
            dateSeparator: .dash,
            dateTimeSeparator: .standard,
            timeSeparator: .colon,
            timeZoneSeparator: .colon,
            includingFractionalSeconds: false,
            timeZone: timeZone
        )
    }

    private static let utc = TimeZone(identifier: "UTC") ?? .gmt

    /// `2026-03-01T12:00:00+01:00` in the system time zone.
    static func offset(_ date: Date, timeZone: TimeZone = .current) -> OffsetDateTime {
        date.formatted(style(timeZone))
    }

    /// `2026-03-01T11:00:00Z`.
    static func utcString(_ date: Date) -> UtcDateTime {
        date.formatted(style(utc))
    }

    /// Parses either flavour; the offset in the string wins over the style's.
    static func date(_ string: String) -> Date? {
        try? Date(string, strategy: style(utc))
    }

    /// The calendar day of `date` in the given time zone, as `yyyy-MM-dd`.
    static func day(_ date: Date, timeZone: TimeZone = .current) -> NaiveDate {
        date.formatted(dayStyle(timeZone))
    }

    /// A bare `yyyy-MM-dd` read back as local midnight.
    static func localDay(_ day: NaiveDate, timeZone: TimeZone = .current) -> Date? {
        try? Date(day, strategy: dayStyle(timeZone))
    }

    private static func dayStyle(_ timeZone: TimeZone) -> Date.ISO8601FormatStyle {
        Date.ISO8601FormatStyle(dateSeparator: .dash, timeZone: timeZone).year().month().day()
    }
}
