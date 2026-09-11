import Foundation
import Observation
import SparagneCore

/// A domain error on its way to an alert. `code` is the stable snake_case
/// code from `ErrorCodes.swift`, so tests and call sites can match on it.
struct AppError: Identifiable, Equatable, Sendable {
    let id = UUID()
    let code: String
    let message: String
    /// The names an ambiguous quick-add marker could have meant; empty
    /// otherwise. The alert offers them so the user can retype one.
    let candidates: [String]
    /// The fragment that was ambiguous (`QuickAddError.AmbiguousName`'s
    /// `name`), so `AppStore.resolveAmbiguous` knows what to replace in
    /// `quickAddText`. `nil` outside that one case.
    let ambiguousFragment: String?

    /// The localized headline for `code` (`ErrorMessages.swift`); the alert
    /// keeps `message`, the Rust `Display` text, as its secondary detail.
    var summary: String { ErrorMessages.summary(for: code) }

    init(code: String, message: String, candidates: [String] = [], ambiguousFragment: String? = nil) {
        self.code = code
        self.message = message
        self.candidates = candidates
        self.ambiguousFragment = ambiguousFragment
    }

    init(_ error: DomainError) {
        self.init(code: error.code, message: error.message)
    }

    init(_ error: QuickAddError) {
        let fragment: String? = if case .AmbiguousName(let name, _) = error { name } else { nil }
        self.init(code: error.code, message: error.message, candidates: error.candidates, ambiguousFragment: fragment)
    }
}

/// A void that has been hidden from the table but not yet applied.
///
/// The row disappears immediately and a toast counts down; `undo()` cancels,
/// and the window elapsing (or another destructive action) commits the
/// `VoidTransaction` command (`docs/v2/DISTILLATO_V1.md` §2.4).
struct PendingUndo: Identifiable, Equatable, Sendable {
    let id: Uuid
    let startedAt: Date
    let duration: Duration

    var seconds: Double {
        let components = duration.components
        return Double(components.seconds) + Double(components.attoseconds) * 1e-18
    }

    var deadline: Date { startedAt.addingTimeInterval(seconds) }

    /// `0...1`, how much of the window has elapsed.
    func progress(at now: Date) -> Double {
        guard seconds > 0 else { return 1 }
        return min(max(now.timeIntervalSince(startedAt) / seconds, 0), 1)
    }
}

extension TransactionPatch {
    /// A patch that carries no field changes nothing, so it is never sent
    /// (`UpdateTransaction` refuses it, `docs/v2/ARCH.md` §4).
    var isEmpty: Bool { self == TransactionPatch() }
}

extension RecurringPatch {
    /// Same rule as `TransactionPatch.isEmpty`: `UpdateRecurring` refuses an
    /// empty patch.
    var isEmpty: Bool { self == RecurringPatch() }
}

/// Everything the window shows and everything it can do.
///
/// The store holds no domain state of its own: `snapshot`, `categories` and
/// `transactions` are query results, refreshed by `reload()` after every
/// command (`docs/v2/ARCH.md` §2.2).
@Observable
@MainActor
final class AppStore {
    /// A month of a personal ledger fits in one page, so the grid never
    /// paginates in practice; `loadMore` stays for the pathological month.
    static let pageSize: UInt32 = 1000
    static let lastVaultKey = "lastVaultId"

    // MARK: Dependencies

    /// Not private: `SyncEngine` owns the same instance and swaps the author
    /// on login, and the tests write rows as a second member of the vault.
    @ObservationIgnored let client: CoreClient
    @ObservationIgnored private let defaults: UserDefaults
    @ObservationIgnored private let undoWindow: Duration
    /// Injected so tests do not wait out the real undo window.
    @ObservationIgnored private let sleeper: @Sendable (Duration) async throws -> Void

    // MARK: Vaults

    private(set) var vaults: [VaultView] = []
    private(set) var currentVault: VaultView?
    /// True when the account has no vault yet and onboarding must run.
    private(set) var needsOnboarding = false

    // MARK: Loaded data

    private(set) var snapshot: VaultSnapshot?
    private(set) var categories: [CategoryView] = []
    private(set) var transactions: [TransactionView] = []
    private(set) var nextCursor: String?
    private(set) var allRows: [TransactionRow] = []
    /// Distinct authors in the vault: the PERSONA segmented control.
    private(set) var authors: [String] = []
    /// Everything the ledger's summary panel draws.
    private(set) var summary: LedgerSummary?
    /// Everything the RIEPILOGO draws: the year of `month`, up to `month`
    /// (`docs/v2/UI.md` §2.2).
    private(set) var year: YearSummary?
    /// When the last command was applied, for the status bar's "saved at".
    private(set) var savedAt: Date?

    // MARK: Category management (Categories window)

    /// Active and archived categories, for the Categories window;
    /// `categories` above stays active-only, for pickers.
    private(set) var windowCategories: [CategoryView] = []
    private(set) var categoryAliases: [AliasView] = []

    // MARK: Recurring

    /// Periods still waiting for a decision, refreshed on every `reload()`
    /// so the ledger window's banner (`RecurringBanner`, `LedgerWindow.swift`)
    /// stays current without a separate poll.
    private(set) var pendingRecurringItems: [PendingRecurring] = []
    /// Every template, active and archived; loaded on demand when the
    /// Recurring panel opens.
    private(set) var recurringTemplates: [RecurringView] = []

    // MARK: Filters and view state

    /// The month the ledger reads and writes (`docs/v2/UI.md` §2.1). Changing
    /// it reloads the rows and every aggregate together, so the panel never
    /// describes a different month from the table.
    var month = MonthKey(Date()) { didSet { if month != oldValue { reload() } } }
    var direction: LedgerDirection = .expenses { didSet { if direction != oldValue { reload() } } }
    /// The PERSONA filter: `nil` is everybody.
    var person: String? { didSet { if person != oldValue { reload() } } }
    /// Which of the two views is on screen; no reload, the data is the same.
    /// The window opens on the RIEPILOGO (`docs/v2/UI.md` §2).
    var tab: LedgerTab = .summary
    var showVoided = false { didSet { if showVoided != oldValue { reload() } } }
    /// Transfers are in neither direction, so the View menu opts into them.
    var showTransfers = false { didSet { if showTransfers != oldValue { reload() } } }
    /// Debounced by the view; call `reload()` when it settles.
    var searchText = ""
    var quickAddText = ""

    // MARK: Transient

    private(set) var pendingUndo: PendingUndo?
    @ObservationIgnored private(set) var undoTask: Task<Void, Never>?
    var presentedError: AppError?

    /// Sticky quick-add defaults: the wallet and envelope last written to.
    private(set) var lastWalletId: Uuid?
    private(set) var lastFlowId: Uuid?

    init(
        client: CoreClient,
        defaults: UserDefaults = .standard,
        undoWindow: Duration = .seconds(5),
        sleeper: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) }
    ) {
        self.client = client
        self.defaults = defaults
        self.undoWindow = undoWindow
        self.sleeper = sleeper
        currentAuthor = client.author
    }

    /// The engine calls this on login and logout, and when Settings change
    /// the local name: the client stamps it on every command from now on.
    func setAuthor(_ name: String) {
        client.author = name
        currentAuthor = name
    }

    // MARK: - Derived

    var currency: Currency { snapshot?.currency ?? .eur }

    /// Who the log will credit the next command to: the PERSONA of a new row.
    /// Stored rather than read off the client, so the empty line redraws when
    /// the engine or Settings change the name (`setAuthor`).
    private(set) var currentAuthor: String
    var currencyCode: String { currency.code }

    /// Active wallets, for the pickers.
    var wallets: [WalletView] { snapshot?.wallets.filter { !$0.archived } ?? [] }

    /// Active envelopes, Unallocated first (the core already orders them).
    var flows: [FlowView] { snapshot?.flows.filter { !$0.archived } ?? [] }

    /// For the management sheet's collapsed "Archived" group.
    var archivedWallets: [WalletView] { snapshot?.wallets.filter { $0.archived } ?? [] }

    /// Unallocated is never archived, so it never needs to appear here.
    var archivedFlows: [FlowView] { snapshot?.flows.filter { $0.archived && !$0.isUnallocated } ?? [] }

    /// Loaded rows minus the one waiting on the undo toast.
    var rows: [TransactionRow] {
        guard let hidden = pendingUndo?.id else { return allRows }
        return allRows.filter { $0.id != hidden }
    }

    func flowName(_ flow: FlowView) -> String {
        flow.isUnallocated ? NameBook.unallocatedLabel : flow.name
    }

    // MARK: - Lifecycle

    /// Loads the vault list and restores the last selected vault, or flags
    /// that onboarding is needed.
    func bootstrap() {
        guarded {
            vaults = try client.vaults()
            guard !vaults.isEmpty else {
                needsOnboarding = true
                return
            }
            needsOnboarding = false
            let stored = defaults.string(forKey: Self.lastVaultKey)
            select(vaults.first { $0.id == stored } ?? vaults[0])
        }
    }

    /// Re-reads everything after the log changed underneath the window: a
    /// sync that rebased the projection, a vault joined from the server, or
    /// a login that relabelled the outbox (`docs/v2/SYNC.md` §5).
    func refreshAfterSync() {
        guarded {
            vaults = try client.vaults()
            guard !vaults.isEmpty else {
                needsOnboarding = true
                currentVault = nil
                reload()
                return
            }
            needsOnboarding = false
            if let current = currentVault, let fresh = vaults.first(where: { $0.id == current.id }) {
                currentVault = fresh
                reload()
            } else {
                let stored = defaults.string(forKey: Self.lastVaultKey)
                select(vaults.first { $0.id == stored } ?? vaults[0])
            }
        }
    }

    func select(_ vault: VaultView) {
        flushPendingUndo()
        currentVault = vault
        defaults.set(vault.id, forKey: Self.lastVaultKey)
        lastWalletId = nil
        lastFlowId = nil
        // Stale until whichever view needs them reloads: the Categories
        // window and the Recurring panel load on appear, not eagerly.
        windowCategories = []
        categoryAliases = []
        recurringTemplates = []
        reload()
    }

    /// Snapshot, categories, the summary aggregates and the first page of
    /// transactions.
    func reload() {
        guard let vault = currentVault else {
            snapshot = nil
            categories = []
            transactions = []
            allRows = []
            nextCursor = nil
            authors = []
            summary = nil
            year = nil
            pendingRecurringItems = []
            return
        }
        guarded {
            snapshot = try client.snapshot(vaultId: vault.id)
            categories = try client.categories(vaultId: vault.id)
            let page = try client.transactions(
                vaultId: vault.id,
                filter: filter,
                limit: Self.pageSize,
                cursor: nil
            )
            transactions = page.items
            nextCursor = page.nextCursor
            authors = try client.authors(vaultId: vault.id)
            // A person who has left the vault's history must not stay
            // selected, or the ledger shows an empty month with no way back.
            if let person, !authors.contains(person) { self.person = nil }
            summary = try loadSummary(vault: vault)
            year = try loadYear(vault: vault)
            pendingRecurringItems = try client.pendingRecurring(vaultId: vault.id, today: CoreDate.day(Date()))
            rebuildRows()
        }
    }

    /// The six aggregate queries behind the panel, loaded as one unit.
    ///
    /// The month and the month before come from a single `bucket_totals` call
    /// with three boundaries, which is the only totals query that takes a
    /// person, so the whole window honours the PERSONA filter. The envelope x
    /// person matrix stays unfiltered on purpose: it *is* the per-person
    /// breakdown, and filtering it would blank every column but one.
    private func loadSummary(vault: VaultView) throws -> LedgerSummary {
        let bounds = month.bounds()
        let previousStart = CoreDate.utcString(month.adding(months: -1).start())
        let pair = try client.bucketTotals(
            vaultId: vault.id,
            bounds: [previousStart, bounds.from, bounds.to],
            person: person
        )
        let empty = PeriodTotals(income: 0, expense: 0, refund: 0, netExpense: 0)
        let trailing = month.trailingYear()
        return LedgerSummary(
            month: month,
            flowPerson: try client.flowPersonTotals(vaultId: vault.id, from: bounds.from, to: bounds.to),
            categories: try client.categoryTotals(
                vaultId: vault.id,
                from: bounds.from,
                to: bounds.to,
                person: person
            ),
            totals: pair.count > 1 ? pair[1] : empty,
            previous: pair.first ?? empty,
            trailing: try client.bucketTotals(vaultId: vault.id, bounds: trailing.bounds, person: person),
            trailingMonths: trailing.months
        )
    }

    /// The RIEPILOGO's year: one `year_breakdown` call with fourteen
    /// boundaries, the epoch and the thirteen month starts, so bucket 0 is
    /// everything before January (`docs/v2/UI.md` §4).
    private func loadYear(vault: VaultView) throws -> YearSummary? {
        let epoch = CoreDate.utcString(Date(timeIntervalSince1970: 0))
        let bounds = [epoch] + MonthKey.yearBounds(month.year)
        let rows = try client.yearBreakdown(vaultId: vault.id, bounds: bounds).map {
            YearRow(
                bucket: Int($0.bucket),
                person: $0.person,
                income: $0.income,
                opening: $0.opening,
                cashExpense: $0.cashExpense,
                fundExpense: $0.fundExpense
            )
        }
        return YearSummary.build(year: month.year, upTo: month, rows: rows, flows: flows)
    }

    /// Appends the next page, if any.
    func loadMore() {
        guard let vault = currentVault, let cursor = nextCursor else { return }
        guarded {
            let page = try client.transactions(
                vaultId: vault.id,
                filter: filter,
                limit: Self.pageSize,
                cursor: cursor
            )
            transactions.append(contentsOf: page.items)
            nextCursor = page.nextCursor
            rebuildRows()
        }
    }

    private var filter: TransactionFilter {
        let bounds = month.bounds()
        let text = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        // Transfers belong to neither direction; the View menu adds them to
        // whichever one is showing rather than replacing it.
        let kinds = showTransfers
            ? direction.kinds + [.transferWallet, .transferFlow]
            : direction.kinds
        return TransactionFilter(
            from: bounds.from,
            to: bounds.to,
            kinds: kinds,
            includeVoided: showVoided,
            includeTransfers: showTransfers,
            walletId: nil,
            flowId: nil,
            text: text.isEmpty ? nil : text,
            author: person,
            ascending: true
        )
    }

    private func rebuildRows() {
        let names = NameBook(snapshot: snapshot)
        allRows = transactions.map { TransactionRow(view: $0, names: names) }
    }

    // MARK: - Vault and entity creation

    /// Onboarding: a vault plus its first wallet, as two commands.
    func createVault(name: String, walletName: String, openingBalance: Int64) {
        guarded {
            let receipt = try client.createVault(name: name)
            guard let vaultId = receipt.resultId else {
                throw DomainError.InvalidCommand(message: "the vault command returned no id")
            }
            let wallet = walletName.trimmingCharacters(in: .whitespacesAndNewlines)
            if !wallet.isEmpty {
                try client.execute(
                    vaultId: vaultId,
                    .createWallet(
                        name: wallet,
                        openingBalance: openingBalance,
                        occurredAt: CoreDate.offset(Date())
                    )
                )
            }
            vaults = try client.vaults()
            needsOnboarding = false
            if let created = vaults.first(where: { $0.id == vaultId }) {
                select(created)
            }
        }
    }

    func createWallet(name: String, openingBalance: Int64) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(
                vaultId: vault.id,
                .createWallet(
                    name: name,
                    openingBalance: openingBalance,
                    occurredAt: CoreDate.offset(Date())
                )
            )
            reload()
        }
    }

    func createEnvelope(name: String, mode: FlowMode, allowNegative: Bool, openingAllocation: Int64) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(
                vaultId: vault.id,
                .createFlow(
                    name: name,
                    mode: mode,
                    allowNegative: allowNegative,
                    openingAllocation: openingAllocation,
                    occurredAt: CoreDate.offset(Date())
                )
            )
            reload()
        }
    }

    // MARK: - Wallet and envelope management

    func renameWallet(_ walletId: Uuid, name: String) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(vaultId: vault.id, .renameWallet(walletId: walletId, name: name))
            reload()
        }
    }

    /// Requires a zero balance in the core; the error surfaces as-is
    /// (docs task 2: no client-side pre-validation).
    func archiveWallet(_ walletId: Uuid) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(vaultId: vault.id, .archiveWallet(walletId: walletId))
            reload()
        }
    }

    func restoreWallet(_ walletId: Uuid) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(vaultId: vault.id, .restoreWallet(walletId: walletId))
            reload()
        }
    }

    /// Only the given fields change; Unallocated cannot be updated
    /// (`.updateFlow`, `docs/v2/ARCH.md` §4).
    func updateEnvelope(_ flowId: Uuid, name: String? = nil, mode: FlowMode? = nil, allowNegative: Bool? = nil) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(
                vaultId: vault.id,
                .updateFlow(flowId: flowId, name: name, mode: mode, allowNegative: allowNegative)
            )
            reload()
        }
    }

    /// Requires a zero balance in the core; the error surfaces as-is.
    func archiveEnvelope(_ flowId: Uuid) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(vaultId: vault.id, .archiveFlow(flowId: flowId))
            reload()
        }
    }

    func restoreEnvelope(_ flowId: Uuid) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(vaultId: vault.id, .restoreFlow(flowId: flowId))
            reload()
        }
    }

    // MARK: - Category management (Categories window)

    /// Loads both the management list (archived included) and the aliases;
    /// called when the Categories window appears (docs task 3).
    func loadCategoryManagement() {
        guard let vault = currentVault else {
            windowCategories = []
            categoryAliases = []
            return
        }
        guarded {
            windowCategories = try client.categories(vaultId: vault.id, includeArchived: true)
            categoryAliases = try client.aliases(vaultId: vault.id)
        }
    }

    /// Active categories near `name`, nearest first; empty on a blank name
    /// or any core error. Used as a live, non-blocking hint while typing
    /// (docs/v2/DISTILLATO_V1.md §2.1: "suggest, don't block").
    func similarCategories(name: String) -> [CategoryView] {
        guard let vault = currentVault, !name.trimmingCharacters(in: .whitespaces).isEmpty else { return [] }
        return (try? client.similarCategories(vaultId: vault.id, name: name)) ?? []
    }

    /// What `mergeCategory` would refuse, without changing anything.
    func previewCategoryMerge(sourceId: Uuid, targetId: Uuid) -> MergePreview? {
        guard let vault = currentVault else { return nil }
        return try? client.previewMerge(vaultId: vault.id, sourceId: sourceId, targetId: targetId)
    }

    func createCategory(name: String) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(vaultId: vault.id, .createCategory(name: name))
            reloadCategories()
        }
    }

    /// System categories cannot be renamed; the core refuses it.
    func renameCategory(_ categoryId: Uuid, name: String) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(vaultId: vault.id, .renameCategory(categoryId: categoryId, name: name))
            reloadCategories()
        }
    }

    func archiveCategory(_ categoryId: Uuid) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(vaultId: vault.id, .archiveCategory(categoryId: categoryId))
            reloadCategories()
        }
    }

    func restoreCategory(_ categoryId: Uuid) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(vaultId: vault.id, .restoreCategory(categoryId: categoryId))
            reloadCategories()
        }
    }

    func addAlias(categoryId: Uuid, alias: String) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(vaultId: vault.id, .addAlias(categoryId: categoryId, alias: alias))
            reloadCategories()
        }
    }

    func removeAlias(categoryId: Uuid, alias: String) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(vaultId: vault.id, .removeAlias(categoryId: categoryId, alias: alias))
            reloadCategories()
        }
    }

    /// Refused when `previewCategoryMerge` reports conflicts; repoints every
    /// transaction of `sourceId` to `targetId` and deletes the source
    /// (`docs/v2/ARCH.md` §4).
    func mergeCategory(sourceId: Uuid, targetId: Uuid) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(vaultId: vault.id, .mergeCategory(sourceId: sourceId, targetId: targetId))
            reloadCategories()
        }
    }

    /// Refreshes the picker list and the management window's list together,
    /// after any category command. A full `reload()` because a rename
    /// propagates its denormalized name onto transactions and a merge
    /// repoints them (`docs/v2/DISTILLATO_V1.md` §2.1): the loaded
    /// transactions page can be stale, not just the category lists.
    private func reloadCategories() {
        guard let vault = currentVault else { return }
        reload()
        windowCategories = (try? client.categories(vaultId: vault.id, includeArchived: true)) ?? windowCategories
        categoryAliases = (try? client.aliases(vaultId: vault.id)) ?? categoryAliases
    }

    // MARK: - Recurring

    /// Every template, active and archived; called when the Recurring panel
    /// opens (`pendingRecurringItems` itself is kept current by `reload()`).
    func loadRecurringTemplates() {
        guard let vault = currentVault else {
            recurringTemplates = []
            return
        }
        guarded {
            recurringTemplates = try client.listRecurring(vaultId: vault.id, includeArchived: true)
        }
    }

    func createRecurring(
        kind: TransactionKind,
        amount: Int64,
        walletId: Uuid?,
        flowId: Uuid?,
        category: String?,
        note: String?,
        schedule: Schedule
    ) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(
                vaultId: vault.id,
                .createRecurring(
                    transactionKind: kind,
                    amount: amount,
                    walletId: walletId,
                    flowId: flowId,
                    category: category,
                    note: note,
                    schedule: schedule
                )
            )
            loadRecurringTemplates()
            reload()
        }
    }

    func updateRecurring(_ recurringId: Uuid, patch: RecurringPatch) {
        guard let vault = currentVault, !patch.isEmpty else { return }
        guarded {
            try client.execute(vaultId: vault.id, .updateRecurring(recurringId: recurringId, patch: patch))
            loadRecurringTemplates()
            reload()
        }
    }

    func archiveRecurring(_ recurringId: Uuid) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(vaultId: vault.id, .archiveRecurring(recurringId: recurringId))
            loadRecurringTemplates()
            reload()
        }
    }

    /// Materializes `periodDate` as a transaction; `occurredAt` is the due
    /// date at the current time of day, in the system offset (team-lead
    /// task 4).
    func executeRecurring(_ recurringId: Uuid, periodDate: NaiveDate) {
        guard let vault = currentVault else { return }
        guarded {
            let now = Date()
            let occurredAt = Self.combine(day: periodDate, timeOf: now)
            try client.execute(
                vaultId: vault.id,
                .executeRecurring(recurringId: recurringId, periodDate: periodDate, occurredAt: occurredAt)
            )
            loadRecurringTemplates()
            reload()
        }
    }

    func skipRecurring(_ recurringId: Uuid, periodDate: NaiveDate) {
        guard let vault = currentVault else { return }
        guarded {
            try client.execute(vaultId: vault.id, .skipRecurring(recurringId: recurringId, periodDate: periodDate))
            loadRecurringTemplates()
            reload()
        }
    }

    /// `periodDate` (a bare day) at today's time of day, in the system
    /// offset: what `executeRecurring` sends as `occurredAt`.
    /// The calendar day of `day` with the clock time of `reference`: moving a
    /// row to another day must not silently move it to midnight.
    static func stamp(day: Date, likeTimeOf reference: Date, calendar: Calendar = .current) -> OffsetDateTime {
        let time = calendar.dateComponents([.hour, .minute, .second], from: reference)
        let combined = calendar.date(
            bySettingHour: time.hour ?? 0,
            minute: time.minute ?? 0,
            second: time.second ?? 0,
            of: day
        )
        return CoreDate.offset(combined ?? day)
    }

    private static func combine(day: NaiveDate, timeOf now: Date) -> OffsetDateTime {
        guard let dayDate = CoreDate.localDay(day) else { return CoreDate.offset(now) }
        let calendar = Calendar.current
        let time = calendar.dateComponents([.hour, .minute, .second], from: now)
        let combined = calendar.date(bySettingHour: time.hour ?? 0, minute: time.minute ?? 0, second: time.second ?? 0, of: dayDate)
        return CoreDate.offset(combined ?? now)
    }

    // MARK: - Ledger writes
    //
    // The grid writes the same commands the quick-add line does; the only
    // difference is where the fields come from (`docs/v2/UI.md` §2.1).

    /// The wallet a new row lands on. The core reads `nil` as "the only
    /// active wallet", which is exactly right for a one-wallet vault and an
    /// error for any other, so a vault with several falls back to the last
    /// one written to, then to the first.
    var defaultWalletId: Uuid? {
        if let lastWalletId, wallets.contains(where: { $0.id == lastWalletId }) { return lastWalletId }
        return wallets.count == 1 ? nil : wallets.first?.id
    }

    /// The envelope a new row lands on: the last one written to, else the
    /// first that is not Unallocated, else Unallocated.
    var defaultFlowId: Uuid? {
        if let lastFlowId, flows.contains(where: { $0.id == lastFlowId }) { return lastFlowId }
        return flows.first { !$0.isUnallocated }?.id ?? flows.first?.id
    }

    /// The last row of the loaded month, which ⌘D copies into the new line.
    var lastRow: TransactionRow? { rows.last }

    /// Appends the row typed in the grid's empty last line. `day` carries the
    /// calendar day the DATA cell shows; the time of day comes from the clock,
    /// so rows entered on the same day keep their typing order.
    ///
    /// A `nil` envelope means "the sticky default", not Unallocated: an empty
    /// FLOW cell has to behave like the last row, not like a system envelope
    /// the user never picked.
    func addRow(day: Date, flowId: Uuid?, category: String?, note: String, amount: Int64) {
        guard let vault = currentVault, amount > 0 else { return }
        let trimmedNote = note.trimmingCharacters(in: .whitespacesAndNewlines)
        let envelope = flowId ?? defaultFlowId
        let entry = Entry(
            amount: amount,
            walletId: defaultWalletId,
            flowId: envelope,
            category: category?.trimmingCharacters(in: .whitespacesAndNewlines),
            note: trimmedNote.isEmpty ? nil : trimmedNote,
            occurredAt: Self.combine(day: CoreDate.day(day), timeOf: Date())
        )
        let command: Command = direction == .income ? .income(entry) : .expense(entry)
        guarded {
            try client.execute(vaultId: vault.id, command)
            lastWalletId = entry.walletId ?? lastWalletId
            lastFlowId = envelope ?? lastFlowId
            savedAt = Date()
            reload()
        }
    }

    /// Resolves what was typed in the FLOW cell to an envelope, with the same
    /// precedence the core's quick-add resolver uses: exact, then unique
    /// prefix, then unique substring (`core/src/quick_add.rs`). Returns `nil`
    /// for blank text, meaning "leave the default".
    func resolveFlow(named text: String) throws -> Uuid? {
        let needle = text.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !needle.isEmpty else { return nil }
        let candidates = flows.map { (id: $0.id, name: flowName($0).lowercased()) }
        if let exact = candidates.first(where: { $0.name == needle }) { return exact.id }
        for matches in [
            candidates.filter { $0.name.hasPrefix(needle) },
            candidates.filter { $0.name.contains(needle) },
        ] {
            if matches.count == 1 { return matches[0].id }
            if matches.count > 1 {
                throw DomainError.InvalidCommand(
                    message: String(localized: "More than one envelope matches \u{201C}\(text)\u{201D}")
                )
            }
        }
        throw DomainError.NotFound(message: String(localized: "No envelope named \u{201C}\(text)\u{201D}"))
    }

    /// Surfaces a validation failure from the grid through the same alert the
    /// core's errors use, so a bad cell reads like a refused command.
    func report(_ error: Error) {
        guarded { throw error }
    }

    // MARK: - Quick add

    /// Pure parse, for the live preview line. No database access.
    func preview(quickAdd input: String) -> Result<QuickAdd, QuickAddError> {
        do {
            return .success(try parseQuickAdd(input: input, currency: currency))
        } catch let error as QuickAddError {
            return .failure(error)
        } catch {
            return .failure(.Domain(code: "unexpected", message: error.localizedDescription))
        }
    }

    /// Parses, resolves names against the vault, executes, reloads.
    func submit(quickAdd input: String) {
        guard let vault = currentVault else { return }
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        guarded {
            let parsed = try parseQuickAdd(input: trimmed, currency: currency)
            let resolved = try client.resolveQuickAdd(
                vaultId: vault.id,
                parsed: parsed,
                now: Date(),
                defaults: QuickAddDefaults(walletId: lastWalletId, flowId: lastFlowId)
            )
            try client.execute(vaultId: vault.id, resolved.command)
            savedAt = Date()
            // The core reports what the names resolved to, so the sticky
            // defaults never depend on the shape of the command.
            lastWalletId = resolved.walletId ?? lastWalletId
            lastFlowId = resolved.flowId ?? lastFlowId
            quickAddText = ""
            reload()
        }
    }

    /// Called from the error alert's candidate buttons after an
    /// `ambiguous_name` quick-add error: rewrites the marker that carried
    /// the ambiguous fragment with the chosen name and resubmits (task 1).
    func resolveAmbiguous(choosing candidate: String) {
        guard let error = presentedError, let fragment = error.ambiguousFragment else { return }
        presentedError = nil
        let rewritten = Self.rewrite(quickAddText, fragment: fragment, with: candidate)
        quickAddText = rewritten
        submit(quickAdd: rewritten)
    }

    /// Finds which marker (`#`, `@`, `>`) carried `fragment` and swaps in
    /// `chosen`, preserving the marker. The grammar allows at most one of
    /// each marker (`docs/v2/DISTILLATO_V1.md` §3.1), so the first match is
    /// unambiguous.
    private static func rewrite(_ text: String, fragment: String, with chosen: String) -> String {
        for marker in ["#", "@", ">"] {
            let needle = marker + fragment
            if let range = text.range(of: needle, options: .caseInsensitive) {
                return text.replacingCharacters(in: range, with: marker + chosen)
            }
        }
        return text
    }

    // MARK: - Void with deferred undo

    /// Hides the row and starts the undo window. The command is only sent
    /// when the window elapses or another destructive action starts.
    func void(transactionId: Uuid) {
        flushPendingUndo()
        pendingUndo = PendingUndo(id: transactionId, startedAt: Date(), duration: undoWindow)
        let window = undoWindow
        let sleep = sleeper
        undoTask = Task { [weak self] in
            try? await sleep(window)
            guard !Task.isCancelled else { return }
            self?.flushPendingUndo()
        }
    }

    /// Cancels the pending void; nothing was ever executed.
    func undo() {
        undoTask?.cancel()
        undoTask = nil
        pendingUndo = nil
    }

    /// Applies a pending void now.
    func flushPendingUndo() {
        guard let pending = pendingUndo, let vault = currentVault else { return }
        pendingUndo = nil
        undoTask?.cancel()
        undoTask = nil
        guarded {
            try client.execute(vaultId: vault.id, .voidTransaction(transactionId: pending.id))
            savedAt = Date()
            reload()
        }
    }

    // MARK: - Editing

    /// Sends only the fields that changed.
    func update(transactionId: Uuid, patch: TransactionPatch) {
        guard let vault = currentVault, !patch.isEmpty else { return }
        guarded {
            try client.execute(
                vaultId: vault.id,
                .updateTransaction(transactionId: transactionId, patch: patch)
            )
            savedAt = Date()
            reload()
        }
    }

    // MARK: - Errors

    /// Runs a piece of work, turning any core error into `presentedError`.
    private func guarded(_ work: () throws -> Void) {
        do {
            try work()
        } catch let error as DomainError {
            presentedError = AppError(error)
        } catch let error as QuickAddError {
            presentedError = AppError(error)
        } catch {
            presentedError = AppError(code: "unexpected", message: error.localizedDescription)
        }
    }
}
