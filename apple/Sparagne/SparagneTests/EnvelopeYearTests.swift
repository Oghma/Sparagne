import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The RIEPILOGO's "Buste" table: every active envelope's balance at the end
/// of the month on screen, and what the year put in and took out of it.
struct EnvelopeYearTests {
    private static let june = MonthKey(year: 2026, month: 6)

    /// An envelope whose id is its name, empty.
    static func flow(_ name: String, mode: FlowMode = .unlimited, unallocated: Bool = false) -> FlowView {
        FlowView(
            id: name,
            name: name,
            balance: 0,
            mode: mode,
            incomeTotal: nil,
            allowNegative: false,
            archived: false,
            isUnallocated: unallocated
        )
    }

    private static func line(_ flow: String, _ person: String, income: Int64 = 0, netExpense: Int64 = 0) -> FlowPersonTotals {
        FlowPersonTotals(
            flowId: flow, person: person, income: income, expense: netExpense, refund: 0, netExpense: netExpense
        )
    }

    /// The core's order puts Unallocated first already; the table does not
    /// rely on it.
    private static var flows: [FlowView] {
        [
            flow("Spesa"),
            flow("Vacanze", mode: .netCapped(cap: 300_000)),
            flow("Non allocato", unallocated: true),
            flow("Casa", mode: .incomeCapped(cap: 1_500_000)),
        ]
    }

    @Test("Unallocated comes first, then the envelopes in the vault's order")
    func ordering() {
        let year = EnvelopeYear.build(flows: Self.flows, flowPerson: [], balances: [], upTo: Self.june, people: [])
        #expect(year.rows.map(\.name) == ["Non allocato", "Spesa", "Vacanze", "Casa"])
        #expect(year.rows.map(\.isUnallocated) == [true, false, false, false])
    }

    @Test("A fund says what its cap is measured on, an envelope without one says nothing")
    func capKinds() {
        let year = EnvelopeYear.build(flows: Self.flows, flowPerson: [], balances: [], upTo: Self.june, people: [])
        #expect(year.rows.map(\.capKind) == [nil, nil, .balance, .income])
    }

    @Test("The balance is everything up to the end of the month on screen, the year before included")
    func balanceUpToTheMonth() {
        let year = EnvelopeYear.build(
            flows: Self.flows,
            flowPerson: [],
            balances: [
                BucketTargetTotals(bucket: 0, targetId: "Vacanze", delta: 100_000),
                BucketTargetTotals(bucket: 2, targetId: "Vacanze", delta: 50_000),
                // A move out of the fund in June counts: it is where the
                // money stands, whatever moved it.
                BucketTargetTotals(bucket: 6, targetId: "Vacanze", delta: -20_000),
                // July is after the month on screen.
                BucketTargetTotals(bucket: 7, targetId: "Vacanze", delta: -100_000),
                BucketTargetTotals(bucket: 1, targetId: "Spesa", delta: 7_000),
                // An envelope the table does not list is nobody's balance.
                BucketTargetTotals(bucket: 1, targetId: "Vecchia", delta: 9_000),
            ],
            upTo: Self.june,
            people: []
        )
        #expect(year.rows.map(\.balance) == [0, 7_000, 130_000, 0])
    }

    @Test("Income and expense are everybody's, and the expense splits by person")
    func incomeExpenseAndSplit() throws {
        let year = EnvelopeYear.build(
            flows: Self.flows,
            flowPerson: [
                Self.line("Spesa", "matteo", income: 10_000, netExpense: 42_000),
                Self.line("Spesa", "elisa", income: 5_000, netExpense: 18_000),
                // Someone who is not a column still counts in the total.
                Self.line("Spesa", "giulia", netExpense: 1_000),
                Self.line("Vacanze", "elisa", income: 30_000),
            ],
            balances: [],
            upTo: Self.june,
            people: ["elisa", "matteo"]
        )
        #expect(year.people == ["elisa", "matteo"])
        let spesa = try #require(year.rows.first { $0.id == "Spesa" })
        #expect(spesa.income == 15_000)
        #expect(spesa.expense == 61_000)
        #expect(spesa.expenseByPerson == [18_000, 42_000])
        // Elisa paid into the fund and spent nothing from it; Matteo has no
        // row on it at all.
        let vacanze = try #require(year.rows.first { $0.id == "Vacanze" })
        #expect(vacanze.income == 30_000)
        #expect(vacanze.expense == 0)
        #expect(vacanze.expenseByPerson == [0, 0])
        // An envelope nobody touched is zeros, one per person.
        let casa = try #require(year.rows.first { $0.id == "Casa" })
        #expect(casa.income == 0)
        #expect(casa.expenseByPerson == [0, 0])
    }

    @Test("A move between envelopes comes into the one it reaches and goes out of the one it leaves, for whoever made it")
    func movesAreInAndOut() throws {
        let year = EnvelopeYear.build(
            flows: Self.flows,
            flowPerson: [Self.line("Vacanze", "elisa", income: 5_000, netExpense: 20_000)],
            transfers: [
                // The allocation plan's share-out, then Elisa moving some of
                // the holidays' money to the shopping.
                FlowTransferTotals(flowId: "Non allocato", person: "matteo", movedIn: 0, movedOut: 150_000),
                FlowTransferTotals(flowId: "Vacanze", person: "matteo", movedIn: 100_000, movedOut: 0),
                FlowTransferTotals(flowId: "Spesa", person: "matteo", movedIn: 50_000, movedOut: 0),
                FlowTransferTotals(flowId: "Vacanze", person: "elisa", movedIn: 0, movedOut: 30_000),
                FlowTransferTotals(flowId: "Spesa", person: "elisa", movedIn: 30_000, movedOut: 0),
            ],
            balances: [],
            upTo: Self.june,
            people: ["elisa", "matteo"]
        )
        let unallocated = try #require(year.rows.first { $0.id == "Non allocato" })
        #expect(unallocated.income == 0)
        #expect(unallocated.expense == 150_000)
        #expect(unallocated.expenseByPerson == [0, 150_000])
        let vacanze = try #require(year.rows.first { $0.id == "Vacanze" })
        #expect(vacanze.income == 105_000)
        #expect(vacanze.expense == 50_000)
        #expect(vacanze.expenseByPerson == [50_000, 0])
        let spesa = try #require(year.rows.first { $0.id == "Spesa" })
        #expect(spesa.income == 80_000)
        #expect(spesa.expense == 0)
        // A move only passes money along: the envelopes take in what they
        // give out, and only the real income and expense are left over.
        #expect(year.rows.reduce(0) { $0 + $1.income } - 5_000 == year.rows.reduce(0) { $0 + $1.expense } - 20_000)
    }

    @Test("A vault with no envelopes has no table")
    func noEnvelopes() {
        let year = EnvelopeYear.build(flows: [], flowPerson: [], balances: [], upTo: Self.june, people: ["matteo"])
        #expect(year.rows.isEmpty)
        #expect(year.people == ["matteo"])
    }
}
