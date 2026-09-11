import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// `AppStore` driven end to end over an in-memory core: every assertion is
/// about what the projection says after a real command was applied.
@MainActor
struct AppStoreTests {
    /// A store on a fresh in-memory database, with its own defaults so the
    /// "last vault" key never leaks between tests.
    private static func makeStore(
        undoWindow: Duration = .seconds(60),
        sleeper: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) }
    ) throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.tests.\(UUID().uuidString)"))
        return AppStore(
            client: try CoreClient.inMemory(author: "tester"),
            defaults: defaults,
            undoWindow: undoWindow,
            sleeper: sleeper
        )
    }

    /// A store that has been through onboarding: vault `Main`, wallet `Cash`
    /// holding 100.00.
    private static func onboarded() throws -> AppStore {
        let store = try makeStore()
        store.bootstrap()
        store.createVault(name: "Main", walletName: "Cash", openingBalance: 10000)
        return store
    }

    private static func pizzaRow(_ store: AppStore) throws -> TransactionRow {
        try #require(store.rows.first { $0.note == "pizza" })
    }

    @Test("An empty database asks for onboarding, which creates the vault and its wallet")
    func onboarding() throws {
        let store = try Self.makeStore()

        store.bootstrap()
        #expect(store.needsOnboarding)
        #expect(store.currentVault == nil)

        store.createVault(name: "Main", walletName: "Cash", openingBalance: 10000)

        #expect(store.presentedError == nil)
        #expect(!store.needsOnboarding)
        #expect(store.currentVault?.name == "Main")
        #expect(store.vaults.count == 1)

        let wallet = try #require(store.wallets.first)
        #expect(wallet.name == "Cash")
        #expect(wallet.balance == 10000)

        // The opening balance is a real transaction, not a special case. The
        // ledger opens on ENTRATE or USCITE, never both, so ask for income.
        store.direction = .income
        #expect(store.rows.count == 1)
        #expect(store.rows.first?.kind == .income)
    }

    @Test("A quick-add line adds a row and moves the wallet balance")
    func quickAddLowersTheWalletBalance() throws {
        let store = try Self.onboarded()

        store.submit(quickAdd: "-12.50 pizza #food")

        #expect(store.presentedError == nil)
        #expect(store.quickAddText.isEmpty)

        let row = try Self.pizzaRow(store)
        #expect(row.kind == .expense)
        #expect(row.absoluteAmount == 1250)
        #expect(row.category == "food")
        #expect(row.walletDisplay == "Cash")

        #expect(store.wallets.first?.balance == 8750)
    }

    @Test("Void hides the row, and undo puts it back without executing anything")
    func undoCancelsThePendingVoid() throws {
        let store = try Self.onboarded()
        store.submit(quickAdd: "-12.50 pizza #food")
        let row = try Self.pizzaRow(store)

        store.void(transactionId: row.id)
        #expect(store.pendingUndo?.id == row.id)
        #expect(!store.rows.contains { $0.id == row.id })
        // Nothing was sent yet: the balance has not moved back.
        #expect(store.wallets.first?.balance == 8750)

        store.undo()
        #expect(store.pendingUndo == nil)
        #expect(store.rows.contains { $0.id == row.id })

        // And the core still has it as a live transaction.
        store.reload()
        let reloaded = try Self.pizzaRow(store)
        #expect(!reloaded.voided)
        #expect(store.wallets.first?.balance == 8750)
    }

    @Test("Letting the undo window elapse voids the transaction for real")
    func elapsedUndoWindowVoids() async throws {
        // The window is driven by the injected sleeper, so the test never waits.
        let store = try Self.makeStore(undoWindow: .zero, sleeper: { _ in })
        store.bootstrap()
        store.createVault(name: "Main", walletName: "Cash", openingBalance: 10000)
        store.submit(quickAdd: "-12.50 pizza #food")
        let row = try Self.pizzaRow(store)

        store.void(transactionId: row.id)
        let task = store.undoTask
        await task?.value

        #expect(store.presentedError == nil)
        #expect(store.pendingUndo == nil)
        // The void gave the money back to the wallet.
        #expect(store.wallets.first?.balance == 10000)
        #expect(!store.rows.contains { $0.id == row.id })

        store.showVoided = true
        let voided = try #require(store.allRows.first { $0.id == row.id })
        #expect(voided.voided)
    }

    @Test("Editing the amount re-applies the legs")
    func updatingTheAmountMovesTheBalance() throws {
        let store = try Self.onboarded()
        store.submit(quickAdd: "-12.50 pizza #food")
        let row = try Self.pizzaRow(store)

        var patch = TransactionPatch()
        patch.amount = 2000
        store.update(transactionId: row.id, patch: patch)

        #expect(store.presentedError == nil)
        #expect(store.wallets.first?.balance == 8000)
        #expect(try Self.pizzaRow(store).absoluteAmount == 2000)
    }

    @Test("An empty patch is not sent")
    func emptyPatchDoesNothing() throws {
        let store = try Self.onboarded()
        store.submit(quickAdd: "-12.50 pizza #food")
        let row = try Self.pizzaRow(store)

        store.update(transactionId: row.id, patch: TransactionPatch())

        #expect(store.presentedError == nil)
        #expect(store.wallets.first?.balance == 8750)
    }

    @Test("Spending more than an envelope holds surfaces insufficient_funds")
    func overspendingAnEnvelopeIsReported() throws {
        let store = try Self.onboarded()
        store.createEnvelope(name: "Vacanze", mode: .unlimited, allowNegative: false, openingAllocation: 0)
        #expect(store.presentedError == nil)

        store.submit(quickAdd: "-5.00 hotel >Vacanze")

        let error = try #require(store.presentedError)
        #expect(error.code == "insufficient_funds")
        #expect(error.message.contains("Vacanze"))
        // Nothing was written.
        #expect(store.wallets.first?.balance == 10000)
    }

    @Test("An unknown envelope name is a quick-add error, not a domain error")
    func unknownNameIsReported() throws {
        let store = try Self.onboarded()

        store.submit(quickAdd: "-5.00 hotel >nowhere")

        let error = try #require(store.presentedError)
        #expect(error.code == "unknown_name")
    }

    @Test("The preview parses without touching the database")
    func previewParses() throws {
        let store = try Self.onboarded()

        guard case .success(let parsed) = store.preview(quickAdd: "-12.50 pizza #food @cash") else {
            Issue.record("expected the line to parse")
            return
        }
        guard case .entry(let kind, let amount, let note, _, _, _, _) = parsed else {
            Issue.record("expected an entry")
            return
        }
        #expect(kind == .expense)
        #expect(amount == 1250)
        #expect(note == "pizza")

        let summary = QuickAddSummary.describe(parsed, currency: .eur)
        #expect(summary.contains("12.50 EUR"))
        #expect(summary.contains("#food"))
        #expect(summary.contains("@cash"))

        guard case .failure(let error) = store.preview(quickAdd: "pizza") else {
            Issue.record("expected a parse failure")
            return
        }
        #expect(error.code == "invalid_amount")
        #expect(store.presentedError == nil)
    }

    @Test("An ambiguous wallet name surfaces the candidates to choose from")
    func ambiguousNameOffersTheCandidates() throws {
        let store = try Self.onboarded()
        store.createWallet(name: "Bank", openingBalance: 0)
        store.createWallet(name: "Bancoposta", openingBalance: 0)
        #expect(store.presentedError == nil)

        store.submit(quickAdd: "-5.00 hotel @ban")

        let error = try #require(store.presentedError)
        #expect(error.code == "ambiguous_name")
        #expect(error.candidates.sorted() == ["Bancoposta", "Bank"])
        #expect(error.message.contains("Bancoposta"))
        // Nothing was written.
        #expect(!store.rows.contains { $0.note == "hotel" })
    }

    @Test("A row exposes the ids of its wallet and envelope")
    func rowExposesItsIds() throws {
        let store = try Self.onboarded()
        store.createEnvelope(name: "Spesa", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        let envelope = try #require(store.flows.first { $0.name == "Spesa" })

        store.submit(quickAdd: "-12.50 pizza #food >Spesa")

        #expect(store.presentedError == nil)
        let row = try Self.pizzaRow(store)
        #expect(row.walletId == store.wallets.first?.id)
        #expect(row.flowId == envelope.id)
        #expect(row.envelopeDisplay == "Spesa")

        // The opening balance carries the system category, localized here.
        store.direction = .income
        let opening = try #require(store.rows.first { $0.kind == .income })
        #expect(opening.category == String(localized: "Opening"))
    }

    @Test("A wallet transfer shows both ends in the wallet column")
    func transferRowShowsBothEnds() throws {
        let store = try Self.onboarded()
        store.createWallet(name: "Bank", openingBalance: 0)

        store.submit(quickAdd: "tw> 20.00 @cash @bank")

        #expect(store.presentedError == nil)
        // Transfers are in neither direction; the View menu opts into them.
        store.showTransfers = true
        let row = try #require(store.rows.first { $0.kind == .transferWallet })
        #expect(row.walletDisplay == "Cash → Bank")
        #expect(row.envelopeDisplay == TransactionRow.placeholder)
        #expect(row.isTransfer)
        #expect(row.walletId == store.wallets.first { $0.name == "Cash" }?.id)
        #expect(row.flowId == nil)
        #expect(store.wallets.first { $0.name == "Bank" }?.balance == 2000)
    }

    // MARK: - Wallet and envelope management

    @Test("Archiving a wallet with a balance surfaces invalid_command")
    func archivingAWalletWithBalanceIsRefused() throws {
        let store = try Self.onboarded()
        let cash = try #require(store.wallets.first { $0.name == "Cash" })

        store.archiveWallet(cash.id)

        let error = try #require(store.presentedError)
        #expect(error.code == "invalid_command")
        #expect(store.wallets.contains { $0.id == cash.id })
    }

    @Test("Updating an envelope's cap shows up in the snapshot")
    func updatingAnEnvelopeChangesTheCapInTheSnapshot() throws {
        let store = try Self.onboarded()
        store.createEnvelope(name: "Vacanze", mode: .unlimited, allowNegative: false, openingAllocation: 0)
        let envelope = try #require(store.flows.first { $0.name == "Vacanze" })

        store.updateEnvelope(envelope.id, mode: .netCapped(cap: 5000))

        #expect(store.presentedError == nil)
        let updated = try #require(store.flows.first { $0.id == envelope.id })
        #expect(updated.mode == .netCapped(cap: 5000))
    }

    // MARK: - Category management

    @Test("Renaming a category shows up in the categories list")
    func renamingACategoryShowsInCategories() throws {
        let store = try Self.onboarded()
        store.submit(quickAdd: "-5.00 pizza #Food")
        #expect(store.presentedError == nil)
        store.loadCategoryManagement()
        let food = try #require(store.windowCategories.first { $0.name == "Food" })

        store.renameCategory(food.id, name: "Groceries")

        #expect(store.presentedError == nil)
        #expect(store.categories.contains { $0.name == "Groceries" })
        #expect(store.windowCategories.contains { $0.id == food.id && $0.name == "Groceries" })
    }

    @Test("A merge with preview conflicts is refused; a clean merge repoints transactions")
    func categoryMergePreviewAndCleanMerge() throws {
        let store = try Self.onboarded()
        store.submit(quickAdd: "-5.00 pizza #Food")
        #expect(store.presentedError == nil)
        store.loadCategoryManagement()
        let food = try #require(store.windowCategories.first { $0.name == "Food" })

        // Merging a category into itself always conflicts.
        let selfPreview = store.previewCategoryMerge(sourceId: food.id, targetId: food.id)
        #expect(selfPreview?.ok == false)
        #expect(selfPreview?.conflicts.contains { $0.kind == .sameCategory } == true)

        store.mergeCategory(sourceId: food.id, targetId: food.id)
        #expect(store.presentedError != nil)
        store.presentedError = nil

        // A fresh, unrelated target has no conflicts and the merge repoints
        // the existing transaction (checked via `listTransactions`).
        store.createCategory(name: "Groceries")
        store.loadCategoryManagement()
        let groceries = try #require(store.windowCategories.first { $0.name == "Groceries" })

        let cleanPreview = store.previewCategoryMerge(sourceId: food.id, targetId: groceries.id)
        #expect(cleanPreview?.ok == true)
        #expect(cleanPreview?.conflicts.isEmpty == true)

        store.mergeCategory(sourceId: food.id, targetId: groceries.id)

        #expect(store.presentedError == nil)
        let pizza = try #require(store.transactions.first { $0.note == "pizza" })
        #expect(pizza.categoryId == groceries.id)
        #expect(!store.windowCategories.contains { $0.id == food.id })
    }

    // MARK: - Recurring

    @Test("A template due a month ago is pending; executing adds a transaction and clears it, skipping leaves no transaction")
    func recurringExecuteAndSkip() throws {
        let store = try Self.onboarded()
        let now = Date()
        let calendar = Calendar(identifier: .gregorian)
        let dayOfMonth = calendar.component(.day, from: now)
        let monthAgo = try #require(calendar.date(byAdding: .month, value: -1, to: now))
        // The executed transaction lands last month, and the ledger reads one
        // month at a time, so move the window there or `rows` will be empty.
        store.month = MonthKey(monthAgo)

        store.createRecurring(
            kind: .expense,
            amount: 999,
            walletId: nil,
            flowId: nil,
            category: "Bills",
            note: "Rent",
            schedule: Schedule(
                frequency: .monthly(day: UInt8(dayOfMonth)),
                interval: 1,
                startDate: CoreDate.day(monthAgo),
                endDate: nil
            )
        )
        #expect(store.presentedError == nil)
        let template = try #require(store.recurringTemplates.first { $0.note == "Rent" })

        let dueDate = try #require(
            store.pendingRecurringItems.first { $0.template.id == template.id }?.due.first
        )

        store.executeRecurring(template.id, periodDate: dueDate)

        #expect(store.presentedError == nil)
        #expect(!(store.pendingRecurringItems.first { $0.template.id == template.id }?.due.contains(dueDate) ?? false))
        let executed = try #require(store.rows.first { $0.note == "Rent" })
        #expect(executed.kind == .expense)
        #expect(executed.absoluteAmount == 999)

        // Skipping the following period (if any is already due) leaves no
        // additional transaction.
        if let nextDue = store.pendingRecurringItems.first(where: { $0.template.id == template.id })?.due.first {
            let countBefore = store.rows.filter { $0.note == "Rent" }.count
            store.skipRecurring(template.id, periodDate: nextDue)
            #expect(store.presentedError == nil)
            #expect(store.rows.filter { $0.note == "Rent" }.count == countBefore)
        }
    }
}

@MainActor
struct MonthKeyTests {
    private static let calendar: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Europe/Rome") ?? .gmt
        return calendar
    }()

    @Test func boundsAreTheMonthInLocalTime() {
        let bounds = MonthKey(year: 2026, month: 2).bounds(Self.calendar)
        // Rome is UTC+1 in February, so local midnight is 23:00 the day before.
        #expect(bounds.from == "2026-01-31T23:00:00Z")
        #expect(bounds.to == "2026-02-28T23:00:00Z")
    }

    @Test func steppingRollsOverTheYear() {
        #expect(MonthKey(year: 2026, month: 12).adding(months: 1, calendar: Self.calendar) == MonthKey(year: 2027, month: 1))
        #expect(MonthKey(year: 2026, month: 1).adding(months: -1, calendar: Self.calendar) == MonthKey(year: 2025, month: 12))
    }

    @Test func aYearIsTwelveBucketsAndThirteenBoundaries() {
        let bounds = MonthKey.yearBounds(2026, calendar: Self.calendar)
        #expect(bounds.count == 13)
        #expect(bounds.first == "2025-12-31T23:00:00Z")
        #expect(bounds.last == "2026-12-31T23:00:00Z")
    }

    @Test func theTrailingYearEndsWithTheMonthItself() {
        let month = MonthKey(year: 2026, month: 8)
        let trailing = month.trailingYear(calendar: Self.calendar)
        #expect(trailing.months.count == 12)
        #expect(trailing.bounds.count == 13)
        #expect(trailing.months.first == MonthKey(year: 2025, month: 9))
        #expect(trailing.months.last == month)
    }

    @Test func aBareDayStaysInsideTheMonthOnScreen() throws {
        let month = MonthKey(year: 2026, month: 8)
        let parsed = try #require(LedgerDate.parseDay("29", in: month, calendar: Self.calendar))
        let parts = Self.calendar.dateComponents([.year, .month, .day], from: parsed)
        #expect(parts.year == 2026)
        #expect(parts.month == 8)
        #expect(parts.day == 29)

        // A day the month does not have snaps back rather than rolling over.
        #expect(LedgerDate.parseDay("31", in: MonthKey(year: 2026, month: 2), calendar: Self.calendar) == nil)
        #expect(LedgerDate.parseDay("", in: month, calendar: Self.calendar) == nil)

        // `29/9` moves to another month of the same year.
        let other = try #require(LedgerDate.parseDay("29/9", in: month, calendar: Self.calendar))
        #expect(Self.calendar.component(.month, from: other) == 9)
    }
}

/// Every code `ErrorMessages.summary(for:)` is documented to cover
/// (`ErrorMessages.swift`): every `DomainError` and `QuickAddError` code,
/// plus the `"domain_error"` fallback key.
struct ErrorMessagesTests {
    private static let allCodes = [
        "insufficient_funds", "max_balance_reached", "not_found", "already_exists",
        "invalid_amount", "invalid_name", "invalid_flow", "currency_mismatch",
        "invalid_command", "invalid_cursor", "storage_error",
        "empty_input", "missing_amount", "duplicate_marker", "marker_not_allowed",
        "missing_transfer_target", "invalid_date", "duplicate_date", "ambiguous_name",
        "unknown_name", "same_target", "domain_error",
        // Sync: the server's codes and the transport's own two.
        "offline", "unauthorized", "forbidden", "author_mismatch",
        "registration_disabled", "invalid_request", "invalid_response",
        "invalid_server_url", "server_error",
    ]

    @Test("Every documented error code maps to a non-empty localized summary")
    func everyCodeHasASummary() {
        for code in Self.allCodes {
            let summary = ErrorMessages.summary(for: code)
            #expect(!summary.isEmpty, "no summary for \(code)")
        }
    }
}

/// Date conversions on the FFI boundary.
@MainActor
struct CoreDateTests {
    @Test func offsetStringRoundTrips() throws {
        let date = Date(timeIntervalSince1970: 1_772_000_000)
        let text = CoreDate.offset(date, timeZone: try #require(TimeZone(identifier: "Europe/Rome")))
        #expect(text == "2026-02-25T07:13:20+01:00")
        #expect(CoreDate.date(text) == date)
    }

    @Test func utcStringUsesZ() {
        let date = Date(timeIntervalSince1970: 1_772_000_000)
        #expect(CoreDate.utcString(date) == "2026-02-25T06:13:20Z")
    }

    @Test func dayIsTheLocalCalendarDay() throws {
        let date = Date(timeIntervalSince1970: 1_772_000_000)
        let rome = try #require(TimeZone(identifier: "Europe/Rome"))
        #expect(CoreDate.day(date, timeZone: rome) == "2026-02-25")
        #expect(CoreDate.localDay("2026-02-25", timeZone: rome) != nil)
    }
}
