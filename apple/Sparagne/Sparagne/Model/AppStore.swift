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
    /// A headline more precise than the code's, when the error says what
    /// it is about: a person nobody here goes by, or several that match.
    private let headline: String?

    /// The localized headline for `code` (`ErrorMessages.swift`); the alert
    /// keeps `message`, the Rust `Display` text, as its secondary detail.
    var summary: String { headline ?? ErrorMessages.summary(for: code) }

    init(
        code: String,
        message: String,
        candidates: [String] = [],
        ambiguousFragment: String? = nil,
        headline: String? = nil
    ) {
        self.code = code
        self.message = message
        self.candidates = candidates
        self.ambiguousFragment = ambiguousFragment
        self.headline = headline
    }

    init(_ error: DomainError) {
        self.init(code: error.code, message: error.message)
    }

    /// `line` is the quick-add line the error is about. An ambiguous name
    /// does not say which marker carried it; the line does, and a `!` makes
    /// it a person.
    init(_ error: QuickAddError, line: String? = nil) {
        var fragment: String?
        var headline: String?
        switch error {
        case .AmbiguousName(let name, _):
            fragment = name
            if let line, AppStore.marker(carrying: name, in: line)?.marker == "!" {
                headline = ErrorMessages.ambiguousPerson
            }
        case .UnknownName(let kind, let name) where kind == "person":
            headline = ErrorMessages.unknownPerson(name)
        default:
            break
        }
        self.init(
            code: error.code,
            message: error.message,
            candidates: error.candidates,
            ambiguousFragment: fragment,
            headline: headline
        )
    }
}

/// A void that has been hidden from the table but not yet applied.
///
/// The rows disappear immediately and a toast counts down; `undo()` cancels,
/// and the window elapsing (or another destructive action, or quitting)
/// commits one `VoidTransaction` per row (`docs/v2/DISTILLATO_V1.md` §2.4).
struct PendingUndo: Identifiable, Equatable, Sendable {
    /// The toast's own identity: a new void is a new toast, even for a row
    /// that was voided, undone and voided again.
    let id = UUID()
    /// The transactions being voided: one for a row, several for a selection.
    let ids: [Uuid]
    /// The vault the rows live in. Not necessarily the one on screen by the
    /// time the window elapses: the user can switch, and a pull can delete it.
    let vaultId: Uuid
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

/// One recurring period waiting for a decision: a template and the day it
/// fell due (`docs/v2/DISTILLATO_V1.md` §2.3).
struct DuePeriod: Identifiable, Hashable, Sendable {
    let template: RecurringView
    let date: NaiveDate

    var id: String { "\(template.id)/\(date)" }
}

/// One recurring period still ahead: a template and a day it will fall due,
/// from `scheduleOccurrences`. What the Ricorrenze tab's agenda lists; never
/// executed from there, since a period only becomes actionable once due.
struct UpcomingPeriod: Identifiable, Hashable, Sendable {
    let template: RecurringView
    let date: NaiveDate

    var id: String { "\(template.id)/\(date)" }
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
///
/// The state lives on the main actor and is written there; the core lives on
/// `CoreActor` and is reached only by awaiting it, so a long query costs a
/// suspension and not a frozen window (`docs/v2/ARCH.md` §8). Every entry
/// point that touches the core is therefore `async`: views call them from a
/// `Task`, tests await them.
@Observable
@MainActor
final class AppStore {
    /// A month of a personal ledger fits in one page in practice. A month that
    /// does not goes on page by page as the grid scrolls to its end
    /// (`loadMore`), and the export reads every page first (`loadAll`).
    static let pageSize: UInt32 = 1000
    static let lastVaultKey = "lastVaultId"
    /// Shared with the View menu's `@AppStorage` toggle, so the preference
    /// has one home (`docs/v2/UI.md` §3).
    static let walletColumnKey = "showWalletColumn"

    // MARK: Dependencies

    /// Not private: `SyncEngine` awaits the same actor, and the tests write
    /// rows as a second member of the vault.
    @ObservationIgnored let core: CoreActor
    @ObservationIgnored private let defaults: UserDefaults
    @ObservationIgnored private let undoWindow: Duration
    /// Injected so tests do not wait out the real undo window.
    @ObservationIgnored private let sleeper: @Sendable (Duration) async throws -> Void
    /// What `createVault` gives a new vault (`DefaultCategories`). The app
    /// passes the list for its language; the tests leave it empty, so their
    /// vaults hold only the system categories and every `#name` they type is
    /// new.
    @ObservationIgnored private let defaultCategories: [DefaultCategory]
    /// Edit ▸ Undo and Redo for the ledger's writes, on the window's undo
    /// manager once the window hands it over (`AppStore+History.swift`).
    @ObservationIgnored let history = LedgerHistory()

    // MARK: Vaults

    private(set) var vaults: [VaultView] = []
    private(set) var currentVault: VaultView?
    /// True when the account has no vault yet and onboarding must run.
    private(set) var needsOnboarding = false
    /// Vaults the account may only read (a viewer's): `CoreActor` refuses to
    /// write to them and the views hide what would write. Fed by the sync
    /// engine from the server's roles.
    private(set) var readOnlyVaultIds: Set<Uuid> = []

    func setReadOnlyVaults(_ ids: Set<Uuid>) {
        readOnlyVaultIds = ids
        // Every bulk action writes: rows picked before the role changed have
        // nothing left to be picked for.
        if isReadOnly { selection.clear() }
    }

    /// Whether the vault on screen is read-only for this account.
    var isReadOnly: Bool {
        currentVault.map { readOnlyVaultIds.contains($0.id) } ?? false
    }

    /// Whether the window may offer to write at all: a vault on screen that
    /// this account does not only read. The grid's empty line, the setup
    /// tables' add lines and every cell that opens for editing follow it.
    var canWrite: Bool { currentVault != nil && !isReadOnly }

    /// The usernames of each vault's members, as the sync engine last heard
    /// them from the server; empty while logged out (`SyncEngine
    /// .publishMembers`).
    private(set) var vaultMembers: [Uuid: [String]] = [:]

    func setVaultMembers(_ members: [Uuid: [String]]) {
        vaultMembers = members
    }

    // MARK: Loaded data

    private(set) var snapshot: VaultSnapshot?
    private(set) var categories: [CategoryView] = []
    private(set) var transactions: [TransactionView] = []
    private(set) var nextCursor: String?
    private(set) var allRows: [TransactionRow] = []
    /// The persons of the vault's live rows (`people()`): the PERSONA
    /// segmented control, and the names the summaries break down by. Who
    /// may be named on a new row is `assignablePeople`, a different list:
    /// a member who never had a row is in that one only, a former member
    /// with rows left behind in this one only.
    private(set) var peopleInRows: [String] = []
    /// Everything the ledger's summary panel draws.
    private(set) var summary: LedgerSummary?
    /// Everything the RIEPILOGO draws: the year of `month`, up to `month`
    /// (`docs/v2/UI.md` §2.2).
    private(set) var year: YearSummary?
    /// When the last command was applied, for the status bar's "saved at".
    private(set) var savedAt: Date?

    // MARK: Category management (the SETUP tab)

    /// Active and archived categories, for the Categories table;
    /// `categories` above stays active-only, for pickers.
    private(set) var windowCategories: [CategoryView] = []
    private(set) var categoryAliases: [AliasView] = []

    // MARK: Category completion

    /// The categories used in the last 90 days, most recent first: the tier
    /// the CATEGORY cells' list puts first (`CategoryCompletion`).
    private(set) var recentCategoryIds: [Uuid] = []
    /// Whether a write may have changed what is recent since the last load
    /// (`loadCategoryCompletion`).
    @ObservationIgnored private var completionIsStale = true

    // MARK: Recurring

    /// Periods still waiting for a decision, refreshed on every `reload()`
    /// so the top bar's due pill (`DuePill`, `Views/Chrome/TopBar.swift`), the
    /// Ricorrenze tab and the Mastro's pending rows stay current without a
    /// separate poll.
    private(set) var pendingRecurringItems: [PendingRecurring] = []
    /// Every template, active and archived; loaded on demand when the
    /// Recurring panel opens.
    private(set) var recurringTemplates: [RecurringView] = []
    /// The periods of the next days, soonest first, for the Ricorrenze tab's
    /// agenda. Written only by that tab's loader (`AppStore+Recurring.swift`),
    /// hence not `private(set)`: an extension in another file has to set it.
    var upcomingRecurring: [UpcomingPeriod] = []

    // MARK: Setup

    /// Rows per category id over the last 90 days, voided excluded: the
    /// usage column of the SETUP categories. Written only by its loader
    /// (`AppStore+Usage.swift`), so not `private(set)` for the same reason.
    var categoryUsage: [Uuid: Int] = [:]

    // MARK: Filters and view state

    /// The month the ledger reads and writes (`docs/v2/UI.md` §2.1). Changing
    /// it reloads the rows and every aggregate together, so the panel never
    /// describes a different month from the table.
    ///
    /// A `didSet` cannot await, so it queues the reload; `settle()` is how a
    /// caller waits for the queue to drain.
    var month = MonthKey(Date()) { didSet { if month != oldValue { filtersChanged() } } }
    var direction: LedgerDirection = .expenses { didSet { if direction != oldValue { filtersChanged() } } }
    /// The PERSONA filter: `nil` is everybody.
    var person: String? { didSet { if person != oldValue { filtersChanged() } } }
    /// Which of the two views is on screen; no reload, the data is the same.
    /// The window opens on the RIEPILOGO (`docs/v2/UI.md` §2).
    var tab: LedgerTab = .summary { didSet { if tab != oldValue { tabFilter = "" } } }
    /// What the top bar's search field holds on the Ricorrenze and the Setup
    /// tabs, which filter their tables locally. Apart from `searchText` so
    /// that typing there never reloads the ledger; cleared with the tab and
    /// the vault, since it named rows of the table that is gone.
    var tabFilter = ""
    var showVoided = false { didSet { if showVoided != oldValue { filtersChanged() } } }
    /// Transfers are in neither direction, so the View menu opts into them.
    var showTransfers = false { didSet { if showTransfers != oldValue { filtersChanged() } } }
    /// The optional WALLET column (`docs/v2/UI.md` §3). Display only: it
    /// changes what the grid draws and what ⌘E writes, never what is loaded,
    /// so it does not reload.
    var showWalletColumn: Bool {
        didSet {
            if showWalletColumn != oldValue { defaults.set(showWalletColumn, forKey: Self.walletColumnKey) }
        }
    }
    /// Debounced by the view; call `reload()` when it settles. The selection
    /// goes at the first keystroke, not with the reload: it named rows of the
    /// list being narrowed.
    var searchText = "" { didSet { if searchText != oldValue { selection.clear() } } }
    var quickAddText = ""

    // MARK: Selection

    /// The rows picked for a bulk action, with the gestures and the actions
    /// in `AppStore+Selection.swift` (which is why the setter is not private).
    /// It names rows of one list, so the month, the vault and every filter
    /// clear it.
    var selection = RowSelection()

    // MARK: Transient

    private(set) var pendingUndo: PendingUndo?
    @ObservationIgnored private(set) var undoTask: Task<Void, Never>?
    var presentedError: AppError?

    /// Sticky quick-add defaults: the wallet and envelope last written to.
    private(set) var lastWalletId: Uuid?
    private(set) var lastFlowId: Uuid?

    /// The queued reload, if any, and a counter that says whether a new one
    /// was queued while the last was running (`settle`).
    @ObservationIgnored private var queuedLoad: Task<Void, Never>?
    @ObservationIgnored private var queuedGeneration = 0
    /// Which reload may write the state: a slower one that started earlier
    /// must not overwrite a newer month with what it found.
    @ObservationIgnored private var loadGeneration = 0
    /// The filter the rows on screen were loaded with. The next page has to
    /// continue that query, not whatever the search field says now.
    @ObservationIgnored private var loadedFilter: TransactionFilter?
    /// The next page on its way, if any (`loadMore`).
    @ObservationIgnored private var pageLoad: Task<Void, Never>?

    init(
        core: CoreActor,
        defaults: UserDefaults = .standard,
        undoWindow: Duration = .seconds(5),
        sleeper: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) },
        defaultCategories: [DefaultCategory] = []
    ) {
        self.core = core
        self.defaults = defaults
        self.undoWindow = undoWindow
        self.sleeper = sleeper
        self.defaultCategories = defaultCategories
        showWalletColumn = defaults.bool(forKey: Self.walletColumnKey)
        currentAuthor = core.initialAuthor
    }

    /// The engine calls this on login and logout, and when Settings change
    /// the local name: the core stamps it on every command from now on.
    func setAuthor(_ name: String) async {
        await core.setAuthor(name)
        currentAuthor = name
    }

    // MARK: - Derived

    var currency: Currency { snapshot?.currency ?? .eur }

    /// Who the log will credit the next command to: the PERSONA of a new row.
    /// Stored rather than read off the actor, so the empty line redraws when
    /// the engine or Settings change the name (`setAuthor`).
    private(set) var currentAuthor: String
    var currencyCode: String { currency.code }

    /// Active wallets, for the pickers.
    var wallets: [WalletView] { snapshot?.wallets.filter { !$0.archived } ?? [] }

    /// Active envelopes, Unallocated first (the core already orders them).
    var flows: [FlowView] { snapshot?.flows.filter { !$0.archived } ?? [] }

    /// The archived rows at the bottom of the SETUP tables, where they can be
    /// restored.
    var archivedWallets: [WalletView] { snapshot?.wallets.filter { $0.archived } ?? [] }

    /// Unallocated is never archived, so it never needs to appear here.
    var archivedFlows: [FlowView] { snapshot?.flows.filter { $0.archived && !$0.isUnallocated } ?? [] }

    /// Loaded rows minus those waiting on the undo toast.
    var rows: [TransactionRow] {
        guard let pending = pendingUndo, pending.vaultId == currentVault?.id else { return allRows }
        let hidden = Set(pending.ids)
        return allRows.filter { !hidden.contains($0.id) }
    }

    func flowName(_ flow: FlowView) -> String {
        flow.isUnallocated ? NameBook.unallocatedLabel : flow.name
    }

    // MARK: - The load queue

    /// A filter changed: what is selected belonged to the old list, and the
    /// new one has to be loaded.
    private func filtersChanged() {
        selection.clear()
        scheduleReload()
    }

    /// Queues a reload behind whatever is already loading. The filters are
    /// plain properties so the views can bind to them, and a `didSet` cannot
    /// await.
    private func scheduleReload() {
        queuedGeneration += 1
        let previous = queuedLoad
        queuedLoad = Task { @MainActor [weak self] in
            await previous?.value
            await self?.reload()
        }
    }

    /// Waits until every queued reload has finished, including any a reload
    /// queued itself (a stale PERSONA being cleared). The views never need
    /// this; anything that reads the store straight after writing a filter
    /// does. An undo or redo pressed before is written first.
    func settle() async {
        await history.settle()
        while queuedLoad != nil {
            let generation = queuedGeneration
            await queuedLoad?.value
            if generation == queuedGeneration {
                queuedLoad = nil
                return
            }
        }
    }

    // MARK: - Lifecycle

    /// Loads the vault list and restores the last selected vault, or flags
    /// that onboarding is needed.
    func bootstrap() async {
        await guarded {
            vaults = try await core.vaults()
            guard !vaults.isEmpty else {
                needsOnboarding = true
                return
            }
            needsOnboarding = false
            let stored = defaults.string(forKey: Self.lastVaultKey)
            await select(vaults.first { $0.id == stored } ?? vaults[0])
        }
    }

    /// Re-reads everything after the log changed underneath the window: a
    /// sync that rebased the projection, a vault joined from the server, or
    /// a login that relabelled the outbox (`docs/v2/SYNC.md` §5).
    func refreshAfterSync() async {
        await guarded { try await adoptVaultList() }
    }

    /// Re-reads the vault list and keeps the selection where it can: the
    /// same vault when it still exists (fresh, so a rename shows), the
    /// remembered one or the first otherwise, onboarding when none is left.
    private func adoptVaultList() async throws {
        vaults = try await core.vaults()
        // A pull can delete the vault a void is waiting in. Its rows went
        // with it, so the void has nothing left to do, and flushing it on
        // the way to another vault would only bring back a "Not found".
        if let pending = pendingUndo, !vaults.contains(where: { $0.id == pending.vaultId }) {
            undo()
        }
        guard !vaults.isEmpty else {
            needsOnboarding = true
            currentVault = nil
            history.clear()
            await reload()
            return
        }
        needsOnboarding = false
        if let current = currentVault, let fresh = vaults.first(where: { $0.id == current.id }) {
            currentVault = fresh
            await reload()
        } else {
            let stored = defaults.string(forKey: Self.lastVaultKey)
            await select(vaults.first { $0.id == stored } ?? vaults[0])
        }
    }

    func select(_ vault: VaultView) async {
        currentVault = vault
        defaults.set(vault.id, forKey: Self.lastVaultKey)
        lastWalletId = nil
        lastFlowId = nil
        // Stale until whichever view needs them reloads: the SETUP tab and
        // the Recurring panel load on appear, not eagerly.
        windowCategories = []
        categoryAliases = []
        recentCategoryIds = []
        recurringTemplates = []
        upcomingRecurring = []
        categoryUsage = [:]
        selection.clear()
        tabFilter = ""
        // A void still counting down is not undone by leaving: it carries its
        // own vault, so it lands where its rows are, and the vault it left
        // is not reloaded for nothing.
        await flushPendingUndo()
        // The steps name rows of the vault being left: ⌘Z here must not
        // reach back into it.
        history.clear()
        await reload()
    }

    /// Snapshot, categories, the summary aggregates and the first page of
    /// transactions: one visit to the core, one consistent window.
    func reload() async {
        guard let vault = currentVault else {
            snapshot = nil
            categories = []
            transactions = []
            allRows = []
            nextCursor = nil
            loadedFilter = nil
            peopleInRows = []
            summary = nil
            year = nil
            pendingRecurringItems = []
            return
        }
        loadGeneration += 1
        let generation = loadGeneration
        completionIsStale = true
        let request = loadRequest(vaultId: vault.id)
        await guarded {
            let loaded = try await core.load(request)
            // A newer month asked for its own load while this one was in the
            // air: the answer on the table is the stale one, so drop it.
            guard generation == loadGeneration else { return }
            snapshot = loaded.snapshot
            categories = loaded.categories
            transactions = loaded.page.items
            nextCursor = loaded.page.nextCursor
            loadedFilter = request.filter
            peopleInRows = loaded.people
            pendingRecurringItems = loaded.pendingRecurring
            summary = Self.summary(month: month, from: loaded)
            year = Self.year(month: month, from: loaded, flows: flows)
            rebuildRows()
            // A sync or a void can take selected rows away: they must not
            // ride along, unseen, in the next bulk action.
            selection.retain(Set(rows.map(\.id)))
            // A person who has left the vault's history must not stay
            // selected, or the ledger shows an empty month with no way back.
            // Clearing it queues the reload that fetches the whole month.
            if let person, !peopleInRows.contains(person) { self.person = nil }
        }
    }

    /// The window on screen, as the core wants it.
    private func loadRequest(vaultId: Uuid) -> VaultLoadRequest {
        let bounds = month.bounds()
        let trailing = month.trailingYear()
        let epoch = CoreDate.utcString(Date(timeIntervalSince1970: 0))
        return VaultLoadRequest(
            vaultId: vaultId,
            filter: filter,
            limit: Self.pageSize,
            monthFrom: bounds.from,
            monthTo: bounds.to,
            previousStart: CoreDate.utcString(month.adding(months: -1).start()),
            trailingBounds: trailing.bounds,
            yearBounds: [epoch] + MonthKey.yearBounds(month.year),
            person: person,
            today: CoreDate.day(Date())
        )
    }

    /// The panel's six aggregates, as they came back from one visit.
    private static func summary(month: MonthKey, from loaded: VaultLoad) -> LedgerSummary {
        let empty = PeriodTotals(income: 0, expense: 0, refund: 0, netExpense: 0)
        return LedgerSummary(
            month: month,
            flowPerson: loaded.flowPerson,
            categories: loaded.categoryTotals,
            totals: loaded.monthPair.count > 1 ? loaded.monthPair[1] : empty,
            previous: loaded.monthPair.first ?? empty,
            trailing: loaded.trailing,
            trailingMonths: month.trailingYear().months
        )
    }

    /// The RIEPILOGO's year: the `year_breakdown` rows of the same visit,
    /// folded into the table the view draws (`docs/v2/UI.md` §4).
    private static func year(month: MonthKey, from loaded: VaultLoad, flows: [FlowView]) -> YearSummary? {
        let rows = loaded.year.map {
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

    /// Appends the next page, if any: the grid asks when its last row comes
    /// into view. A caller that arrives while a page is already on its way
    /// waits for that one instead of asking twice for the same cursor.
    func loadMore() async {
        if let inFlight = pageLoad {
            await inFlight.value
            return
        }
        guard let vault = currentVault, let cursor = nextCursor, let filter = loadedFilter else { return }
        let generation = loadGeneration
        let load = Task { @MainActor [weak self] in
            guard let self else { return }
            await guarded {
                let page = try await core.transactions(
                    vaultId: vault.id,
                    filter: filter,
                    limit: Self.pageSize,
                    cursor: cursor
                )
                // A reload landed meanwhile: its first page replaced the rows
                // this one would continue.
                guard generation == loadGeneration, cursor == nextCursor else { return }
                transactions.append(contentsOf: page.items)
                nextCursor = page.nextCursor
                rebuildRows()
            }
        }
        pageLoad = load
        await load.value
        pageLoad = nil
    }

    /// Every remaining page of the month, for what must see all of it and
    /// not just what has been scrolled to: the ⌘E export.
    func loadAll() async {
        while let cursor = nextCursor {
            await loadMore()
            // A refused page (the alert is up) or a reload that took over:
            // stop rather than ask for the same cursor for ever.
            if nextCursor == cursor { return }
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
            person: person,
            ascending: true
        )
    }

    private func rebuildRows() {
        let names = NameBook(snapshot: snapshot)
        allRows = transactions.map { TransactionRow(view: $0, names: names) }
    }

    // MARK: - Vault and entity creation

    /// Onboarding: a vault, then what it starts with (`firstContents`) as one
    /// batch.
    func createVault(name: String, walletName: String, openingBalance: Int64) async {
        await guarded {
            let receipt = try await core.createVault(name: name)
            guard let vaultId = receipt.resultId else {
                throw DomainError.InvalidCommand(message: "the vault command returned no id")
            }
            try await core.executeBatch(
                envelopes: await firstContents(of: vaultId, walletName: walletName, openingBalance: openingBalance)
            )
            vaults = try await core.vaults()
            needsOnboarding = false
            if let created = vaults.first(where: { $0.id == vaultId }) {
                await select(created)
            }
        }
    }

    /// The first wallet, when the sheet named one, and `defaultCategories`
    /// with their aliases. Minted up front: an alias names its category by
    /// id, and the id is the command's.
    private func firstContents(of vaultId: Uuid, walletName: String, openingBalance: Int64) async -> [CommandEnvelope] {
        var envelopes: [CommandEnvelope] = []
        let wallet = walletName.trimmingCharacters(in: .whitespacesAndNewlines)
        if !wallet.isEmpty {
            let command = Command.createWallet(
                name: wallet,
                openingBalance: openingBalance,
                occurredAt: CoreDate.offset(Date())
            )
            envelopes.append(await core.envelope(vaultId: vaultId, command))
        }
        for category in defaultCategories {
            let created = await core.envelope(vaultId: vaultId, .createCategory(name: category.name))
            envelopes.append(created)
            for alias in category.aliases {
                envelopes.append(await core.envelope(vaultId: vaultId, .addAlias(categoryId: created.id, alias: alias)))
            }
        }
        return envelopes
    }

    // MARK: - Vault management

    /// Renames a vault everywhere its name shows: the picker, the title, the
    /// palette. The name is a label, so two vaults may share it, and the
    /// currency never changes (`docs/v2/ARCH.md` §4).
    func renameVault(_ vaultId: Uuid, name: String) async {
        await guarded {
            try await core.execute(vaultId: vaultId, .renameVault(name: name))
            try await adoptVaultList()
        }
    }

    /// Deletes a vault with everything in it, for every member and on every
    /// device once the command has synced (`docs/v2/SYNC.md` §4.6). Only the
    /// owner may; the core refuses anyone else. The window moves to another
    /// vault, or back to onboarding when it was the last one.
    func deleteVault(_ vaultId: Uuid) async {
        // A row waiting on the undo toast dies with its vault: voiding it
        // now would only be refused.
        if pendingUndo?.vaultId == vaultId { undo() }
        await guarded {
            try await core.execute(vaultId: vaultId, .deleteVault)
            try await adoptVaultList()
        }
    }

    func createWallet(name: String, openingBalance: Int64) async {
        guard let vault = currentVault, !refusedAsReadOnly() else { return }
        await guarded {
            try await core.execute(
                vaultId: vault.id,
                .createWallet(
                    name: name,
                    openingBalance: openingBalance,
                    occurredAt: CoreDate.offset(Date())
                )
            )
            await reload()
        }
    }

    func createEnvelope(name: String, mode: FlowMode, allowNegative: Bool, openingAllocation: Int64) async {
        guard let vault = currentVault, !refusedAsReadOnly() else { return }
        await guarded {
            try await core.execute(
                vaultId: vault.id,
                .createFlow(
                    name: name,
                    mode: mode,
                    allowNegative: allowNegative,
                    openingAllocation: openingAllocation,
                    occurredAt: CoreDate.offset(Date())
                )
            )
            await reload()
        }
    }

    // MARK: - Wallet and envelope management

    func renameWallet(_ walletId: Uuid, name: String) async {
        await command(.renameWallet(walletId: walletId, name: name))
    }

    /// Requires a zero balance in the core; the error surfaces as-is
    /// (docs task 2: no client-side pre-validation).
    func archiveWallet(_ walletId: Uuid) async {
        await command(.archiveWallet(walletId: walletId))
    }

    func restoreWallet(_ walletId: Uuid) async {
        await command(.restoreWallet(walletId: walletId))
    }

    /// Only the given fields change; Unallocated cannot be updated
    /// (`.updateFlow`, `docs/v2/ARCH.md` §4).
    func updateEnvelope(
        _ flowId: Uuid,
        name: String? = nil,
        mode: FlowMode? = nil,
        allowNegative: Bool? = nil
    ) async {
        await command(.updateFlow(flowId: flowId, name: name, mode: mode, allowNegative: allowNegative))
    }

    /// Requires a zero balance in the core; the error surfaces as-is.
    func archiveEnvelope(_ flowId: Uuid) async {
        await command(.archiveFlow(flowId: flowId))
    }

    func restoreEnvelope(_ flowId: Uuid) async {
        await command(.restoreFlow(flowId: flowId))
    }

    /// One command against the current vault, then the reload that shows what
    /// it did. The shape every management action has.
    private func command(_ command: Command) async {
        guard let vault = currentVault, !refusedAsReadOnly() else { return }
        await guarded {
            try await core.execute(vaultId: vault.id, command)
            await reload()
        }
    }

    // MARK: - Category management (the SETUP tab)

    /// Loads both the management list (archived included) and the aliases;
    /// called when the SETUP tab appears (docs task 3).
    func loadCategoryManagement() async {
        guard let vault = currentVault else {
            windowCategories = []
            categoryAliases = []
            return
        }
        await guarded {
            let management = try await core.categoryManagement(vaultId: vault.id)
            windowCategories = management.categories
            categoryAliases = management.aliases
        }
    }

    /// What the CATEGORY cells complete from besides `categories`: the aliases,
    /// and the categories used in the last 90 days (`DISTILLATO_V1.md` §3.2).
    /// Loaded when a cell takes the caret, and only again after a reload:
    /// most reloads are never followed by typing a category.
    ///
    /// Silent on failure, like `similarCategories`: the list is a hint, and a
    /// cell without it still takes any name.
    func loadCategoryCompletion() async {
        guard completionIsStale, let vault = currentVault else { return }
        completionIsStale = false
        let since = Calendar.current.date(byAdding: .day, value: -90, to: Date()) ?? Date()
        guard let usage = try? await core.recentUsage(vaultId: vault.id, since: CoreDate.utcString(since), limit: 50),
              let aliases = try? await core.aliases(vaultId: vault.id),
              currentVault?.id == vault.id
        else {
            completionIsStale = true
            return
        }
        recentCategoryIds = usage.categories
        categoryAliases = aliases
    }

    /// The category the vault's history files `note` under, learned by the
    /// core from past notes over the last year (`suggest_categories`); `nil`
    /// when it has nothing to say. Only ever offered, never applied unseen.
    func suggestedCategory(forNote note: String) async -> String? {
        let trimmed = note.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let vault = currentVault, !trimmed.isEmpty else { return nil }
        let since = Calendar.current.date(byAdding: .year, value: -1, to: Date()) ?? Date()
        let suggestions = try? await core.suggestCategories(
            vaultId: vault.id,
            notes: [trimmed],
            since: CoreDate.utcString(since)
        )
        return suggestions?.first.flatMap { $0?.name }
    }

    /// Active categories near `name`, nearest first; empty on a blank name
    /// or any core error. Used as a live, non-blocking hint while typing
    /// (docs/v2/DISTILLATO_V1.md §2.1: "suggest, don't block").
    func similarCategories(name: String) async -> [CategoryView] {
        guard let vault = currentVault, !name.trimmingCharacters(in: .whitespaces).isEmpty else { return [] }
        return (try? await core.similarCategories(vaultId: vault.id, name: name)) ?? []
    }

    /// What `mergeCategory` would refuse, without changing anything.
    func previewCategoryMerge(sourceId: Uuid, targetId: Uuid) async -> MergePreview? {
        guard let vault = currentVault else { return nil }
        return try? await core.previewMerge(vaultId: vault.id, sourceId: sourceId, targetId: targetId)
    }

    func createCategory(name: String) async {
        await categoryCommand(.createCategory(name: name))
    }

    /// System categories cannot be renamed; the core refuses it.
    func renameCategory(_ categoryId: Uuid, name: String) async {
        await categoryCommand(.renameCategory(categoryId: categoryId, name: name))
    }

    func archiveCategory(_ categoryId: Uuid) async {
        await categoryCommand(.archiveCategory(categoryId: categoryId))
    }

    func restoreCategory(_ categoryId: Uuid) async {
        await categoryCommand(.restoreCategory(categoryId: categoryId))
    }

    func addAlias(categoryId: Uuid, alias: String) async {
        await categoryCommand(.addAlias(categoryId: categoryId, alias: alias))
    }

    func removeAlias(categoryId: Uuid, alias: String) async {
        await categoryCommand(.removeAlias(categoryId: categoryId, alias: alias))
    }

    /// Refused when `previewCategoryMerge` reports conflicts; repoints every
    /// transaction of `sourceId` to `targetId` and deletes the source
    /// (`docs/v2/ARCH.md` §4).
    func mergeCategory(sourceId: Uuid, targetId: Uuid) async {
        await categoryCommand(.mergeCategory(sourceId: sourceId, targetId: targetId))
    }

    /// A category command, then the refresh of the picker list and the
    /// management table together. A full `reload()` because a rename
    /// propagates its denormalized name onto transactions and a merge
    /// repoints them (`docs/v2/DISTILLATO_V1.md` §2.1): the loaded
    /// transactions page can be stale, not just the category lists.
    private func categoryCommand(_ command: Command) async {
        guard let vault = currentVault, !refusedAsReadOnly() else { return }
        await guarded {
            try await core.execute(vaultId: vault.id, command)
            await reload()
            if let management = try? await core.categoryManagement(vaultId: vault.id) {
                windowCategories = management.categories
                categoryAliases = management.aliases
            }
        }
    }

    // MARK: - Recurring

    /// Every template, active and archived; called when the Recurring panel
    /// opens (`pendingRecurringItems` itself is kept current by `reload()`).
    func loadRecurringTemplates() async {
        guard let vault = currentVault else {
            recurringTemplates = []
            return
        }
        await guarded {
            let templates = try await core.listRecurring(vaultId: vault.id, includeArchived: true)
            // Another vault opened while the list was on its way: it belongs
            // to the vault that is gone, and would sit under the new one's
            // name until the next load.
            guard currentVault?.id == vault.id else { return }
            recurringTemplates = templates
        }
    }

    /// Returns the new template's id, from the core's receipt, or `nil` when
    /// it was refused: the inspector selects what it created by that id, not
    /// by what the list gained, which a sync landing at the same time could
    /// also have added to.
    @discardableResult
    func createRecurring(
        kind: TransactionKind,
        amount: Int64,
        walletId: Uuid?,
        flowId: Uuid?,
        category: String?,
        note: String?,
        schedule: Schedule,
        owner: String? = nil
    ) async -> Uuid? {
        guard let vault = currentVault, !refusedAsReadOnly() else { return nil }
        var created: Uuid?
        // `recurringCommand`'s steps, keeping the receipt it drops.
        await guarded {
            let receipt = try await core.execute(
                vaultId: vault.id,
                .createRecurring(
                    transactionKind: kind,
                    amount: amount,
                    walletId: walletId,
                    flowId: flowId,
                    category: category,
                    note: note,
                    schedule: schedule,
                    // The author is the core's default owner: said by
                    // leaving it out, like a row's person.
                    owner: explicitPerson(owner)
                )
            )
            created = receipt.resultId
            await loadRecurringTemplates()
            await reload()
        }
        return created
    }

    func updateRecurring(_ recurringId: Uuid, patch: RecurringPatch) async {
        guard !patch.isEmpty else { return }
        await recurringCommand(.updateRecurring(recurringId: recurringId, patch: patch))
    }

    func archiveRecurring(_ recurringId: Uuid) async {
        await recurringCommand(.archiveRecurring(recurringId: recurringId))
    }

    /// Puts an archived template back on the schedule; the periods it missed
    /// while archived come back as due.
    func restoreRecurring(_ recurringId: Uuid) async {
        await recurringCommand(.restoreRecurring(recurringId: recurringId))
    }

    /// Every period waiting for a decision, oldest first: what the due sheet
    /// lists and what Execute All runs, in that order.
    var duePeriods: [DuePeriod] {
        pendingRecurringItems
            .flatMap { item in item.due.map { DuePeriod(template: item.template, date: $0) } }
            .sorted { $0.date < $1.date }
    }

    /// Materializes `periodDate` as a transaction; `occurredAt` is the due
    /// date at the current time of day, in the system offset (team-lead
    /// task 4). The row is for the template's owner, whoever presses
    /// Registra: the mortgage stays Matteo's when Elisa records it.
    func executeRecurring(_ recurringId: Uuid, periodDate: NaiveDate) async {
        await recurringCommand(
            execution(recurringId, owner: owner(ofTemplate: recurringId), periodDate: periodDate, now: Date())
        )
    }

    func skipRecurring(_ recurringId: Uuid, periodDate: NaiveDate) async {
        await recurringCommand(.skipRecurring(recurringId: recurringId, periodDate: periodDate))
    }

    /// Executes every period on the due list as one batch: all of them are
    /// written or, when one is refused (an envelope that would go below zero,
    /// a wallet archived since), none is, and the alert says which. Half a
    /// backlog applied would leave the user working out what is still due.
    func executeAllDueRecurring() async {
        guard let vault = currentVault, !refusedAsReadOnly() else { return }
        let now = Date()
        let commands = duePeriods.map {
            execution($0.template.id, owner: $0.template.owner, periodDate: $0.date, now: now)
        }
        guard !commands.isEmpty else { return }
        await guarded {
            try await core.executeBatch(vaultId: vault.id, commands)
            savedAt = Date()
            await reload()
        }
    }

    /// The owner of a template, from the due list `reload()` keeps fresh, or
    /// from the templates the Ricorrenze tab loaded.
    func owner(ofTemplate id: Uuid) -> String? {
        pendingRecurringItems.first { $0.template.id == id }?.template.owner
            ?? recurringTemplates.first { $0.id == id }?.owner
    }

    /// The command that records one period, for `owner`. The owner goes in
    /// the command rather than being read when it is applied: a later change
    /// of owner must not rewrite on replay what was recorded before it. The
    /// author is left out, as on any row of theirs.
    private func execution(_ recurringId: Uuid, owner: String?, periodDate: NaiveDate, now: Date) -> Command {
        .executeRecurring(
            recurringId: recurringId,
            periodDate: periodDate,
            occurredAt: Self.combine(day: periodDate, timeOf: now),
            person: explicitPerson(owner)
        )
    }

    /// A recurring command, then the template list and the ledger together:
    /// executing a period writes a transaction as well as a run.
    private func recurringCommand(_ command: Command) async {
        guard let vault = currentVault, !refusedAsReadOnly() else { return }
        await guarded {
            try await core.execute(vaultId: vault.id, command)
            await loadRecurringTemplates()
            await reload()
        }
    }

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

    /// `periodDate` (a bare day) at today's time of day, in the system
    /// offset: what `executeRecurring` sends as `occurredAt`.
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

    /// The name of the wallet a new row would land on, for the WALLET cell of
    /// the empty line. `defaultWalletId` is `nil` in a one-wallet vault (the
    /// core resolves it), which still has a name to show.
    var defaultWalletName: String? {
        if let lastWalletId, let wallet = wallets.first(where: { $0.id == lastWalletId }) { return wallet.name }
        return wallets.first?.name
    }

    /// The envelope a new row lands on: the last one written to, else the
    /// first that is not Unallocated, else Unallocated.
    var defaultFlowId: Uuid? {
        if let lastFlowId, flows.contains(where: { $0.id == lastFlowId }) { return lastFlowId }
        return flows.first { !$0.isUnallocated }?.id ?? flows.first?.id
    }

    /// The last row of the loaded month that ⌘D can copy into the new line.
    /// Transfers are passed over: the empty line has one wallet and one
    /// envelope, and a transfer's two ends fit neither.
    var lastRow: TransactionRow? { rows.last { !$0.isTransfer } }

    /// Appends the row typed in the grid's empty last line. `day` carries the
    /// calendar day the DATA cell shows; the time of day comes from the clock,
    /// so rows entered on the same day keep their typing order.
    ///
    /// A `nil` envelope means "the sticky default", not Unallocated: an empty
    /// FLOW cell has to behave like the last row, not like a system envelope
    /// the user never picked. A `nil` kind is the direction on screen's; a
    /// duplicate passes its source's, so a refund copied stays a refund.
    ///
    /// `person` is who the row is for, already one of `assignablePeople`.
    /// The author is sent as no person at all: the core reads that as the
    /// author, and the command reads exactly as one from before rows could
    /// be for someone else.
    func addRow(
        day: Date,
        flowId: Uuid?,
        category: String?,
        note: String,
        amount: Int64,
        walletId: Uuid? = nil,
        kind: TransactionKind? = nil,
        person: String? = nil
    ) async {
        guard let vault = currentVault, amount > 0, !refusedAsReadOnly() else { return }
        let trimmedNote = note.trimmingCharacters(in: .whitespacesAndNewlines)
        let envelope = flowId ?? defaultFlowId
        let entry = Entry(
            amount: amount,
            walletId: walletId ?? defaultWalletId,
            flowId: envelope,
            category: category?.trimmingCharacters(in: .whitespacesAndNewlines),
            note: trimmedNote.isEmpty ? nil : trimmedNote,
            occurredAt: Self.combine(day: CoreDate.day(day), timeOf: Date()),
            person: explicitPerson(person)
        )
        let command: Command
        switch kind ?? direction.newRowKind {
        case .expense: command = .expense(entry)
        case .income: command = .income(entry)
        case .refund: command = .refund(entry)
        // Two ends, not a wallet and an envelope: the grid never writes one.
        case .transferWallet, .transferFlow: return
        }
        await guarded {
            // Minted first, so undo knows which row to void.
            let minted = await core.envelope(vaultId: vault.id, command)
            let receipt = try await core.execute(envelope: minted)
            recordAddedRow(command, id: receipt.resultId ?? minted.id, vaultId: vault.id)
            lastWalletId = entry.walletId ?? lastWalletId
            lastFlowId = envelope ?? lastFlowId
            savedAt = Date()
            await reload()
        }
    }

    /// What a command says for `person`: nothing for the author, or a blank,
    /// the name otherwise. Shared by the rows the grid adds and the periods
    /// a template records for its owner.
    func explicitPerson(_ person: String?) -> String? {
        guard let trimmed = person?.trimmingCharacters(in: .whitespacesAndNewlines),
              !trimmed.isEmpty, trimmed != currentAuthor
        else { return nil }
        return trimmed
    }

    /// Resolves what was typed in the FLOW cell to an envelope, with the same
    /// precedence the core's quick-add resolver uses: exact, then unique
    /// prefix, then unique substring (`core/src/quick_add.rs`). Returns `nil`
    /// for blank text, meaning "leave the default".
    ///
    /// Synchronous on purpose: the names come from the loaded snapshot, not
    /// from the core, so a cell can be validated while it is being typed.
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

    /// The WALLET cell's resolver, the same precedence `resolveFlow` uses.
    /// Returns `nil` for blank text, meaning "leave the default".
    func resolveWallet(named text: String) throws -> Uuid? {
        let needle = text.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !needle.isEmpty else { return nil }
        let candidates = wallets.map { (id: $0.id, name: $0.name.lowercased()) }
        if let exact = candidates.first(where: { $0.name == needle }) { return exact.id }
        for matches in [
            candidates.filter { $0.name.hasPrefix(needle) },
            candidates.filter { $0.name.contains(needle) },
        ] {
            if matches.count == 1 { return matches[0].id }
            if matches.count > 1 {
                throw DomainError.InvalidCommand(
                    message: String(localized: "More than one wallet matches \u{201C}\(text)\u{201D}")
                )
            }
        }
        throw DomainError.NotFound(message: String(localized: "No wallet named \u{201C}\(text)\u{201D}"))
    }

    /// Surfaces a validation failure from the grid through the same alert the
    /// core's errors use, so a bad cell reads like a refused command.
    func report(_ error: Error) {
        present(error)
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
    func submit(quickAdd input: String) async {
        guard let vault = currentVault else { return }
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        // The line stays as typed, and nothing is resolved against the core:
        // a viewer is told, not left wondering why the row never came.
        guard !trimmed.isEmpty, !refusedAsReadOnly() else { return }
        await guarded {
            let parsed = try parseQuickAdd(input: trimmed, currency: currency)
            let resolved: ResolvedQuickAdd
            do {
                // A `!name` picks among the people a row may be for.
                resolved = try await core.resolveQuickAdd(
                    vaultId: vault.id,
                    parsed: parsed,
                    now: Date(),
                    defaults: QuickAddDefaults(walletId: lastWalletId, flowId: lastFlowId),
                    people: assignablePeople
                )
            } catch let error as QuickAddError {
                // The line knows which marker a name came from.
                presentedError = AppError(error, line: trimmed)
                return
            }
            // A row like one typed in the grid, and undone the same way.
            let minted = await core.envelope(vaultId: vault.id, resolved.command)
            let receipt = try await core.execute(envelope: minted)
            recordAddedRow(resolved.command, id: receipt.resultId ?? minted.id, vaultId: vault.id)
            savedAt = Date()
            // The core reports what the names resolved to, so the sticky
            // defaults never depend on the shape of the command.
            lastWalletId = resolved.walletId ?? lastWalletId
            lastFlowId = resolved.flowId ?? lastFlowId
            quickAddText = ""
            await reload()
        }
    }

    /// Called from the error alert's candidate buttons after an
    /// `ambiguous_name` quick-add error: rewrites the marker that carried
    /// the ambiguous fragment with the chosen name and resubmits (task 1).
    func resolveAmbiguous(choosing candidate: String) async {
        guard let error = presentedError, let fragment = error.ambiguousFragment else { return }
        presentedError = nil
        let rewritten = Self.rewrite(quickAddText, fragment: fragment, with: candidate)
        quickAddText = rewritten
        await submit(quickAdd: rewritten)
    }

    /// Swaps `chosen` in for `fragment` behind the marker that carried it,
    /// preserving the marker.
    nonisolated static func rewrite(_ text: String, fragment: String, with chosen: String) -> String {
        guard let found = marker(carrying: fragment, in: text) else { return text }
        return text.replacingCharacters(in: found.range, with: found.marker + chosen)
    }

    /// Which marker (`#`, `@`, `>`, `!`) carried `fragment` in `text`, and
    /// where: the grammar allows at most one of each
    /// (`docs/v2/DISTILLATO_V1.md` §3.1), so one match per marker is all
    /// there is. A whole token wins over the start of a longer one: in
    /// `#elisir !eli` the person is `!eli`, not the `#eli` of the category.
    nonisolated static func marker(carrying fragment: String, in text: String) -> (marker: String, range: Range<String.Index>)? {
        let markers = ["#", "@", ">", "!"]
        var partial: (marker: String, range: Range<String.Index>)?
        for marker in markers {
            var start = text.startIndex
            while let range = text.range(of: marker + fragment, options: .caseInsensitive, range: start..<text.endIndex) {
                let opens = range.lowerBound == text.startIndex || text[text.index(before: range.lowerBound)].isWhitespace
                let closes = range.upperBound == text.endIndex || text[range.upperBound].isWhitespace
                if opens, closes { return (marker, range) }
                if opens, partial == nil { partial = (marker, range) }
                start = range.upperBound
            }
        }
        return partial
    }

    // MARK: - Void with deferred undo

    /// Hides the row and starts the undo window. The command is only sent
    /// when the window elapses or another destructive action starts.
    func void(transactionId: Uuid) async {
        await void(transactionIds: [transactionId])
    }

    /// Hides the rows and starts one undo window for all of them; the voids
    /// go out together, as one batch, when it elapses.
    func void(transactionIds ids: [Uuid]) async {
        guard let vault = currentVault, !isReadOnly, !ids.isEmpty else { return }
        await flushPendingUndo()
        pendingUndo = PendingUndo(ids: ids, vaultId: vault.id, startedAt: Date(), duration: undoWindow)
        // ⌘Z while the toast is up is the toast's own Undo.
        history.recordPendingVoid(name: String(localized: "Delete Transactions")) { [weak self] in self?.undo() }
        let window = undoWindow
        let sleep = sleeper
        undoTask = Task { [weak self] in
            try? await sleep(window)
            guard !Task.isCancelled else { return }
            await self?.flushPendingUndo()
        }
    }

    /// Cancels the pending void; nothing was ever executed. Edit ▸ Undo has
    /// nothing left to cancel either, however the toast was dismissed.
    func undo() {
        undoTask?.cancel()
        undoTask = nil
        pendingUndo = nil
        history.forgetPendingVoid()
    }

    /// Applies a pending void now, in the vault it was made in: the one on
    /// screen may have changed since. Also what quitting awaits
    /// (`AppDelegate`), so the toast never outlives the app with its rows
    /// still live.
    ///
    /// A vault that no longer exists takes its rows with it, so the void is
    /// dropped without a word rather than refused with a "Not found".
    ///
    /// A void written is never undoable: there is no un-void. ⌘Z loses it, and
    /// every earlier step about its rows, which could only be refused now.
    func flushPendingUndo() async {
        guard let pending = pendingUndo else { return }
        pendingUndo = nil
        undoTask?.cancel()
        undoTask = nil
        history.forgetPendingVoid()
        await guarded {
            guard try await core.vaults().contains(where: { $0.id == pending.vaultId }) else { return }
            try await core.executeBatch(
                vaultId: pending.vaultId,
                pending.ids.map { .voidTransaction(transactionId: $0) }
            )
            history.forget(rows: Set(pending.ids))
            savedAt = Date()
            // Another vault's void changes nothing on screen: after a switch
            // `select` reloads the vault it moved to by itself.
            if pending.vaultId == currentVault?.id { await reload() }
        }
    }

    // MARK: - Editing

    /// Sends only the fields that changed, and makes the edit undoable.
    func update(transactionId: Uuid, patch: TransactionPatch) async {
        guard let vault = currentVault, !patch.isEmpty, !refusedAsReadOnly() else { return }
        // Read before the write: undo puts back what is stored now.
        let inverse = storedInverse(of: patch, for: transactionId)
        await guarded {
            try await core.execute(
                vaultId: vault.id,
                .updateTransaction(transactionId: transactionId, patch: patch)
            )
            if let inverse {
                recordPatches(
                    [RowPatch(id: transactionId, patch: patch)],
                    inverse: [RowPatch(id: transactionId, patch: inverse)],
                    vaultId: vault.id,
                    name: String(localized: "Edit Row")
                )
            }
            savedAt = Date()
            await reload()
        }
    }

    // MARK: - Writes without a step
    //
    // What the bulk actions write, and what undo and redo replay: the callers
    // record their own step, or are one.

    /// Applies `commands` to `vaultId` as one batch, all of them or none, then
    /// reloads. Says whether it went through; a refusal is already on the
    /// alert.
    @discardableResult
    func apply(_ commands: [Command], in vaultId: Uuid) async -> Bool {
        guard !commands.isEmpty, !refusedAsReadOnly() else { return false }
        var applied = false
        await guarded {
            try await core.executeBatch(vaultId: vaultId, commands)
            applied = true
            savedAt = Date()
            if vaultId == currentVault?.id { await reload() }
        }
        return applied
    }

    /// Applies `command` under an id minted for it and returns the id of what
    /// it created: redo adds a row back as a new one, and has to know which.
    func applyMinted(_ command: Command, in vaultId: Uuid) async -> Uuid? {
        guard !refusedAsReadOnly() else { return nil }
        var created: Uuid?
        await guarded {
            let minted = await core.envelope(vaultId: vaultId, command)
            let receipt = try await core.execute(envelope: minted)
            created = receipt.resultId ?? minted.id
            savedAt = Date()
            if vaultId == currentVault?.id { await reload() }
        }
        return created
    }

    // MARK: - Errors

    /// Says no to a write on a read-only vault before anything reaches the
    /// core, through the same alert a refused command gets. The views already
    /// hide what would write; this catches what they cannot, like a quick-add
    /// line typed and sent.
    private func refusedAsReadOnly() -> Bool {
        guard isReadOnly else { return false }
        presentedError = AppError(
            code: "forbidden",
            message: String(localized: "This vault is shared with you to read, not to change.")
        )
        return true
    }

    /// Runs a piece of work, turning any core error into `presentedError`.
    private func guarded(_ work: () async throws -> Void) async {
        do {
            try await work()
        } catch {
            present(error)
        }
    }

    private func present(_ error: Error) {
        switch error {
        case let error as DomainError: presentedError = AppError(error)
        case let error as QuickAddError: presentedError = AppError(error)
        default: presentedError = AppError(code: "unexpected", message: error.localizedDescription)
        }
    }
}
