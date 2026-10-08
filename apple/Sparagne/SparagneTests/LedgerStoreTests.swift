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
        return AppStore(core: try CoreActor.inMemory(author: author), defaults: defaults)
    }

    /// A vault with one wallet, one envelope beside Unallocated, and rows in
    /// the current month written by two people.
    private static func household() async throws -> (store: AppStore, envelope: FlowView) {
        let store = try makeStore()
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        await store.createEnvelope(name: "Cash", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        let envelope = try #require(store.flows.first { $0.name == "Cash" })

        // matteo earns and spends, elisa only spends.
        store.direction = .income
        await store.settle()
        await store.addRow(day: Date(), flowId: envelope.id, category: "Stipendio", note: "stipendio", amount: 300_000)
        store.direction = .expenses
        await store.settle()
        await store.addRow(day: Date(), flowId: envelope.id, category: "Casa", note: "mutuo", amount: 95_000)
        await store.addRow(day: Date(), flowId: envelope.id, category: "Spesa", note: "coop", amount: 5_000)
        #expect(store.presentedError == nil)
        return (store, envelope)
    }

    // MARK: - The month window

    @Test("The ledger shows one month, and stepping to an empty one empties the rows")
    func monthWindow() async throws {
        let (store, _) = try await Self.household()
        #expect(store.rows.count == 2)

        store.month = store.month.adding(months: -1)
        await store.settle()
        #expect(store.rows.isEmpty)
        #expect(store.summary?.totals.income == 0)

        store.month = store.month.adding(months: 1)
        await store.settle()
        #expect(store.rows.count == 2)
    }

    @Test("The menu and the palette step the month either way, and come back to the month of today")
    func monthSteps() async throws {
        let (store, _) = try await Self.household()
        let current = store.month

        store.stepMonth(by: -1)
        #expect(store.month == current.adding(months: -1))
        store.stepMonth(by: 13)
        #expect(store.month == current.adding(months: 12))

        let march = try #require(Calendar.current.date(from: DateComponents(year: 2025, month: 3, day: 15)))
        store.showCurrentMonth(today: march)
        #expect(store.month == MonthKey(year: 2025, month: 3))

        store.showCurrentMonth()
        await store.settle()
        #expect(store.month == current)
        #expect(store.rows.count == 2)
    }

    @Test("Nuova ricorrenza… opens the Ricorrenze tab with a request the tab takes")
    func newRecurringRequest() async throws {
        let (store, _) = try await Self.household()
        #expect(store.tab == .summary)
        #expect(!store.newRecurringRequested)

        store.requestNewRecurring()
        #expect(store.tab == .recurring)
        #expect(store.newRecurringRequested)
    }

    @Test("Rows come back oldest first, the reading order of a ledger")
    func rowsAreAscending() async throws {
        let store = try Self.makeStore()
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 0)
        let calendar = Calendar.current
        let month = store.month
        for day in [12, 3, 27] {
            let date = try #require(calendar.date(from: DateComponents(year: month.year, month: month.month, day: day)))
            await store.addRow(day: date, flowId: nil, category: "Spesa", note: "day \(day)", amount: 1_000)
        }
        #expect(store.presentedError == nil)

        let days = store.rows.map { calendar.component(.day, from: $0.occurredAt) }
        #expect(days == [3, 12, 27])
    }

    @Test("A month longer than a page is loaded page by page, and loadAll reads every one")
    func loadAllReadsEveryPage() async throws {
        let store = try Self.makeStore()
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        let vault = try #require(store.currentVault)
        // 1100 expenses of a cent each, written in one visit.
        let now = CoreDate.offset(Date())
        let entries: [Command] = (1...1100).map { index in
            .expense(Entry(amount: 1, walletId: nil, flowId: nil, category: "Spesa", note: "row \(index)", occurredAt: now))
        }
        try await store.core.executeBatch(vaultId: vault.id, entries)
        await store.reload()

        #expect(store.rows.count == Int(AppStore.pageSize))
        #expect(store.nextCursor != nil)

        await store.loadAll()

        #expect(store.presentedError == nil)
        #expect(store.nextCursor == nil)
        #expect(store.rows.count == 1100)
        #expect(Set(store.rows.map(\.id)).count == 1100)
    }

    // MARK: - Filters

    @Test("The direction switch shows expenses or income, never both")
    func directionFilter() async throws {
        let (store, _) = try await Self.household()
        #expect(store.rows.allSatisfy { $0.kind == .expense })

        store.direction = .income
        await store.settle()
        // The opening balance is income too, so both rows show.
        #expect(store.rows.count == 2)
        #expect(store.rows.allSatisfy { $0.kind == .income })
    }

    @Test("The person filter narrows the rows and the aggregates, but not the person columns")
    func personFilter() async throws {
        let (store, envelope) = try await Self.household()
        // A second author writes a row of her own.
        await store.setAuthor("elisa")
        await store.addRow(day: Date(), flowId: envelope.id, category: "Spesa", note: "verdura", amount: 2_000)
        await store.setAuthor("matteo")
        await store.reload()

        #expect(store.peopleInRows == ["elisa", "matteo"])
        #expect(store.rows.count == 3)

        store.person = "elisa"
        await store.settle()
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
    func stalePersonIsCleared() async throws {
        let (store, _) = try await Self.household()
        store.person = "nobody"
        await store.settle()
        // `nobody` never wrote anything, so reload must not leave the ledger
        // stuck on an empty month with no way back.
        #expect(store.person == nil)
    }

    // MARK: - Aggregates

    @Test("The summary describes the month the rows are in")
    func summaryMatchesTheMonth() async throws {
        let (store, envelope) = try await Self.household()
        let summary = try #require(store.summary)

        // 100.00 opening + 3000.00 salary in, 950.00 + 50.00 out.
        #expect(summary.totals.income == 400_000)
        #expect(summary.totals.netExpense == 100_000)
        #expect(summary.savings == 300_000)
        #expect(summary.netExpense(flow: envelope.id) == 100_000)

        // Heaviest category first.
        #expect(summary.categories.first?.name == "Casa")
        #expect(summary.categories.first?.netExpense == 95_000)

        // Twelve buckets ending with this month.
        #expect(summary.trailing.count == 12)
        #expect(summary.trailingMonths.last == store.month)
        #expect(summary.trailing.last?.income == 400_000)

        #expect(summary.spendByPerson.first?.person == "matteo")
    }

    @Test("A refund nets off the expense it reverses")
    func refundsNetOff() async throws {
        let (store, envelope) = try await Self.household()
        let before = try #require(store.summary).totals.netExpense

        await store.submit(quickAdd: "r 20.00 rimborso #Spesa >Cash")
        #expect(store.presentedError == nil)

        let after = try #require(store.summary)
        #expect(after.totals.netExpense == before - 2_000)
        #expect(after.netExpense(flow: envelope.id) == before - 2_000)
    }

    // MARK: - Writes from the grid

    @Test("A new row lands on the sticky envelope and moves the balances")
    func newRowUsesTheStickyEnvelope() async throws {
        let (store, envelope) = try await Self.household()
        #expect(store.defaultFlowId == envelope.id)

        await store.addRow(day: Date(), flowId: nil, category: "Auto", note: "benzina", amount: 6_000)
        #expect(store.presentedError == nil)

        let row = try #require(store.rows.first { $0.note == "benzina" })
        #expect(row.flowId == store.defaultFlowId)
        #expect(row.person == "matteo")
        #expect(store.savedAt != nil)
    }

    @Test("A duplicated refund is saved as a refund, not as an expense")
    func duplicateKeepsTheKind() async throws {
        let (store, _) = try await Self.household()
        await store.submit(quickAdd: "r 20.00 rimborso #Spesa >Cash")
        let refund = try #require(store.rows.first { $0.note == "rimborso" })
        #expect(refund.kind == .refund)

        let copy = try #require(RowDraft.duplicate(of: refund, store: store))
        #expect(copy.kind == .refund)
        let entry = try #require(try copy.entry(store: store))
        await store.addRow(
            day: entry.day,
            flowId: entry.flowId,
            category: entry.category,
            note: entry.note,
            amount: entry.amount,
            walletId: entry.walletId,
            kind: entry.kind
        )

        #expect(store.presentedError == nil)
        let copies = store.rows.filter { $0.note == "rimborso" }
        #expect(copies.count == 2)
        #expect(copies.allSatisfy { $0.kind == .refund })
    }

    @Test("A transfer offers no duplicate, and ⌘D passes over it")
    func transfersAreNotDuplicated() async throws {
        let (store, _) = try await Self.household()
        await store.createWallet(name: "Contanti", openingBalance: 0)
        await store.submit(quickAdd: "tw> 10.00 @conto @contanti")
        store.showTransfers = true
        await store.settle()
        #expect(store.presentedError == nil)

        // The transfer is the newest row of the month, so the last on screen.
        let transfer = try #require(store.rows.last)
        #expect(transfer.isTransfer)
        #expect(RowDraft.duplicate(of: transfer, store: store) == nil)
        #expect(store.lastRow?.note == "coop")
    }

    @Test("A row typed with no amount is not a row")
    func zeroAmountIsIgnored() async throws {
        let (store, _) = try await Self.household()
        let before = store.rows.count
        await store.addRow(day: Date(), flowId: nil, category: "Auto", note: "niente", amount: 0)
        #expect(store.rows.count == before)
        #expect(store.presentedError == nil)
    }

    // MARK: - Name resolution in the FLOW cell

    @Test("The envelope cell resolves exactly, then by prefix, and complains when it cannot")
    func flowCellResolution() async throws {
        let (store, envelope) = try await Self.household()

        #expect(try store.resolveFlow(named: "Cash") == envelope.id)
        #expect(try store.resolveFlow(named: "ca") == envelope.id)
        #expect(try store.resolveFlow(named: "  ") == nil)

        #expect(throws: DomainError.self) { try store.resolveFlow(named: "vacanze") }

        // Two envelopes sharing a prefix are ambiguous, not a coin toss.
        await store.createEnvelope(name: "Casa", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        #expect(throws: DomainError.self) { try store.resolveFlow(named: "cas") }
    }

    // MARK: - The draft behind a row being edited

    @Test("A committed row sends the cells that changed and nothing else")
    func draftPatchesOnlyWhatChanged() async throws {
        let (store, _) = try await Self.household()
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
    func draftKeepsTheTimeOfDay() async throws {
        let (store, _) = try await Self.household()
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
    func blankDraftFollowsTheMonth() async throws {
        let (store, envelope) = try await Self.household()
        let calendar = Calendar.current

        // The current month: the line is ready for a row entered today.
        let today = RowDraft.blank(in: store)
        #expect(calendar.isDateInToday(today.day))
        #expect(today.flow == envelope.name)

        // Any other month has no "today", so the line starts on its first day.
        store.month = store.month.adding(months: -3)
        await store.settle()
        let past = RowDraft.blank(in: store)
        #expect(past.day == store.month.start())
        #expect(calendar.component(.day, from: past.day) == 1)
        #expect(past.flow == envelope.name)
    }

    // MARK: - A category suggested by the note

    @Test("The empty line shows the category its note was filed under, and saves it while the cell stays empty")
    func noteSuggestsACategory() async throws {
        let (store, _) = try await Self.household()
        #expect(await store.suggestedCategory(forNote: "ferramenta") == nil)
        let suggested = try #require(await store.suggestedCategory(forNote: "coop"))
        #expect(suggested == "Spesa")

        var draft = RowDraft.blank(in: store)
        draft.note = "coop"
        draft.amount = "4"
        draft.suggestion = NoteSuggestion(note: "coop", category: suggested)
        #expect(draft.suggestedCategory == "Spesa")
        #expect(try draft.entry(store: store)?.category == "Spesa")

        // Typed over, the cell wins.
        draft.category = "Casa"
        #expect(try draft.entry(store: store)?.category == "Casa")

        // Another note: the suggestion was about the old one.
        draft.category = ""
        draft.note = "coop e forno"
        #expect(draft.suggestedCategory == nil)
        #expect(try draft.entry(store: store)?.category == nil)
    }

    @Test("The quick-add hint changes nothing until ⇥ writes it into the line")
    func quickAddHintIsNeverSilent() async throws {
        let (store, _) = try await Self.household()
        guard case .success(let parsed) = store.preview(quickAdd: "-4.00 coop") else {
            Issue.record("expected the line to parse")
            return
        }
        let note = try #require(QuickAddSummary.noteWithoutCategory(parsed))
        let hint = try #require(await store.suggestedCategory(forNote: note))
        #expect(hint == "Spesa")

        // Sent as typed: no category, whatever the hint said.
        await store.submit(quickAdd: "-4.00 coop")
        #expect(store.presentedError == nil)
        #expect(store.rows.first { $0.absoluteAmount == 400 }?.category == String(localized: "Uncategorized"))

        // Accepted: the line says it, and the row is filed there.
        let accepted = try #require(QuickAddSummary.accepting(hint, into: "-3.00 coop "))
        #expect(accepted == "-3.00 coop #Spesa")
        await store.submit(quickAdd: accepted)
        #expect(store.rows.first { $0.absoluteAmount == 300 }?.category == "Spesa")

        // A line that names its category asks for nothing, and a name `#`
        // cannot carry is not offered.
        guard case .success(let named) = store.preview(quickAdd: "-4.00 coop #Casa") else {
            Issue.record("expected the line to parse")
            return
        }
        #expect(QuickAddSummary.noteWithoutCategory(named) == nil)
        #expect(QuickAddSummary.accepting("Spesa casa", into: "-1.00 coop") == nil)
    }

    // MARK: - VoiceOver

    @Test("VoiceOver reads a row as one sentence: the amount with its kind, the category, the note, the envelope, who, and deleted")
    func rowReadsAsOneSentence() async throws {
        let (store, envelope) = try await Self.household()
        let row = try #require(store.rows.first { $0.note == "mutuo" })

        let label = LedgerAccessibility.label(for: row)
        #expect(label.hasPrefix(row.occurredAt.formatted(date: .long, time: .omitted)))
        #expect(label.contains(String(localized: "expense \(LedgerMoney.amount(95_000))")))
        #expect(label.contains("Casa, mutuo"))
        #expect(label.contains(String(localized: "envelope \(envelope.name)")))
        // matteo's own row: who it is for, and nobody else who recorded it.
        #expect(label.hasSuffix(LedgerAccessibility.person("matteo")))

        try await store.core.execute(vaultId: try #require(store.currentVault).id, .voidTransaction(transactionId: row.id))
        store.showVoided = true
        await store.settle()
        let voided = try #require(store.rows.first { $0.id == row.id })
        #expect(LedgerAccessibility.label(for: voided).hasSuffix(String(localized: "deleted")))
    }

    // MARK: - The optional WALLET column (`docs/v2/UI.md` §3)

    @Test("The wallet column is off until it is asked for, and the choice is remembered")
    func walletColumnIsRememberedOff() throws {
        let suite = "sparagne.wallet.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        let store = AppStore(core: try CoreActor.inMemory(author: "matteo"), defaults: defaults)
        #expect(store.showWalletColumn == false)

        store.showWalletColumn = true

        // A later launch reads the preference back, the same key the View
        // menu's toggle writes.
        let relaunched = AppStore(core: try CoreActor.inMemory(author: "matteo"), defaults: defaults)
        #expect(relaunched.showWalletColumn)
        #expect(defaults.bool(forKey: AppStore.walletColumnKey))
    }

    @Test("A row carries the name of its wallet, and the empty line offers the sticky default")
    func rowShowsItsWallet() async throws {
        let (store, envelope) = try await Self.household()
        await store.createWallet(name: "Contanti", openingBalance: 0)
        let cash = try #require(store.wallets.first { $0.name == "Contanti" })

        let row = try #require(store.rows.first)
        #expect(row.walletDisplay == "Conto")

        // The empty line fills the cell only when the column is on screen.
        store.showWalletColumn = false
        #expect(RowDraft.blank(in: store).wallet == "")
        store.showWalletColumn = true
        #expect(RowDraft.blank(in: store).wallet == store.defaultWalletName)

        // A row written against another wallet lands there, and the cell says so.
        await store.addRow(
            day: Date(),
            flowId: envelope.id,
            category: "Spesa",
            note: "contanti",
            amount: 1_000,
            walletId: cash.id
        )
        #expect(store.presentedError == nil)
        let written = try #require(store.rows.first { $0.note == "contanti" })
        #expect(written.walletDisplay == "Contanti")
    }

    @Test("The WALLET cell is editable: the diff carries the new wallet and the core moves the row")
    func walletCellEdits() async throws {
        let (store, _) = try await Self.household()
        store.showWalletColumn = true
        await store.createWallet(name: "Contanti", openingBalance: 0)
        let cash = try #require(store.wallets.first { $0.name == "Contanti" })

        let row = try #require(store.rows.first { $0.note == "mutuo" })
        var draft = RowDraft(row: row, store: store)
        #expect(draft.wallet == "Conto")

        // Typed into the cell, resolved the way the FLOW cell resolves names.
        draft.wallet = "conta"
        let patch = try draft.patch(against: row, store: store)
        #expect(patch.walletId == cash.id)

        await store.update(transactionId: row.id, patch: patch)
        #expect(store.presentedError == nil)
        let moved = try #require(store.rows.first { $0.id == row.id })
        #expect(moved.walletDisplay == "Contanti")
    }

    @Test("A wallet name that matches nothing, or two wallets, keeps the row open")
    func walletCellRefusesBadNames() async throws {
        let (store, _) = try await Self.household()
        await store.createWallet(name: "Contanti", openingBalance: 0)
        await store.createWallet(name: "Contanti extra", openingBalance: 0)

        #expect(throws: DomainError.self) { try store.resolveWallet(named: "banca") }
        #expect(throws: DomainError.self) { try store.resolveWallet(named: "cont") }
        // Blank means "leave the default", not an error.
        #expect(try store.resolveWallet(named: "  ") == nil)
    }
}
