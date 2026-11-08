import Foundation
import SparagneCore

/// How a new loan's installments reach the ledger.
enum LoanPayment: Equatable, Sendable {
    /// Nothing records them: the user types them, or the bank's statement
    /// brings them.
    case none
    /// A new monthly expense template, made with the loan in one batch.
    case create(LoanLedgerTarget)
    /// An expense template of the vault that no other loan links.
    case link(Uuid)
}

/// Where a loan's money is recorded in the ledger: the linked template's
/// installments, or an early repayment's expense. `nil` wallet = the only
/// active one, `nil` flow = Unallocated, as for any entry.
struct LoanLedgerTarget: Equatable, Sendable {
    var walletId: Uuid?
    var flowId: Uuid?
    /// Free text, resolved by the core; blank = Uncategorized.
    var category: String
    /// Who the rows are for; `nil` = the author.
    var owner: String?
}

/// The Patrimonio tab's commands. Every write that changes a linked loan's
/// installment or end also brings its template in line in the same batch
/// (`UpdateRecurring` of amount and end date), so Spese fisse and the
/// Ricorrenze tab say the current installment.
///
/// Prepared as a contract: every write says it did nothing, every read
/// finds nothing.
extension AppStore {
    /// Makes a loan, with its template when `payment` asks for one. Says
    /// whether it went through; a refusal is already on the alert.
    @discardableResult
    func createLoan(name: String, note: String?, terms: LoanTerms, payment: LoanPayment) async -> Bool {
        false
    }

    /// Saves the loan's name, note and terms as edited.
    @discardableResult
    func updateLoan(_ loan: LoanView, name: String, note: String?, terms: LoanTerms) async -> Bool {
        false
    }

    /// Adds a rate change or an early repayment; with `expense`, the
    /// repayment and its penalty are recorded as one expense in the same
    /// batch.
    @discardableResult
    func addLoanEvent(_ event: LoanEvent, to loan: LoanView, expense: LoanLedgerTarget?) async -> Bool {
        false
    }

    @discardableResult
    func removeLoanEvent(_ eventId: Uuid, from loan: LoanView) async -> Bool {
        false
    }

    /// Links the loan to an expense template, or unlinks it with `nil`.
    @discardableResult
    func linkLoan(_ loan: LoanView, to recurringId: Uuid?) async -> Bool {
        false
    }

    /// Makes the loan's template from its schedule and links it, in one
    /// batch.
    @discardableResult
    func createLoanRecurring(for loan: LoanView, target: LoanLedgerTarget) async -> Bool {
        false
    }

    @discardableResult
    func setLoanArchived(_ loan: LoanView, _ archived: Bool) async -> Bool {
        false
    }

    /// One saved loan's whole schedule, `nil` when the core refuses.
    func loanSchedule(_ loanId: Uuid) async -> LoanSchedule? {
        nil
    }

    /// The schedule of a form not saved yet, `nil` when the terms are refused.
    func previewLoan(terms: LoanTerms, events: [LoanEventView]) async -> LoanSchedule? {
        nil
    }

    /// The active expense templates a loan may link: none linked by another
    /// loan, `loan`'s own included.
    func linkableRecurring(for loan: LoanView?) -> [RecurringView] {
        []
    }
}
