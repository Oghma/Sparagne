import Testing

@testable import SparagneCore

/// One in-memory database with a vault and a `Cash` wallet holding 100.00.
private struct Fixture {
    let core: CoreHandle
    let vaultId: Uuid
    let walletId: Uuid

    static let author = "alice"
    static let now: OffsetDateTime = "2026-03-01T12:00:00+01:00"

    init() throws {
        core = try CoreHandle.openInMemory()

        let vaultReceipt = try core.execute(
            envelope: createVaultEnvelope(author: Self.author, name: "Main", currency: .eur)
        )
        vaultId = try #require(vaultReceipt.resultId)

        let walletReceipt = try core.execute(
            envelope: newEnvelope(
                vaultId: vaultId,
                author: Self.author,
                command: .createWallet(
                    name: "Cash",
                    openingBalance: 10000,
                    occurredAt: Self.now
                )
            )
        )
        walletId = try #require(walletReceipt.resultId)
    }

    func execute(_ command: Command) throws -> Receipt {
        try core.execute(
            envelope: newEnvelope(vaultId: vaultId, author: Self.author, command: command)
        )
    }
}

/// Non-transfer, non-voided transactions of the given kinds; `nil` = all kinds.
/// Every other field of the filter keeps its default.
private func filter(kinds: [TransactionKind]? = nil) -> TransactionFilter {
    TransactionFilter(kinds: kinds)
}

@Test("A quick-add line becomes a command, an entry and a balance")
func quickAddRoundTrip() throws {
    let fixture = try Fixture()

    let parsed = try parseQuickAdd(input: "-12.50 pizza #food @cash", currency: .eur)
    guard case .entry(let kind, let amount, let note, let category, let wallet, _, _) = parsed
    else {
        Issue.record("expected an entry, got \(parsed)")
        return
    }
    #expect(kind == .expense)
    #expect(amount == 1250)
    #expect(note == "pizza")
    #expect(category == "food")
    #expect(wallet == "cash")

    let resolved = try fixture.core.resolveQuickAdd(
        vaultId: fixture.vaultId,
        parsed: parsed,
        now: Fixture.now,
        defaults: QuickAddDefaults()
    )
    guard case .expense(let entry) = resolved.command else {
        Issue.record("expected an expense, got \(resolved.command)")
        return
    }
    #expect(entry.walletId == fixture.walletId)
    // The resolution hands the ids over, so the app never digs into the command.
    #expect(resolved.walletId == fixture.walletId)
    #expect(resolved.fromId == nil)

    _ = try fixture.core.execute(
        envelope: newEnvelope(
            vaultId: fixture.vaultId, author: Fixture.author, command: resolved.command)
    )

    let snapshot = try fixture.core.snapshot(vaultId: fixture.vaultId)
    let cash = try #require(snapshot.wallets.first { $0.id == fixture.walletId })
    #expect(cash.balance == 8750)

    // The wallet's opening balance is a real income transaction on the system
    // category `Opening`, so filter down to the expense.
    let page = try fixture.core.listTransactions(
        vaultId: fixture.vaultId,
        filter: filter(kinds: [.expense]),
        limit: 50,
        cursor: nil
    )
    #expect(page.nextCursor == nil)
    #expect(page.items.count == 1)
    let transaction = try #require(page.items.first)
    #expect(transaction.kind == .expense)
    #expect(transaction.amount == 1250)
    #expect(transaction.category == "food")
    #expect(transaction.categoryIsSystem == false)
    #expect(transaction.voided == false)
    #expect(transaction.legs.count == 2)
    #expect(transaction.walletId == fixture.walletId)
    #expect(transaction.flowId != nil)
    #expect(transaction.fromId == nil)

    let all = try fixture.core.listTransactions(
        vaultId: fixture.vaultId,
        filter: filter(),
        limit: 50,
        cursor: nil
    )
    #expect(all.items.count == 2)
    #expect(all.items.contains { $0.category == "Opening" && $0.kind == .income })
}

@Test("Money formats with the vault currency")
func moneyFormatting() {
    #expect(formatMoney(minor: -1250, currency: .eur) == "-12.50 EUR")
    #expect(formatMoney(minor: 0, currency: .eur) == "0.00 EUR")
}

@Test("Money parses major units into minor units")
func moneyParsing() throws {
    #expect(try parseMoney(text: "12,50", currency: .eur) == 1250)
    #expect(throws: DomainError.self) {
        try parseMoney(text: "12.345", currency: .eur)
    }
}

@Test("Spending more than an envelope holds throws a matchable DomainError")
func insufficientFundsIsCatchableByCase() throws {
    let fixture = try Fixture()

    let flowReceipt = try fixture.execute(
        .createFlow(
            name: "Vacanze",
            mode: .unlimited,
            allowNegative: false,
            openingAllocation: 0,
            occurredAt: Fixture.now
        )
    )
    let flowId = try #require(flowReceipt.resultId)

    do {
        _ = try fixture.execute(
            .expense(
                Entry(
                    amount: 500,
                    walletId: fixture.walletId,
                    flowId: flowId,
                    category: "food",
                    note: nil,
                    occurredAt: Fixture.now
                )
            )
        )
        Issue.record("expected the expense to be refused")
    } catch let error as DomainError {
        guard case .InsufficientFunds(let message) = error else {
            Issue.record("expected insufficient funds, got \(error)")
            return
        }
        #expect(message.contains("Vacanze"))
        #expect(error.code == "insufficient_funds")
    }

    // Nothing was written: only the wallet's opening balance is on the log.
    let page = try fixture.core.listTransactions(
        vaultId: fixture.vaultId,
        filter: filter(),
        limit: 50,
        cursor: nil
    )
    #expect(page.items.count == 1)
    #expect(page.items.allSatisfy { $0.kind == .income })
}

@Test("A malformed id lifts into a domain error instead of panicking")
func malformedIdBecomesADomainError() throws {
    let fixture = try Fixture()
    do {
        _ = try fixture.core.snapshot(vaultId: "not-a-uuid")
        Issue.record("expected the lift to fail")
    } catch let error as DomainError {
        #expect(error.code == "invalid_command")
    }
}

@Test("Calendar dates round-trip as yyyy-MM-dd")
func naiveDatesRoundTrip() throws {
    #expect(try resolveDateSpec(spec: .daysAgo(3), today: "2026-03-10") == "2026-03-07")
    #expect(try resolveDateSpec(spec: .yesterday, today: "2026-03-01") == "2026-02-28")
    #expect(throws: QuickAddError.self) {
        try resolveDateSpec(spec: .dayMonth(day: 31, month: 2), today: "2026-03-10")
    }

    let fixture = try Fixture()
    _ = try fixture.execute(
        .createRecurring(
            transactionKind: .expense,
            amount: 4900,
            walletId: fixture.walletId,
            flowId: nil,
            category: "rent",
            note: "flat",
            schedule: Schedule(
                frequency: .monthly(day: 31),
                interval: 1,
                startDate: "2026-01-01",
                endDate: nil
            )
        )
    )

    let pending = try fixture.core.pendingRecurring(vaultId: fixture.vaultId, today: "2026-03-15")
    #expect(pending.count == 1)
    let template = try #require(pending.first)
    // The 31st clamps to the length of each month without drifting.
    #expect(template.due == ["2026-01-31", "2026-02-28"])
    #expect(template.template.schedule.frequency == .monthly(day: 31))
}

@Test("A schedule previews its next dates without a database")
func scheduleOccurrencesPreview() throws {
    let schedule = Schedule(
        frequency: .monthly(day: 31),
        interval: 1,
        startDate: "2027-01-01",
        endDate: nil
    )
    #expect(
        try scheduleOccurrences(schedule: schedule, from: "2027-01-15", limit: 3)
            == ["2027-01-31", "2027-02-28", "2027-03-31"]
    )
    #expect(throws: DomainError.self) {
        try scheduleOccurrences(
            schedule: Schedule(
                frequency: .monthly(day: 0), interval: 1, startDate: "2027-01-01", endDate: nil),
            from: "2027-01-01",
            limit: 3
        )
    }
}

@Test("An ambiguous wallet name comes back with its candidates")
func ambiguousNameCarriesTheCandidates() throws {
    let fixture = try Fixture()
    _ = try fixture.execute(.createWallet(name: "Bank", openingBalance: 0, occurredAt: Fixture.now))
    _ = try fixture.execute(
        .createWallet(name: "Bancoposta", openingBalance: 0, occurredAt: Fixture.now))

    let parsed = try parseQuickAdd(input: "-5.00 hotel @ban", currency: .eur)
    do {
        _ = try fixture.core.resolveQuickAdd(
            vaultId: fixture.vaultId,
            parsed: parsed,
            now: Fixture.now,
            defaults: QuickAddDefaults()
        )
        Issue.record("expected the name to be ambiguous")
    } catch let error as QuickAddError {
        guard case .AmbiguousName(let name, let candidates) = error else {
            Issue.record("expected an ambiguous name, got \(error)")
            return
        }
        #expect(name == "ban")
        #expect(candidates.sorted() == ["Bancoposta", "Bank"])
        #expect(error.code == "ambiguous_name")
        #expect(error.candidates.count == 2)
        #expect(error.message.contains("Bank"))
    }
}

@Test("Period totals with no bounds cover the whole ledger")
func periodTotalsWithoutBounds() throws {
    let fixture = try Fixture()
    let parsed = try parseQuickAdd(input: "-12.50 pizza #food", currency: .eur)
    let resolved = try fixture.core.resolveQuickAdd(
        vaultId: fixture.vaultId,
        parsed: parsed,
        now: Fixture.now,
        defaults: QuickAddDefaults()
    )
    _ = try fixture.execute(resolved.command)

    let all = try fixture.core.periodTotals(vaultId: fixture.vaultId, from: nil, to: nil)
    #expect(all.expense == 1250)
    // The wallet's opening balance is an income of 100.00.
    #expect(all.income == 10000)

    // A window that ends before the expense sees only the opening balance.
    let before = try fixture.core.periodTotals(
        vaultId: fixture.vaultId,
        from: nil,
        to: "2026-03-01T10:00:00Z"
    )
    #expect(before.expense == 0)

    // from >= to is still refused when both are given.
    #expect(throws: DomainError.self) {
        try fixture.core.periodTotals(
            vaultId: fixture.vaultId,
            from: "2026-03-02T00:00:00Z",
            to: "2026-03-01T00:00:00Z"
        )
    }
}
