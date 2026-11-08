import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Hand-made loans, overviews and schedules for the Patrimonio's pure
/// models: what the core would hand the tab, without a core.
enum WealthFixture {
    static let today: NaiveDate = "2026-11-08"

    static func terms(
        principal: Int64 = 16_500_000,
        rate: UInt32 = 32_500,
        months: UInt32 = 300,
        firstDue: NaiveDate = "2024-03-01",
        installment: LoanInstallment = .computed,
        upfrontFees: Int64 = 0,
        installmentFee: Int64 = 0
    ) -> LoanTerms {
        LoanTerms(
            principal: principal,
            annualRatePpm: rate,
            months: months,
            firstDue: firstDue,
            installment: installment,
            upfrontFees: upfrontFees,
            installmentFee: installmentFee
        )
    }

    static func loan(
        id: Uuid = "loan-1",
        name: String = "Mutuo casa",
        terms: LoanTerms = terms(),
        events: [LoanEventView] = [],
        recurringId: Uuid? = nil,
        archived: Bool = false
    ) -> LoanView {
        LoanView(
            id: id,
            name: name,
            note: nil,
            terms: terms,
            events: events,
            recurringId: recurringId,
            archived: archived,
            createdBy: "matteo"
        )
    }

    static func overview(
        loanId: Uuid = "loan-1",
        residual: Int64 = 15_249_096,
        paid: UInt32 = 33,
        total: UInt32 = 300,
        nextPayment: Int64? = 77_188,
        endDate: NaiveDate? = "2049-02-01",
        registered: UInt32? = nil,
        warnings: [LoanWarning] = []
    ) -> LoanOverview {
        LoanOverview(
            loanId: loanId,
            residual: residual,
            paidCount: paid,
            totalCount: total,
            nextDue: nextPayment == nil ? nil : "2026-12-01",
            nextPayment: nextPayment,
            capitalRepaid: 0,
            interestPaid: 0,
            interestLeft: 0,
            endDate: endDate,
            taegPpm: nil,
            totalCost: 0,
            registeredCount: registered,
            warnings: warnings
        )
    }

    static func row(
        _ number: UInt32,
        due: NaiveDate,
        payment: Int64 = 10_000,
        interest: Int64 = 1_000,
        fee: Int64 = 0,
        residual: Int64,
        events: [Uuid] = []
    ) -> LoanScheduleRow {
        LoanScheduleRow(
            number: number,
            dueDate: due,
            payment: payment,
            interest: interest,
            capital: payment - interest,
            fee: fee,
            residual: residual,
            annualRatePpm: 32_500,
            eventIds: events
        )
    }

    static func schedule(
        _ rows: [LoanScheduleRow],
        beforeFirst: [Uuid] = [],
        totalInterest: Int64? = nil,
        warnings: [LoanWarning] = []
    ) -> LoanSchedule {
        LoanSchedule(
            rows: rows,
            eventIdsBeforeFirst: beforeFirst,
            totalInterest: totalInterest ?? rows.reduce(0) { $0 + $1.interest },
            totalFees: rows.reduce(0) { $0 + $1.fee },
            totalPrepaid: 0,
            totalPenalties: 0,
            totalCost: totalInterest ?? rows.reduce(0) { $0 + $1.interest },
            taegPpm: 33_700,
            warnings: warnings
        )
    }

    static func wallet(_ name: String, _ balance: Int64, archived: Bool = false) -> WalletView {
        WalletView(id: "wallet-\(name)", name: name, balance: balance, archived: archived)
    }
}

/// The Patrimonio's three cards and its status line.
struct WealthFiguresTests {
    private typealias F = WealthFixture

    @Test("Liquidity is the active wallets' balances, archived ones left out, in the vault's order")
    func liquidityFromActiveWallets() {
        let figures = WealthFigures.build(
            wallets: [F.wallet("Conto", 4_380_623), F.wallet("Vecchio", 50_000, archived: true), F.wallet("Contanti", 36_000)],
            loans: [],
            overview: []
        )
        #expect(figures.liquidity == 4_416_623)
        #expect(figures.wallets.map(\.name) == ["Conto", "Contanti"])
        #expect(WealthText.wallets(figures.wallets) == "Conto 43.806,23 \u{00B7} Contanti 360,00")
    }

    @Test("The debt is the residuals of the loans still running, archived and paid-off ones left out")
    func debtFromActiveLoans() {
        let figures = WealthFigures.build(
            wallets: [],
            loans: [
                F.loan(id: "mutuo"),
                F.loan(id: "auto", name: "Prestito auto"),
                F.loan(id: "cucina", name: "Rata cucina", archived: true),
                F.loan(id: "tv", name: "Televisore"),
            ],
            overview: [
                F.overview(loanId: "mutuo", residual: 15_249_096, nextPayment: 77_188),
                F.overview(loanId: "auto", residual: 926_595, nextPayment: 26_792),
                // Archived since the overview was read: it counts no more.
                F.overview(loanId: "cucina", residual: 40_000, nextPayment: 20_000),
                // Paid off but not archived: nothing owed, nothing next.
                F.overview(loanId: "tv", residual: 0, nextPayment: nil),
            ]
        )
        #expect(figures.debt == 16_175_691)
        #expect(figures.activeLoans == 2)
        // The status line's «rate al mese 1.039,80».
        #expect(figures.monthlyInstallments == 103_980)
    }

    @Test("Net worth is liquidity less debt, below zero while the loans owe more")
    func netWorthSign() {
        let owing = WealthFigures.build(
            wallets: [F.wallet("Conto", 4_416_623)],
            loans: [F.loan()],
            overview: [F.overview(residual: 16_175_691)]
        )
        #expect(owing.netWorth == -11_759_068)

        let ahead = WealthFigures.build(wallets: [F.wallet("Conto", 1_000_000)], loans: [F.loan()], overview: [F.overview(residual: 400_000)])
        #expect(ahead.netWorth == 600_000)

        let none = WealthFigures.build(wallets: [F.wallet("Conto", 1_000)], loans: [], overview: [])
        #expect(none.netWorth == 1_000)
        #expect(none.activeLoans == 0)
        #expect(none.debt == 0)
    }
}
