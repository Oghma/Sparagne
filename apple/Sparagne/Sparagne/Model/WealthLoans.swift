import Foundation
import SparagneCore

/// Something about a loan worth a look: the table's warning icon, and the
/// notes of its inspector.
enum LoanIssue: Equatable, Hashable, Sendable {
    /// What the schedule says of itself: an installment that never covers
    /// the interest, a balloon, a repayment cut to the debt, an event after
    /// the end.
    case schedule(LoanWarning)
    /// The installments due by today that the linked template has not
    /// recorded in the ledger: `registered` of `due`.
    case unregistered(registered: UInt32, due: UInt32)
}

/// What the Patrimonio reads off a loan beside the core's overview: the rate
/// in force, whether it ever changed, and, for an archived loan the overview
/// leaves out, the same figures worked out from its schedule.
enum LoanFacts {
    /// The TAN in force today: the last rate change dated on or before
    /// today, else the terms'. The events come by date, so the last one that
    /// qualifies wins.
    static func currentRate(_ loan: LoanView, today: NaiveDate) -> UInt32 {
        var rate = loan.terms.annualRatePpm
        for view in loan.events {
            if case .rateChange(let date, let ppm, _) = view.event, date <= today {
                rate = ppm
            }
        }
        return rate
    }

    /// A loan with a rate change is a variable-rate one, the «variabile» tag.
    static func isVariable(_ loan: LoanView) -> Bool {
        loan.events.contains { if case .rateChange = $0.event { true } else { false } }
    }

    /// The rate each rate change replaced, by event id: `3,25% → 2,85%`.
    static func previousRates(_ loan: LoanView) -> [Uuid: UInt32] {
        var rate = loan.terms.annualRatePpm
        var previous: [Uuid: UInt32] = [:]
        for view in loan.events {
            if case .rateChange(_, let ppm, _) = view.event {
                previous[view.id] = rate
                rate = ppm
            }
        }
        return previous
    }

    /// The schedule's warnings, then the installments the ledger misses.
    /// A loan with no linked template has nothing to compare: the
    /// installments are the user's to record.
    static func issues(_ overview: LoanOverview) -> [LoanIssue] {
        var issues = overview.warnings.map(LoanIssue.schedule)
        if let registered = overview.registeredCount, registered < overview.paidCount {
            issues.append(.unregistered(registered: registered, due: overview.paidCount))
        }
        return issues
    }

    /// The overview of a loan the core's leaves out (an archived one), from
    /// its schedule, as of `today`: the installments due on or before it are
    /// paid. No template is compared and no warning repeated: an archived
    /// loan is only looked back on.
    static func overview(of loan: LoanView, schedule: LoanSchedule, today: NaiveDate) -> LoanOverview {
        let paid = schedule.rows.filter { $0.dueDate <= today }
        let left = schedule.rows.dropFirst(paid.count)
        let residual = paid.last?.residual ?? opening(loan, schedule: schedule)
        let next = left.first
        return LoanOverview(
            loanId: loan.id,
            residual: residual,
            paidCount: UInt32(paid.count),
            totalCount: UInt32(schedule.rows.count),
            nextDue: next?.dueDate,
            nextPayment: next.map { $0.payment + $0.fee },
            capitalRepaid: loan.terms.principal - residual,
            interestPaid: paid.reduce(0) { $0 + $1.interest },
            interestLeft: left.reduce(0) { $0 + $1.interest },
            endDate: schedule.rows.last?.dueDate,
            taegPpm: schedule.taegPpm,
            totalCost: schedule.totalCost,
            registeredCount: nil,
            warnings: []
        )
    }

    /// The debt before the first installment: the principal, less the
    /// early repayments made before it. Nothing once they paid it all.
    static func opening(_ loan: LoanView, schedule: LoanSchedule) -> Int64 {
        let before = Set(schedule.eventIdsBeforeFirst)
        let repaid = loan.events.reduce(Int64(0)) { sum, view in
            guard before.contains(view.id), case .prepayment(_, let amount, _, _) = view.event else { return sum }
            return sum + amount
        }
        return max(loan.terms.principal - repaid, 0)
    }

    /// The residual right after the installments due on or before `day`,
    /// and whether one falls on it: what an event dated `day` starts from.
    static func residual(on day: NaiveDate, loan: LoanView, schedule: LoanSchedule) -> (amount: Int64, afterInstallment: Bool) {
        guard let row = schedule.rows.last(where: { $0.dueDate <= day }) else {
            return (opening(loan, schedule: schedule), false)
        }
        return (row.residual, row.dueDate == day)
    }
}

/// One line of the «Prestiti e mutui» table.
struct LoanRow: Identifiable, Equatable, Sendable {
    let id: Uuid
    let name: String
    let archived: Bool
    /// It has a rate change: the «variabile» tag.
    let variable: Bool
    /// Nothing left to pay: the «estinto» tag.
    let paidOff: Bool
    /// The next installment with its fee; a loan with none left shows its
    /// last one. `nil` when nothing is known of it.
    let installment: Int64?
    /// The TAN in force today.
    let rate: UInt32
    /// Installments due by today, of `total`.
    let paid: UInt32?
    let total: UInt32?
    let residual: Int64?
    /// The last installment's due date.
    let end: NaiveDate?
    /// Empty for an archived loan.
    let issues: [LoanIssue]

    /// How much of the schedule is behind: the meter under `paid/total`.
    var progress: Double? {
        guard let paid, let total, total > 0 else { return nil }
        return Double(paid) / Double(total)
    }

    /// The loans as the table lists them: the running ones in the vault's
    /// order (by name), then the archived ones. `overview` covers the
    /// running loans; `schedules` stands in for the ones it leaves out,
    /// when the tab has loaded theirs.
    static func rows(
        loans: [LoanView],
        overview: [LoanOverview],
        schedules: [Uuid: LoanSchedule],
        today: NaiveDate
    ) -> [LoanRow] {
        let byLoan = Dictionary(overview.map { ($0.loanId, $0) }, uniquingKeysWith: { first, _ in first })
        let ordered = loans.filter { !$0.archived } + loans.filter(\.archived)
        return ordered.map { loan in
            let schedule = schedules[loan.id]
            let figures = byLoan[loan.id] ?? schedule.map { LoanFacts.overview(of: loan, schedule: $0, today: today) }
            let last = schedule?.rows.last.map { $0.payment + $0.fee }
            let contract: Int64? = if case .contract(let amount) = loan.terms.installment {
                amount + loan.terms.installmentFee
            } else {
                nil
            }
            return LoanRow(
                id: loan.id,
                name: loan.name,
                archived: loan.archived,
                variable: LoanFacts.isVariable(loan),
                paidOff: figures.map { $0.residual == 0 } ?? false,
                installment: figures?.nextPayment ?? last ?? contract,
                rate: LoanFacts.currentRate(loan, today: today),
                paid: figures?.paidCount,
                total: figures?.totalCount,
                residual: figures?.residual,
                end: figures?.endDate,
                issues: loan.archived ? [] : figures.map(LoanFacts.issues) ?? []
            )
        }
    }
}
