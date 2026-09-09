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

    init(code: String, message: String, candidates: [String] = []) {
        self.code = code
        self.message = message
        self.candidates = candidates
    }

    init(_ error: DomainError) {
        self.init(code: error.code, message: error.message)
    }

    init(_ error: QuickAddError) {
        self.init(code: error.code, message: error.message, candidates: error.candidates)
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

/// Everything the window shows and everything it can do.
///
/// The store holds no domain state of its own: `snapshot`, `categories` and
/// `transactions` are query results, refreshed by `reload()` after every
/// command (`docs/v2/ARCH.md` §2.2).
@Observable
@MainActor
final class AppStore {
    static let pageSize: UInt32 = 100
    static let lastVaultKey = "lastVaultId"

    // MARK: Dependencies

    @ObservationIgnored private let client: CoreClient
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
    private(set) var totals: PeriodTotals?
    private(set) var allRows: [TransactionRow] = []

    // MARK: Filters and view state

    var period: Period = .thisMonth { didSet { if period != oldValue { reload() } } }
    var showVoided = false { didSet { if showVoided != oldValue { reload() } } }
    var showTransfers = true { didSet { if showTransfers != oldValue { reload() } } }
    /// Debounced by the view; call `reload()` when it settles.
    var searchText = ""
    var quickAddText = ""
    var selection: Uuid?

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
    }

    // MARK: - Derived

    var currency: Currency { snapshot?.currency ?? .eur }
    var currencyCode: String { currency.code }

    /// Active wallets, for the pickers.
    var wallets: [WalletView] { snapshot?.wallets.filter { !$0.archived } ?? [] }

    /// Active envelopes, Unallocated first (the core already orders them).
    var flows: [FlowView] { snapshot?.flows.filter { !$0.archived } ?? [] }

    /// Loaded rows minus the one waiting on the undo toast.
    var rows: [TransactionRow] {
        guard let hidden = pendingUndo?.id else { return allRows }
        return allRows.filter { $0.id != hidden }
    }

    var selectedRow: TransactionRow? {
        guard let selection else { return nil }
        return rows.first { $0.id == selection }
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

    func select(_ vault: VaultView) {
        flushPendingUndo()
        currentVault = vault
        defaults.set(vault.id, forKey: Self.lastVaultKey)
        selection = nil
        lastWalletId = nil
        lastFlowId = nil
        reload()
    }

    /// Snapshot, categories, totals and the first page of transactions.
    func reload() {
        guard let vault = currentVault else {
            snapshot = nil
            categories = []
            transactions = []
            allRows = []
            nextCursor = nil
            totals = nil
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
            let bounds = period.bounds()
            totals = try client.totals(vaultId: vault.id, from: bounds.from, to: bounds.to)
            rebuildRows()
        }
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
        let bounds = period.bounds()
        let text = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        return TransactionFilter(
            from: bounds.from,
            to: bounds.to,
            kinds: nil,
            includeVoided: showVoided,
            includeTransfers: showTransfers,
            walletId: nil,
            flowId: nil,
            text: text.isEmpty ? nil : text
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
            // The core reports what the names resolved to, so the sticky
            // defaults never depend on the shape of the command.
            lastWalletId = resolved.walletId ?? lastWalletId
            lastFlowId = resolved.flowId ?? lastFlowId
            quickAddText = ""
            reload()
        }
    }

    // MARK: - Void with deferred undo

    /// Hides the row and starts the undo window. The command is only sent
    /// when the window elapses or another destructive action starts.
    func void(transactionId: Uuid) {
        flushPendingUndo()
        if selection == transactionId { selection = nil }
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
