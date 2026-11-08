import Foundation
import SparagneCore

/// What an event would change in a loan's schedule, the «Aggiungi evento»
/// preview: the installment after it, the end, and the interest it saves,
/// each beside what the schedule says now.
struct LoanEventEffect: Equatable, Sendable {
    /// The first installment due after the event, without its fee.
    let installment: Int64?
    let previousInstallment: Int64?
    /// That installment's fee: the linked template records both.
    let fee: Int64
    let end: NaiveDate?
    let previousEnd: NaiveDate?
    /// The interest the event takes off the schedule; below zero when it
    /// adds some (a higher rate).
    let interestSaved: Int64
    /// What the new schedule warns of that the current one does not.
    let newWarnings: [LoanWarning]

    var installmentChanged: Bool { installment != previousInstallment }
    var endChanged: Bool { end != previousEnd }

    /// `current` is the saved schedule, `next` the one with the event added;
    /// the event takes effect on `date`.
    static func of(current: LoanSchedule, next: LoanSchedule, date: NaiveDate) -> LoanEventEffect {
        let after = next.rows.first { $0.dueDate > date }
        let before = current.rows.first { $0.dueDate > date }
        return LoanEventEffect(
            installment: after?.payment,
            previousInstallment: before?.payment,
            fee: after?.fee ?? 0,
            end: next.rows.last?.dueDate,
            previousEnd: current.rows.last?.dueDate,
            interestSaved: current.totalInterest - next.totalInterest,
            newWarnings: next.warnings.filter { !current.warnings.contains($0) }
        )
    }

    /// The same early repayment with the other mode: what the sheet's note
    /// compares the chosen one with.
    static func otherMode(_ event: LoanEvent) -> LoanEvent? {
        guard case .prepayment(let date, let amount, let mode, let fee) = event else { return nil }
        return .prepayment(date: date, amount: amount, mode: mode == .shorten ? .lower : .shorten, fee: fee)
    }

    /// The id a previewed event carries: it sorts after every real one, as a
    /// new event made today would among those of its date.
    static let previewId: Uuid = "ffffffff-ffff-7fff-bfff-ffffffffffff"
}
