import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The «Nuovo prestito» form as typed (`LoanDraft`): what each cell reads as,
/// the field each mistake lands on, and a saved loan read back into the form.
/// A plain struct, so none of this needs a vault; the TAEG and the contract's
/// installment go through the core's loan math.
struct LoanDraftTests {
    private static func day(_ text: NaiveDate) throws -> Date {
        try #require(CoreDate.localDay(text))
    }

    /// 200.000,00 at 3,125% for 25 years from 5 March 2026, no fees.
    private static func mortgage() throws -> LoanDraft {
        var draft = LoanDraft(firstDue: try day("2026-03-05"))
        draft.name = "Mutuo"
        draft.principal = "200.000,00"
        draft.rate = "3,125"
        draft.duration = "25"
        return draft
    }

    private static func terms(_ draft: LoanDraft) throws -> LoanTerms {
        try draft.terms().get()
    }

    /// The field of the error the draft gives, `nil` when it gives terms.
    private static func field(_ draft: LoanDraft) -> LoanDraftError.Field? {
        guard case .failure(let error) = draft.terms() else { return nil }
        #expect(!error.message.isEmpty)
        return error.field
    }

    // MARK: - Rates

    @Test("A rate reads in percent with either decimal separator, up to four decimals, as millionths")
    func rateParses() {
        #expect(LoanRate.ppm("3") == 30_000)
        #expect(LoanRate.ppm("3,125") == 31_250)
        #expect(LoanRate.ppm("3.125") == 31_250)
        #expect(LoanRate.ppm(" 3,125 % ") == 31_250)
        #expect(LoanRate.ppm("0,0001") == 1)
        #expect(LoanRate.ppm(",5") == 5_000)
        #expect(LoanRate.ppm("0") == 0)
        #expect(LoanRate.ppm("50") == 500_000)
        for garbage in ["", " ", "%", "abc", "3,12345", "1,2,3", "-1", "+1", "1000", "1e3", ".", "3,1,"] {
            #expect(LoanRate.ppm(garbage) == nil, "\(garbage)")
        }
    }

    @Test("A rate is written with the decimal comma and no trailing zeros, and reads back the same")
    func rateText() {
        #expect(LoanRate.text(31_250) == "3,125")
        #expect(LoanRate.text(30_000) == "3")
        #expect(LoanRate.text(5) == "0,0005")
        #expect(LoanRate.text(0) == "0")
        #expect(LoanRate.text(75_500) == "7,55")
        #expect(LoanRate.label(31_250) == "3,125%")
        for ppm: UInt32 in [0, 1, 50, 999, 31_250, 75_000, 123_456, 500_000] {
            #expect(LoanRate.ppm(LoanRate.text(ppm)) == ppm)
        }
    }

    // MARK: - Terms

    @Test("The cells read as terms: the amount with its thousands dots, the TAN in millionths, years in months, the first due as its day")
    func termsParse() throws {
        let terms = try Self.terms(try Self.mortgage())
        #expect(terms == LoanTerms(
            principal: 20_000_000,
            annualRatePpm: 31_250,
            months: 300,
            firstDue: "2026-03-05",
            installment: .computed,
            upfrontFees: 0,
            installmentFee: 0
        ))
    }

    @Test("Months as typed, fees with or without their dots, a contract's installment beside a rate typed")
    func termsInMonthsWithFees() throws {
        var draft = try Self.mortgage()
        draft.duration = "18"
        draft.durationUnit = .months
        draft.upfrontFees = "1.500,00"
        draft.installmentFee = "2,50"
        draft.installmentMode = .contract
        draft.installment = "11.200"
        let terms = try Self.terms(draft)
        #expect(terms.months == 18)
        #expect(terms.upfrontFees == 150_000)
        #expect(terms.installmentFee == 250)
        #expect(terms.installment == .contract(amount: 1_120_000))
        // A rate typed is the TAN: nothing is solved.
        #expect(terms.annualRatePpm == 31_250)
        #expect(!draft.solvesRate)
        #expect(!draft.rateIsEstimate)
    }

    @Test("A zero rate is a loan without interest")
    func zeroRate() throws {
        var draft = try Self.mortgage()
        draft.rate = "0"
        #expect(try Self.terms(draft).annualRatePpm == 0)
    }

    @Test("The name is not a term: the preview runs before it is typed, and loanName refuses it blank")
    func name() throws {
        var draft = try Self.mortgage()
        draft.name = "   "
        #expect(throws: Never.self) { try draft.terms().get() }
        #expect(draft.loanName() == .failure(LoanDraftError(field: .name, message: LoanText.nameMissing)))
        draft.name = "  Mutuo casa "
        #expect(draft.loanName() == .success("Mutuo casa"))
        draft.note = "  "
        #expect(draft.noteText == nil)
        draft.note = " tasso variabile "
        #expect(draft.noteText == "tasso variabile")
    }

    // MARK: - Every field's error

    @Test("The amount borrowed: blank, not an amount, zero or below")
    func principalErrors() throws {
        var draft = try Self.mortgage()
        for text in ["", "abc", "0", "-5", "0,00"] {
            draft.principal = text
            #expect(Self.field(draft) == .principal, "\(text)")
        }
    }

    @Test("The rate: not a number, above 50%, or blank with the installment worked out")
    func rateErrors() throws {
        var draft = try Self.mortgage()
        for text in ["abc", "3,12345", "50,0001", "60", ""] {
            draft.rate = text
            #expect(Self.field(draft) == .rate, "\(text)")
        }
        draft.rate = "50"
        #expect(Self.field(draft) == nil)
    }

    @Test("The duration: blank, not whole, zero, past 50 years or 600 months")
    func durationErrors() throws {
        var draft = try Self.mortgage()
        for text in ["", "2,5", "abc", "0", "51", "-1", "99999"] {
            draft.duration = text
            #expect(Self.field(draft) == .duration, "\(text)")
        }
        draft.duration = "50"
        #expect(try Self.terms(draft).months == 600)
        draft.durationUnit = .months
        draft.duration = "601"
        #expect(Self.field(draft) == .duration)
        draft.duration = "1"
        #expect(try Self.terms(draft).months == 1)
    }

    @Test("The contract's installment: blank, not an amount, zero")
    func installmentErrors() throws {
        var draft = try Self.mortgage()
        draft.installmentMode = .contract
        for text in ["", "abc", "0"] {
            draft.installment = text
            #expect(Self.field(draft) == .installment, "\(text)")
        }
        // The installment typed in computed mode is not read at all.
        draft.installmentMode = .computed
        draft.installment = "abc"
        #expect(Self.field(draft) == nil)
    }

    @Test("Fees: blank is zero, below zero or not an amount is refused on its own field")
    func feeErrors() throws {
        var draft = try Self.mortgage()
        draft.upfrontFees = "-1"
        #expect(Self.field(draft) == .upfrontFees)
        draft.upfrontFees = "abc"
        #expect(Self.field(draft) == .upfrontFees)
        draft.upfrontFees = " "
        draft.installmentFee = "-0,50"
        #expect(Self.field(draft) == .installmentFee)
        draft.installmentFee = "0"
        #expect(try Self.terms(draft).installmentFee == 0)
    }

    @Test("The first field in error is the one reported, in the form's order")
    func firstFieldWins() throws {
        var draft = try Self.mortgage()
        draft.principal = ""
        draft.rate = "abc"
        draft.duration = ""
        draft.upfrontFees = "-1"
        #expect(Self.field(draft) == .principal)
        draft.principal = "1.000"
        #expect(Self.field(draft) == .rate)
        draft.rate = "3"
        #expect(Self.field(draft) == .duration)
        draft.duration = "10"
        #expect(Self.field(draft) == .upfrontFees)
    }

    // MARK: - The installments

    @Test("Each payment choice asks for what it says; a link needs its template")
    func payment() throws {
        var draft = try Self.mortgage()
        draft.target = LoanLedgerTarget(walletId: "wallet", flowId: "casa", category: "Mutuo", owner: "elisa")
        #expect(draft.paymentRequest() == .success(.create(draft.target)))
        draft.payment = .none
        #expect(draft.paymentRequest() == .success(LoanPayment.none))
        draft.payment = .link
        #expect(draft.paymentRequest() == .failure(LoanDraftError(field: .payment, message: LoanText.linkMissing)))
        draft.linkedRecurringId = "template"
        #expect(draft.paymentRequest() == .success(.link("template")))
    }

    // MARK: - Editing a saved loan

    private static func loan(_ terms: LoanTerms, recurringId: Uuid? = nil) -> LoanView {
        LoanView(
            id: "loan",
            name: "Cucina",
            note: "Tasso fisso",
            terms: terms,
            events: [],
            recurringId: recurringId,
            archived: false,
            createdBy: "matteo"
        )
    }

    @Test("A saved loan reads back into the form as typed, in years when they are whole, and gives the same terms")
    func editRoundTrips() throws {
        let terms = try Self.terms(try Self.mortgage())
        let draft = LoanDraft(loan: Self.loan(terms))
        #expect(draft.name == "Cucina")
        #expect(draft.note == "Tasso fisso")
        #expect(draft.principal == "200.000,00")
        #expect(draft.rate == "3,125")
        #expect(draft.rateKind == .tan)
        #expect(draft.duration == "25")
        #expect(draft.durationUnit == .years)
        #expect(draft.installmentMode == .computed)
        #expect(draft.upfrontFees.isEmpty)
        #expect(draft.installmentFee.isEmpty)
        #expect(draft.payment == .none)
        #expect(draft.linkedRecurringId == nil)
        #expect(CoreDate.day(draft.firstDue) == "2026-03-05")
        #expect(draft.terms() == .success(terms))
    }

    @Test("A loan in months, with the contract's installment and fees, linked, reads back the same")
    func editWithContractRoundTrips() throws {
        let terms = LoanTerms(
            principal: 1_234_567,
            annualRatePpm: 75_500,
            months: 18,
            firstDue: "2026-01-31",
            installment: .contract(amount: 72_150),
            upfrontFees: 35_000,
            installmentFee: 150
        )
        let draft = LoanDraft(loan: Self.loan(terms, recurringId: "template"))
        #expect(draft.principal == "12.345,67")
        #expect(draft.rate == "7,55")
        #expect(draft.duration == "18")
        #expect(draft.durationUnit == .months)
        #expect(draft.installmentMode == .contract)
        #expect(draft.installment == "721,50")
        #expect(draft.upfrontFees == "350,00")
        #expect(draft.installmentFee == "1,50")
        #expect(draft.payment == .link)
        #expect(draft.linkedRecurringId == "template")
        #expect(draft.terms() == .success(terms))
    }

    // MARK: - Through the core's math

    @Test("The reference installments: 200.000 at 3% in 300 months is 948,42, 10.000 at 7,5% in 60 is 200,38")
    func referenceInstallments() throws {
        #expect(try loanInstallment(principal: 20_000_000, annualRatePpm: 30_000, months: 300) == 94_842)
        #expect(try loanInstallment(principal: 1_000_000, annualRatePpm: 75_000, months: 60) == 20_038)
    }

    @Test("With a TAEG the TAN is worked out from it, flagged as an estimate")
    func taeg() throws {
        var draft = try Self.mortgage()
        draft.rateKind = .taeg
        // 7,5% TAN monthly is 7,7633% a year.
        draft.rate = "7,7633"
        #expect(draft.rateIsEstimate)
        let tan = try Self.terms(draft).annualRatePpm
        #expect((74_999...75_001).contains(tan), "\(tan)")
        // A TAEG whose TAN is past 50% is refused on the rate.
        draft.rate = "70"
        #expect(Self.field(draft) == .rate)
    }

    @Test("With the contract's installment and no rate, the TAN is the one at which that installment repays the amount")
    func contractSolvesRate() throws {
        var draft = try Self.mortgage()
        draft.rate = ""
        draft.duration = "300"
        draft.durationUnit = .months
        draft.installmentMode = .contract
        draft.installment = "948,42"
        #expect(draft.solvesRate)
        let terms = try Self.terms(draft)
        #expect(terms.installment == .contract(amount: 94_842))
        #expect((29_995...30_005).contains(terms.annualRatePpm), "\(terms.annualRatePpm)")
        #expect(try loanInstallment(principal: terms.principal, annualRatePpm: terms.annualRatePpm, months: 300) == 94_842)
    }

    @Test("A contract's installment that never repays the amount, or only above 50%, is refused on the installment")
    func contractCannotSolve() throws {
        var draft = try Self.mortgage()
        draft.rate = ""
        draft.principal = "10.000"
        draft.duration = "12"
        draft.durationUnit = .months
        draft.installmentMode = .contract
        draft.installment = "100"
        #expect(Self.field(draft) == .installment)
        // 12 × 1.500 on 10.000 is a rate far above 50%.
        draft.installment = "1.500"
        #expect(Self.field(draft) == .installment)
    }
}
