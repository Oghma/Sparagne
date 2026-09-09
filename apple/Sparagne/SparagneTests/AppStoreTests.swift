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

        // The opening balance is a real transaction, not a special case.
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
        #expect(row.signedAmount == -1250)
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

    @Test("A wallet transfer shows both ends in the wallet column")
    func transferRowShowsBothEnds() throws {
        let store = try Self.onboarded()
        store.createWallet(name: "Bank", openingBalance: 0)

        store.submit(quickAdd: "tw> 20.00 @cash @bank")

        #expect(store.presentedError == nil)
        let row = try #require(store.rows.first { $0.kind == .transferWallet })
        #expect(row.walletDisplay == "Cash → Bank")
        #expect(row.envelopeDisplay == TransactionRow.placeholder)
        #expect(row.isTransfer)
        #expect(store.wallets.first { $0.name == "Bank" }?.balance == 2000)
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

/// The period filter's half-open bounds.
@MainActor
struct PeriodTests {
    private static let calendar: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Europe/Rome") ?? .gmt
        return calendar
    }()

    @Test func allIsUnfiltered() {
        let bounds = Period.all.bounds(now: Date(), calendar: Self.calendar)
        #expect(bounds.from == nil)
        #expect(bounds.to == nil)
    }

    @Test func thisMonthStartsOnTheFirst() throws {
        let now = try #require(Self.calendar.date(from: DateComponents(year: 2026, month: 2, day: 25, hour: 12)))
        let bounds = Period.thisMonth.bounds(now: now, calendar: Self.calendar)
        #expect(bounds.from == "2026-01-31T23:00:00Z")
        #expect(bounds.to == "2026-02-28T23:00:00Z")
    }

    @Test func totalsBoundsAreAlwaysConcrete() {
        let bounds = Period.all.totalsBounds(now: Date(), calendar: Self.calendar)
        #expect(bounds.from == "1970-01-01T00:00:00Z")
        #expect(bounds.from < bounds.to)
    }
}
