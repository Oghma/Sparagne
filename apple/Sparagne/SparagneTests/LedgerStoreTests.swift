import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The ledger's own state on `AppStore`: the month window, the direction and
/// person filters, the aggregates behind the summary panel, and the writes the
/// grid makes (`docs/v2/UI.md`).
@MainActor
struct LedgerStoreTests {
    private static func makeStore(author: String = "matteo") throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.ledger.\(UUID().uuidString)"))
        return AppStore(client: try CoreClient.inMemory(author: author), defaults: defaults)
    }

    /// A vault with one wallet, one envelope beside Unallocated, and rows in
    /// the current month written by two people.
    private static func household() throws -> (store: AppStore, envelope: FlowView) {
        let store = try makeStore()
        store.bootstrap()
        store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        store.createEnvelope(name: "Cash", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        let envelope = try #require(store.flows.first { $0.name == "Cash" })

        // matteo earns and spends, elisa only spends.
        store.direction = .income
        store.addRow(day: Date(), flowId: envelope.id, category: "Stipendio", note: "stipendio", amount: 300_000)
        store.direction = .expenses
        store.addRow(day: Date(), flowId: envelope.id, category: "Casa", note: "mutuo", amount: 95_000)
        store.addRow(day: Date(), flowId: envelope.id, category: "Spesa", note: "coop", amount: 5_000)
        #expect(store.presentedError == nil)
        return (store, envelope)
    }

    // MARK: - The month window

    @Test("The ledger shows one month, and stepping to an empty one empties the rows")
    func monthWindow() throws {
        let (store, _) = try Self.household()
        #expect(store.rows.count == 2)

        store.month = store.month.adding(months: -1)
        #expect(store.rows.isEmpty)
        #expect(store.summary?.totals.income == 0)

        store.month = store.month.adding(months: 1)
        #expect(store.rows.count == 2)
    }

    @Test("Rows come back oldest first, the reading order of a ledger")
    func rowsAreAscending() throws {
        let store = try Self.makeStore()
        store.bootstrap()
        store.createVault(name: "Casa", walletName: "Conto", openingBalance: 0)
        let calendar = Calendar.current
        let month = store.month
        for day in [12, 3, 27] {
            let date = try #require(calendar.date(from: DateComponents(year: month.year, month: month.month, day: day)))
            store.addRow(day: date, flowId: nil, category: "Spesa", note: "day \(day)", amount: 1_000)
        }
        #expect(store.presentedError == nil)

        let days = store.rows.map { calendar.component(.day, from: $0.occurredAt) }
        #expect(days == [3, 12, 27])
    }

    // MARK: - Filters

    @Test("The direction switch shows expenses or income, never both")
    func directionFilter() throws {
        let (store, _) = try Self.household()
        #expect(store.rows.allSatisfy { $0.kind == .expense })

        store.direction = .income
        // The opening balance is income too, so both rows show.
        #expect(store.rows.count == 2)
        #expect(store.rows.allSatisfy { $0.kind == .income })
    }

    @Test("The person filter narrows the rows and the aggregates, but not the person columns")
    func personFilter() throws {
        let (store, envelope) = try Self.household()
        // A second author writes a row of her own.
        store.client.author = "elisa"
        store.addRow(day: Date(), flowId: envelope.id, category: "Spesa", note: "verdura", amount: 2_000)
        store.client.author = "matteo"
        store.reload()

        #expect(store.authors == ["elisa", "matteo"])
        #expect(store.rows.count == 3)

        store.person = "elisa"
        #expect(store.rows.count == 1)
        #expect(store.rows.first?.note == "verdura")

        let summary = try #require(store.summary)
        // The month totals follow the filter...
        #expect(summary.totals.netExpense == 2_000)
        // ...but the matrix keeps both columns, or the panel would blank out.
        #expect(summary.people.sorted() == ["elisa", "matteo"])
        #expect(summary.totals(for: "elisa").netExpense == 2_000)
        #expect(summary.totals(for: "matteo").netExpense == 100_000)
    }

    @Test("A person who no longer appears in the vault is dropped from the filter")
    func stalePersonIsCleared() throws {
        let (store, _) = try Self.household()
        store.person = "nobody"
        // `nobody` never wrote anything, so reload must not leave the ledger
        // stuck on an empty month with no way back.
        #expect(store.person == nil)
    }

    // MARK: - Aggregates

    @Test("The summary describes the month the rows are in")
    func summaryMatchesTheMonth() throws {
        let (store, envelope) = try Self.household()
        let summary = try #require(store.summary)

        // 100.00 opening + 3000.00 salary in, 950.00 + 50.00 out.
        #expect(summary.totals.income == 400_000)
        #expect(summary.totals.netExpense == 100_000)
        #expect(summary.savings == 300_000)
        #expect(summary.netExpense(flow: envelope.id) == 100_000)

        // Heaviest category first.
        #expect(summary.categories.first?.name == "Casa")
        #expect(summary.categories.first?.netExpense == 95_000)

        // Twelve buckets ending with this month, and a calendar year of them.
        #expect(summary.trailing.count == 12)
        #expect(summary.trailingMonths.last == store.month)
        #expect(summary.trailing.last?.income == 400_000)
        #expect(summary.year.count == 12)
        #expect(summary.year[store.month.month - 1].income == 400_000)

        #expect(summary.top.first?.note == "mutuo")
        #expect(summary.spendByPerson.first?.person == "matteo")
    }

    @Test("A refund nets off the expense it reverses")
    func refundsNetOff() throws {
        let (store, envelope) = try Self.household()
        let before = try #require(store.summary).totals.netExpense

        store.submit(quickAdd: "r 20.00 rimborso #Spesa >Cash")
        #expect(store.presentedError == nil)

        let after = try #require(store.summary)
        #expect(after.totals.netExpense == before - 2_000)
        #expect(after.netExpense(flow: envelope.id) == before - 2_000)
    }

    // MARK: - Writes from the grid

    @Test("A new row lands on the sticky envelope and moves the balances")
    func newRowUsesTheStickyEnvelope() throws {
        let (store, envelope) = try Self.household()
        #expect(store.defaultFlowId == envelope.id)

        store.addRow(day: Date(), flowId: nil, category: "Auto", note: "benzina", amount: 6_000)
        #expect(store.presentedError == nil)

        let row = try #require(store.rows.first { $0.note == "benzina" })
        #expect(row.flowId == store.defaultFlowId)
        #expect(row.person == "matteo")
        #expect(store.savedAt != nil)
    }

    @Test("A row typed with no amount is not a row")
    func zeroAmountIsIgnored() throws {
        let (store, _) = try Self.household()
        let before = store.rows.count
        store.addRow(day: Date(), flowId: nil, category: "Auto", note: "niente", amount: 0)
        #expect(store.rows.count == before)
        #expect(store.presentedError == nil)
    }

    // MARK: - Name resolution in the FLOW cell

    @Test("The envelope cell resolves exactly, then by prefix, and complains when it cannot")
    func flowCellResolution() throws {
        let (store, envelope) = try Self.household()

        #expect(try store.resolveFlow(named: "Cash") == envelope.id)
        #expect(try store.resolveFlow(named: "ca") == envelope.id)
        #expect(try store.resolveFlow(named: "  ") == nil)

        #expect(throws: DomainError.self) { try store.resolveFlow(named: "vacanze") }

        // Two envelopes sharing a prefix are ambiguous, not a coin toss.
        store.createEnvelope(name: "Casa", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        #expect(throws: DomainError.self) { try store.resolveFlow(named: "cas") }
    }

    // MARK: - The draft behind a row being edited

    @Test("A committed row sends the cells that changed and nothing else")
    func draftPatchesOnlyWhatChanged() throws {
        let (store, _) = try Self.household()
        let row = try #require(store.rows.first { $0.note == "mutuo" })

        var draft = RowDraft(row: row, store: store)
        // Untouched: the same values the cells were filled with.
        let untouched = try draft.patch(against: row, store: store)
        #expect(untouched.isEmpty)

        draft.note = "mutuo agosto"
        // An emptied amount cell is not a way to say zero, so it is left alone.
        draft.amount = "  "
        let patch = try draft.patch(against: row, store: store)

        #expect(patch.note == "mutuo agosto")
        #expect(patch.amount == nil)
        #expect(patch.category == nil)
        #expect(patch.flowId == nil)
        #expect(patch.occurredAt == nil)
    }

    @Test("Moving a row to another day keeps its time of day")
    func draftKeepsTheTimeOfDay() throws {
        let (store, _) = try Self.household()
        let row = try #require(store.rows.first { $0.note == "coop" })
        let calendar = Calendar.current
        let moved = try #require(calendar.date(byAdding: .day, value: 1, to: row.occurredAt))

        var draft = RowDraft(row: row, store: store)
        // The DATA cell hands over a bare calendar day, at midnight.
        draft.day = try #require(calendar.date(from: calendar.dateComponents([.year, .month, .day], from: moved)))

        let patch = try draft.patch(against: row, store: store)
        // Rows entered on the same day keep their typing order, so a row must
        // not fall back to midnight when it changes day.
        #expect(patch.occurredAt == AppStore.stamp(day: draft.day, likeTimeOf: row.occurredAt))
        #expect(patch.amount == nil)
    }

    @Test("The empty line starts on the month it is shown in, with the sticky envelope")
    func blankDraftFollowsTheMonth() throws {
        let (store, envelope) = try Self.household()
        let calendar = Calendar.current

        // The current month: the line is ready for a row entered today.
        let today = RowDraft.blank(in: store)
        #expect(calendar.isDateInToday(today.day))
        #expect(today.flow == envelope.name)

        // Any other month has no "today", so the line starts on its first day.
        store.month = store.month.adding(months: -3)
        let past = RowDraft.blank(in: store)
        #expect(past.day == store.month.start())
        #expect(calendar.component(.day, from: past.day) == 1)
        #expect(past.flow == envelope.name)
    }
}
