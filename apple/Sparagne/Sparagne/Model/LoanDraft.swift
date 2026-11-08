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

    /// A saved loan's fields, written the way `terms()` reads them back:
    /// amounts as the window writes them (`200.000,00`), the TAN with the
    /// decimal comma, the duration in years when it is whole years, fees of
    /// zero left blank. A linked loan's payment is that link; the edit itself
    /// never sends it (`AppStore.updateLoan`).
    init(loan: LoanView) {
        let terms = loan.terms
        firstDue = CoreDate.localDay(terms.firstDue) ?? Date()
        name = loan.name
        note = loan.note ?? ""
        principal = LedgerMoney.bare(terms.principal)
        rate = LoanRate.text(terms.annualRatePpm)
        if terms.months % 12 == 0 {
            duration = "\(terms.months / 12)"
            durationUnit = .years
        } else {
            duration = "\(terms.months)"
            durationUnit = .months
        }
        switch terms.installment {
        case .computed:
            installmentMode = .computed
        case .contract(let amount):
            installmentMode = .contract
            installment = LedgerMoney.bare(amount)
        }
        upfrontFees = terms.upfrontFees == 0 ? "" : LedgerMoney.bare(terms.upfrontFees)
        installmentFee = terms.installmentFee == 0 ? "" : LedgerMoney.bare(terms.installmentFee)
        payment = loan.recurringId == nil ? .none : .link
        linkedRecurringId = loan.recurringId
    }

    // MARK: - What the fields say

    /// The TAN comes from a TAEG: exact only for a loan without fees, an
    /// overestimate of the installment with them, which the form says.
    var rateIsEstimate: Bool { rateKind == .taeg }

    /// The TAN is worked out from the contract's installment, the rate being
    /// left blank.
    var solvesRate: Bool { installmentMode == .contract && Self.isBlank(rate) }

    /// The name, trimmed; a blank one is refused. Apart from `terms()`, so
    /// the preview runs before the loan has a name.
    func loanName() -> Result<String, LoanDraftError> {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return .failure(LoanDraftError(field: .name, message: LoanText.nameMissing)) }
        return .success(trimmed)
    }

    /// The note, trimmed; `nil` when blank.
    var noteText: String? {
        let trimmed = note.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    /// The terms as typed, or the first field in error, in the form's order:
    /// amount, rate, duration, installment, fees. The name is `loanName()`'s.
    ///
    /// With a TAEG the TAN is `tanFromTaeg`'s; with the contract's
    /// installment and no rate, `solveLoanRate`'s, the installment kept as
    /// the contract says (it wins over the formula).
    func terms(currency: Currency = .eur) -> Result<LoanTerms, LoanDraftError> {
        LoanDraftError.catching { try readTerms(currency: currency) }
    }

    private func readTerms(currency: Currency) throws -> LoanTerms {
        let principal = try LoanMoney.read(
            self.principal,
            field: .principal,
            currency: currency,
            blank: LoanText.principalMissing,
            rule: .positive
        )
        // A rate typed is read now, in the form's order; a blank one waits
        // for the installment, which may give it.
        let typedRate: UInt32? = try Self.isBlank(rate) ? nil : readRate()
        let months = try readMonths()
        let firstDue = CoreDate.day(self.firstDue)
        let installment: LoanInstallment
        switch installmentMode {
        case .computed:
            installment = .computed
        case .contract:
            installment = .contract(amount: try LoanMoney.read(
                self.installment,
                field: .installment,
                currency: currency,
                blank: LoanText.installmentMissing,
                rule: .positive
            ))
        }
        let annualRate: UInt32
        if let typedRate {
            annualRate = typedRate
        } else if case .contract(let amount) = installment {
            annualRate = try Self.solvedRate(principal: principal, installment: amount, months: months)
        } else {
            throw LoanDraftError(field: .rate, message: LoanText.rateMissing)
        }
        let upfront = try LoanMoney.read(upfrontFees, field: .upfrontFees, currency: currency, blank: nil, rule: .notNegative)
        let fee = try LoanMoney.read(installmentFee, field: .installmentFee, currency: currency, blank: nil, rule: .notNegative)
        return LoanTerms(
            principal: principal,
            annualRatePpm: annualRate,
            months: months,
            firstDue: firstDue,
            installment: installment,
            upfrontFees: upfront,
            installmentFee: fee
        )
    }

    /// The TAN the rate field says: as typed, or from the TAEG typed.
    private func readRate() throws -> UInt32 {
        guard let typed = LoanRate.ppm(rate) else {
            throw LoanDraftError(field: .rate, message: LoanText.rateUnreadable)
        }
        let tan: UInt32
        switch rateKind {
        case .tan:
            tan = typed
        case .taeg:
            do {
                tan = try tanFromTaeg(taegPpm: typed)
            } catch {
                throw LoanDraftError(field: .rate, message: LoanText.taegUnusable)
            }
        }
        guard tan <= LoanRate.maxPpm else { throw LoanDraftError(field: .rate, message: LoanText.rateRange) }
        return tan
    }

    private func readMonths() throws -> UInt32 {
        let trimmed = duration.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { throw LoanDraftError(field: .duration, message: LoanText.durationMissing) }
        guard trimmed.allSatisfy({ ("0"..."9").contains($0) }), trimmed.count <= 4, let count = UInt32(trimmed) else {
            throw LoanDraftError(field: .duration, message: LoanText.durationRange)
        }
        let months = durationUnit == .years ? count * 12 : count
        guard (1...LoanRate.maxMonths).contains(months) else {
            throw LoanDraftError(field: .duration, message: LoanText.durationRange)
        }
        return months
    }

    /// The TAN at which the contract's installment repays the amount in
    /// `months`; refused on the installment, the figure that does not add up.
    private static func solvedRate(principal: Int64, installment: Int64, months: UInt32) throws -> UInt32 {
        let solved: UInt32
        do {
            solved = try solveLoanRate(principal: principal, installment: installment, months: months)
        } catch {
            throw LoanDraftError(field: .installment, message: LoanText.installmentTooLow)
        }
        guard solved <= LoanRate.maxPpm else {
            throw LoanDraftError(field: .installment, message: LoanText.installmentTooHigh)
        }
        return solved
    }

    /// What the form asks for the installments.
    func paymentRequest() -> Result<LoanPayment, LoanDraftError> {
        switch payment {
        case .none:
            return .success(LoanPayment.none)
        case .create:
            return .success(.create(target))
        case .link:
            guard let linkedRecurringId else {
                return .failure(LoanDraftError(field: .payment, message: LoanText.linkMissing))
            }
            return .success(.link(linkedRecurringId))
        }
    }

    fileprivate static func isBlank(_ text: String) -> Bool {
        text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}

/// The «Aggiungi evento» form as typed.
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

    /// The event as typed, or the first field in error. With the loan's
    /// `terms`, the date is also held to what the core takes: a rate change
    /// from the first installment to the last one the terms set, an early
    /// repayment on or before that last one (before the first is fine: it
    /// lowers the amount borrowed).
    func event(currency: Currency = .eur, terms: LoanTerms? = nil) -> Result<LoanEvent, LoanDraftError> {
        LoanDraftError.catching { try readEvent(currency: currency, terms: terms) }
    }

    private func readEvent(currency: Currency, terms: LoanTerms?) throws -> LoanEvent {
        let day = CoreDate.day(date)
        if let terms, let last = LoanSchedules.lastNominalDue(terms) {
            switch kind {
            case .rateChange where day < terms.firstDue || day > last:
                throw LoanDraftError(field: .date, message: LoanText.rateChangeDate)
            case .prepayment where day > last:
                throw LoanDraftError(field: .date, message: LoanText.prepaymentDate)
            default:
                break
            }
        }
        switch kind {
        case .rateChange:
            guard !LoanDraft.isBlank(rate) else { throw LoanDraftError(field: .rate, message: LoanText.newRateMissing) }
            guard let ppm = LoanRate.ppm(rate) else { throw LoanDraftError(field: .rate, message: LoanText.rateUnreadable) }
            guard ppm <= LoanRate.maxPpm else { throw LoanDraftError(field: .rate, message: LoanText.rateRange) }
            let installment: LoanInstallment
            switch installmentMode {
            case .computed:
                installment = .computed
            case .contract:
                installment = .contract(amount: try LoanMoney.read(
                    self.installment,
                    field: .installment,
                    currency: currency,
                    blank: LoanText.letterInstallmentMissing,
                    rule: .positive
                ))
            }
            return .rateChange(date: day, annualRatePpm: ppm, installment: installment)
        case .prepayment:
            let repaid = try LoanMoney.read(
                amount,
                field: .amount,
                currency: currency,
                blank: LoanText.prepaymentMissing,
                rule: .positive
            )
            let penalty = try LoanMoney.read(fee, field: .fee, currency: currency, blank: nil, rule: .notNegative)
            return .prepayment(date: day, amount: repaid, mode: mode, fee: penalty)
        }
    }
}

extension LoanDraftError {
    /// `body`'s value, or the field error it threw.
    fileprivate static func catching<T>(_ body: () throws -> T) -> Result<T, LoanDraftError> {
        do {
            return .success(try body())
        } catch let error as LoanDraftError {
            return .failure(error)
        } catch {
            // Every throw above is a `LoanDraftError`; kept for the compiler.
            return .failure(LoanDraftError(field: .principal, message: error.localizedDescription))
        }
    }
}

/// The rates of a loan in millionths, the core's unit: 3,125% is 31_250, so
/// a percentage takes up to four decimals.
nonisolated enum LoanRate {
    /// The highest TAN a loan takes, 50% (`core/src/loan/mod.rs`).
    static let maxPpm: UInt32 = 500_000
    /// The longest loan, 50 years of monthly installments.
    static let maxMonths: UInt32 = 600

    /// `"3,125"`, `"3.125"` or `"3,125 %"` as millionths, whatever its size;
    /// `nil` for anything that is not a number with at most four decimals.
    /// Either separator is the decimal one: a rate has no thousands.
    static func ppm(_ text: String) -> UInt32? {
        var trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.hasSuffix("%") {
            trimmed.removeLast()
            trimmed = trimmed.trimmingCharacters(in: .whitespaces)
        }
        let parts = trimmed.replacingOccurrences(of: ",", with: ".").split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count <= 2 else { return nil }
        let whole = parts[0]
        let fraction = parts.count == 2 ? parts[1] : ""
        let digits: (Substring) -> Bool = { $0.allSatisfy { ("0"..."9").contains($0) } }
        guard !whole.isEmpty || !fraction.isEmpty, digits(whole), digits(fraction),
              whole.count <= 3, fraction.count <= 4,
              let units = UInt32(whole.isEmpty ? "0" : whole),
              let millionths = UInt32((fraction + "0000").prefix(4))
        else { return nil }
        return units * 10_000 + millionths
    }

    /// `31_250` → `"3,125"`, `30_000` → `"3"`: the decimal comma, no
    /// trailing zeros.
    static func text(_ ppm: UInt32) -> String {
        let units = ppm / 10_000
        var fraction = String(ppm % 10_000)
        guard fraction != "0" else { return "\(units)" }
        fraction = String(repeating: "0", count: 4 - fraction.count) + fraction
        while fraction.hasSuffix("0") { fraction.removeLast() }
        return "\(units),\(fraction)"
    }

    /// `"3,125%"`.
    static func label(_ ppm: UInt32) -> String {
        text(ppm) + "%"
    }
}

/// Money as the loan forms take it: `AllocationMoney.parse`, so
/// `200.000,00` reads with its thousands dots.
private enum LoanMoney {
    enum Rule {
        case positive
        case notNegative
    }

    /// The amount in `text`; `blank` is the error for an empty field, or
    /// with `nil` an empty field is zero.
    static func read(
        _ text: String,
        field: LoanDraftError.Field,
        currency: Currency,
        blank: String?,
        rule: Rule
    ) throws -> Int64 {
        if LoanDraft.isBlank(text) {
            if let blank { throw LoanDraftError(field: field, message: blank) }
            return 0
        }
        guard let value = AllocationMoney.amount(text, currency: currency) else {
            throw LoanDraftError(field: field, message: LoanText.amountUnreadable)
        }
        switch rule {
        case .positive where value <= 0:
            throw LoanDraftError(field: field, message: LoanText.amountNotPositive)
        case .notNegative where value < 0:
            throw LoanDraftError(field: field, message: LoanText.amountNegative)
        default:
            return value
        }
    }
}

/// Due dates as the core lays them out: monthly on the first one's day, the
/// month's last day when it is shorter, through `scheduleOccurrences`.
enum LoanSchedules {
    /// A monthly schedule on `firstDue`'s day, from `start` (`firstDue`
    /// unless given) to `end`.
    static func monthly(firstDue: NaiveDate, start: NaiveDate? = nil, end: NaiveDate?) -> Schedule? {
        guard let day = NaiveDay.components(firstDue)?.day else { return nil }
        return Schedule(frequency: .monthly(day: UInt8(day)), interval: 1, startDate: start ?? firstDue, endDate: end)
    }

    /// The last due date the terms set, events aside: `months` installments
    /// from `firstDue`.
    static func lastNominalDue(_ terms: LoanTerms) -> NaiveDate? {
        guard terms.months > 0, let schedule = monthly(firstDue: terms.firstDue, end: nil) else { return nil }
        return (try? scheduleOccurrences(schedule: schedule, from: terms.firstDue, limit: terms.months))?.last
    }
}

/// The forms' words, one place for the messages two forms share.
nonisolated enum LoanText {
    static var nameMissing: String { String(localized: "Name the loan") }
    static var principalMissing: String { String(localized: "Type the amount borrowed") }
    static var amountUnreadable: String { String(localized: "Not an amount: type it like 1.234,56") }
    static var amountNotPositive: String { String(localized: "The amount must be above zero") }
    static var amountNegative: String { String(localized: "Fees cannot be below zero") }
    static var rateUnreadable: String { String(localized: "Type the rate in percent, like 3,125") }
    static var rateRange: String { String(localized: "A rate goes from 0 to 50%") }
    static var rateMissing: String { String(localized: "Type the rate, or the contract's installment to work it out") }
    static var taegUnusable: String { String(localized: "The TAN cannot be worked out from this TAEG") }
    static var durationMissing: String { String(localized: "Type how long the loan lasts") }
    static var durationRange: String { String(localized: "A loan lasts from 1 month to 50 years") }
    static var installmentMissing: String { String(localized: "Type the installment of the contract") }
    static var installmentTooLow: String { String(localized: "This installment does not repay the amount in that time") }
    static var installmentTooHigh: String { String(localized: "This installment means a rate above 50%") }
    static var linkMissing: String { String(localized: "Pick the recurring expense to link") }
    static var rateChangeDate: String { String(localized: "A rate change falls between the first and the last installment") }
    static var prepaymentDate: String { String(localized: "An early repayment falls on or before the last installment") }
    static var newRateMissing: String { String(localized: "Type the new rate") }
    static var letterInstallmentMissing: String { String(localized: "Type the installment the bank's letter states") }
    static var prepaymentMissing: String { String(localized: "Type the amount repaid") }
}
