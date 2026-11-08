import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The RIEPILOGO's flow cards: every active flow at the end of the month on
/// screen, what the month put in and took out, and the order they are
/// drawn in.
struct FlowCardsTests {
    private static let june = MonthKey(year: 2026, month: 6)

    private static func cards(
        _ flows: [FlowView],
        flowPerson: [FlowPersonTotals] = [],
        transfers: [FlowTransferTotals] = [],
        balances: [BucketTargetTotals] = []
    ) -> [FlowCard] {
        FlowCard.cards(
            flows: flows,
            month: EnvelopeYear.build(
                flows: flows,
                flowPerson: flowPerson,
                transfers: transfers,
                balances: balances,
                upTo: june,
                people: []
            )
        )
    }

    @Test("The capped flows still filling come first, then the open ones, then the full ones; Unallocated has no card")
    func order() {
        let cards = Self.cards(
            [
                TestFlow.make("Non allocato", unallocated: true),
                TestFlow.make("Cash"),
                TestFlow.make("Emergenza", mode: .netCapped(cap: 100)),
                TestFlow.make("Casa"),
                TestFlow.make("Vacanze", mode: .netCapped(cap: 100)),
                TestFlow.make("Auto", mode: .incomeCapped(cap: 100), incomeTotal: 40),
            ],
            balances: [
                BucketTargetTotals(bucket: 1, targetId: "Emergenza", delta: 100),
                BucketTargetTotals(bucket: 1, targetId: "Vacanze", delta: 30),
            ]
        )
        #expect(cards.map(\.name) == ["Vacanze", "Auto", "Cash", "Casa", "Emergenza"])
        #expect(cards.map(\.isComplete) == [false, false, false, false, true])
    }

    @Test("A cap on the balance fills with the balance at the month's end, not today's")
    func balanceAtTheMonthsEnd() throws {
        let cards = Self.cards(
            [TestFlow.make("Vacanze", balance: 90_000, mode: .netCapped(cap: 100_000))],
            balances: [
                BucketTargetTotals(bucket: 0, targetId: "Vacanze", delta: 20_000),
                BucketTargetTotals(bucket: 6, targetId: "Vacanze", delta: 30_000),
                // July is after the month on screen.
                BucketTargetTotals(bucket: 7, targetId: "Vacanze", delta: 40_000),
            ]
        )
        let card = try #require(cards.first)
        #expect(card.balance == 50_000)
        #expect(card.gauge?.filled == 50_000)
        #expect(card.gauge?.kind == .balance)
    }

    @Test("A cap on the income fills with the cumulative income, the balance while there is none")
    func incomeCap() {
        let cards = Self.cards(
            [
                TestFlow.make("Casa", mode: .incomeCapped(cap: 15_000_000), incomeTotal: 3_193_500),
                TestFlow.make("Varie", mode: .incomeCapped(cap: 500_000)),
            ],
            balances: [
                BucketTargetTotals(bucket: 2, targetId: "Casa", delta: 500),
                BucketTargetTotals(bucket: 2, targetId: "Varie", delta: 461_100),
            ]
        )
        #expect(cards.map { $0.gauge?.filled } == [3_193_500, 461_100])
        #expect(cards.map { $0.gauge?.kind } == [.income, .income])
        #expect(cards.map(\.balance) == [500, 461_100])
    }

    @Test("An open flow has no bar and is never full, whatever it holds")
    func openFlow() throws {
        let cards = Self.cards(
            [TestFlow.make("Cash")],
            balances: [BucketTargetTotals(bucket: 3, targetId: "Cash", delta: -12_000)]
        )
        let card = try #require(cards.first)
        #expect(card.gauge == nil)
        #expect(!card.isComplete)
        #expect(card.balance == -12_000)
    }

    @Test("The month's income and expense count the moves between flows")
    func monthMoves() {
        let cards = Self.cards(
            [TestFlow.make("Casa"), TestFlow.make("Vacanze", mode: .netCapped(cap: 300_000))],
            flowPerson: [
                FlowPersonTotals(flowId: "Casa", person: "elisa", income: 200_000, expense: 31_000, refund: 1_000, netExpense: 30_000),
                FlowPersonTotals(flowId: "Casa", person: "matteo", income: 0, expense: 12_000, refund: 0, netExpense: 12_000),
            ],
            transfers: [
                FlowTransferTotals(flowId: "Casa", person: "matteo", movedIn: 0, movedOut: 50_000),
                FlowTransferTotals(flowId: "Vacanze", person: "matteo", movedIn: 50_000, movedOut: 0),
            ]
        )
        #expect(cards.map(\.name) == ["Vacanze", "Casa"])
        #expect(cards.map(\.monthIncome) == [50_000, 200_000])
        #expect(cards.map(\.monthExpense) == [0, 92_000])
    }
}
