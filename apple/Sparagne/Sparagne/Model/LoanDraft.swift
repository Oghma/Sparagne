import Foundation
import SparagneCore

/// Which field of a loan form is wrong, and why, for the form to point at.
struct LoanDraftError: Error, Equatable, Sendable {
    enum Field: Equatable, Sendable {
        case name, principal, rate, duration, firstDue, installment, upfrontFees, installmentFee, payment
        case date, amount, fee
    }

    let field: Field
    /// Already localized.
    let message: String
}

/// The «Nuovo prestito» form as typed: every cell a string until it is
/// turned into terms, so a half-typed amount is never lost and the error
/// names its field.
///
/// Prepared as a contract: `terms()` refuses until it is written.
struct LoanDraft: Equatable, Sendable {
    enum RateKind: Equatable, Sendable { case tan, taeg }
    enum DurationUnit: Equatable, Sendable { case years, months }
    enum InstallmentMode: Equatable, Sendable { case computed, contract }
    enum PaymentChoice: Equatable, Sendable { case create, link, none }

    var name = ""
    var note = ""
    /// The amount borrowed, as typed.
    var principal = ""
    /// TAN or TAEG, in percent as typed ("3,125").
    var rate = ""
    var rateKind: RateKind = .tan
    var duration = ""
    var durationUnit: DurationUnit = .years
    var firstDue: Date
    var installmentMode: InstallmentMode = .computed
    /// The contract's installment, when `installmentMode` is `.contract`.
    var installment = ""
    var upfrontFees = ""
    var installmentFee = ""
    var payment: PaymentChoice = .create
    /// Where a template made with the loan records the installments.
    var target = LoanLedgerTarget(walletId: nil, flowId: nil, category: "", owner: nil)
    /// The template to link, for `.link`.
    var linkedRecurringId: Uuid?

    init(firstDue: Date = Date()) {
        self.firstDue = firstDue
    }

    /// The terms as typed, or the first field in error.
    func terms() -> Result<LoanTerms, LoanDraftError> {
        .failure(LoanDraftError(field: .principal, message: ""))
    }

    /// What the form asks for the installments.
    func paymentRequest() -> Result<LoanPayment, LoanDraftError> {
        .success(.none)
    }
}

/// The «Aggiungi evento» form as typed.
///
/// Prepared as a contract: `event()` refuses until it is written.
struct LoanEventDraft: Equatable, Sendable {
    enum Kind: Equatable, Sendable { case rateChange, prepayment }

    var kind: Kind = .prepayment
    var date: Date
    /// The new TAN, in percent as typed.
    var rate = ""
    var installmentMode: LoanDraft.InstallmentMode = .computed
    /// The installment of the bank's letter, for `.contract`.
    var installment = ""
    /// The repayment, as typed.
    var amount = ""
    var mode: PrepaymentMode = .shorten
    /// The penalty, as typed.
    var fee = ""
    /// Records the repayment and its penalty as one expense.
    var recordsExpense = true
    var expense = LoanLedgerTarget(walletId: nil, flowId: nil, category: "", owner: nil)

    init(date: Date = Date()) {
        self.date = date
    }

    /// The event as typed, or the first field in error.
    func event() -> Result<LoanEvent, LoanDraftError> {
        .failure(LoanDraftError(field: .date, message: ""))
    }
}
