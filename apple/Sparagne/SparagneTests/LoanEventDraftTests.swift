import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The «Aggiungi evento» form as typed (`LoanEventDraft`): a rate change or
/// an early repayment, the field each mistake lands on, and the dates the
/// loan's terms allow.
struct LoanEventDraftTests {
    private static func day(_ text: NaiveDate) throws -> Date {
        try #require(CoreDate.localDay(text))
    }

    /// Twelve installments from 31 January 2026: the last one on 31 December,
    /// February's on the 28th.
    private static let terms = LoanTerms(
        principal: 1_000_000,
        annualRatePpm: 75_000,
        months: 12,
        firstDue: "2026-01-31",
        installment: .computed,
        upfrontFees: 0,
        installmentFee: 0
    )

    private static func rateChange(on date: NaiveDate = "2026-06-30") throws -> LoanEventDraft {
        var draft = LoanEventDraft(date: try day(date))
        draft.kind = .rateChange
        draft.rate = "4,5"
        return draft
    }

    private static func prepayment(on date: NaiveDate = "2026-06-30") throws -> LoanEventDraft {
        var draft = LoanEventDraft(date: try day(date))
        draft.amount = "2.000,00"
        draft.fee = "20"
        return draft
    }

    private static func field(_ draft: LoanEventDraft, terms: LoanTerms? = nil) -> LoanDraftError.Field? {
        guard case .failure(let error) = draft.event(terms: terms) else { return nil }
        #expect(!error.message.isEmpty)
        return error.field
    }

    // MARK: - A rate change

    @Test("A rate change reads its day and the new TAN, the installment worked out again or the bank's")
    func rateChange() throws {
        var draft = try Self.rateChange()
        #expect(draft.event() == .success(.rateChange(date: "2026-06-30", annualRatePpm: 45_000, installment: .computed)))
        draft.installmentMode = .contract
        draft.installment = "880,50"
        #expect(draft.event() == .success(.rateChange(date: "2026-06-30", annualRatePpm: 45_000, installment: .contract(amount: 88_050))))
        // What only a repayment asks for is not read.
        draft.amount = "abc"
        draft.fee = "-1"
        #expect(Self.field(draft) == nil)
    }

    @Test("A rate change's rate: blank, not a number, above 50%; the bank's installment: blank or zero")
    func rateChangeErrors() throws {
        var draft = try Self.rateChange()
        for text in ["", "abc", "4,12345", "51"] {
            draft.rate = text
            #expect(Self.field(draft) == .rate, "\(text)")
        }
        draft.rate = "0"
        #expect(Self.field(draft) == nil)
        draft.installmentMode = .contract
        for text in ["", "0", "abc"] {
            draft.installment = text
            #expect(Self.field(draft) == .installment, "\(text)")
        }
    }

    // MARK: - An early repayment

    @Test("An early repayment reads its amount, its mode and its penalty, a blank penalty being none")
    func prepayment() throws {
        var draft = try Self.prepayment()
        #expect(draft.recordsExpense)
        #expect(draft.event() == .success(.prepayment(date: "2026-06-30", amount: 200_000, mode: .shorten, fee: 2_000)))
        draft.mode = .lower
        draft.fee = " "
        #expect(draft.event() == .success(.prepayment(date: "2026-06-30", amount: 200_000, mode: .lower, fee: 0)))
        // A rate typed is not read.
        draft.rate = "abc"
        #expect(Self.field(draft) == nil)
    }

    @Test("An early repayment's amount: blank, not an amount, zero; its penalty: below zero or not an amount")
    func prepaymentErrors() throws {
        var draft = try Self.prepayment()
        for text in ["", "abc", "0"] {
            draft.amount = text
            #expect(Self.field(draft) == .amount, "\(text)")
        }
        draft.amount = "100"
        for text in ["-1", "abc"] {
            draft.fee = text
            #expect(Self.field(draft) == .fee, "\(text)")
        }
    }

    @Test("Given the debt left on the day, a repayment above it is refused on its amount, saying how much is left")
    func prepaymentAboveResidual() throws {
        let draft = try Self.prepayment()
        let refused = draft.event(residual: 150_000)
        #expect(refused == .failure(LoanDraftError(field: .amount, message: LoanText.aboveResidual(150_000))))
        #expect(LoanText.aboveResidual(150_000).hasSuffix("1.500,00"))
        // The whole debt is a payoff, not too much.
        #expect(draft.event(residual: 200_000) == .success(.prepayment(date: "2026-06-30", amount: 200_000, mode: .shorten, fee: 2_000)))
        // A rate change has no amount to hold to it.
        #expect(throws: Never.self) { try Self.rateChange().event(residual: 0).get() }
    }

    // MARK: - The dates the terms allow

    @Test("The last installment the terms set is the core's: monthly on the first one's day, clamped to short months")
    func lastNominalDue() {
        #expect(LoanSchedules.lastNominalDue(Self.terms) == "2026-12-31")
        var short = Self.terms
        short.months = 2
        #expect(LoanSchedules.lastNominalDue(short) == "2026-02-28")
        short.months = 1
        #expect(LoanSchedules.lastNominalDue(short) == "2026-01-31")
    }

    @Test("A rate change falls from the first installment to the last one")
    func rateChangeWindow() throws {
        #expect(Self.field(try Self.rateChange(on: "2026-01-30"), terms: Self.terms) == .date)
        #expect(Self.field(try Self.rateChange(on: "2026-01-31"), terms: Self.terms) == nil)
        #expect(Self.field(try Self.rateChange(on: "2026-12-31"), terms: Self.terms) == nil)
        #expect(Self.field(try Self.rateChange(on: "2027-01-01"), terms: Self.terms) == .date)
        // Without the terms the day is the core's to check.
        #expect(Self.field(try Self.rateChange(on: "2027-01-01")) == nil)
    }

    @Test("An early repayment may come before the first installment, never after the last")
    func prepaymentWindow() throws {
        #expect(Self.field(try Self.prepayment(on: "2025-12-01"), terms: Self.terms) == nil)
        #expect(Self.field(try Self.prepayment(on: "2026-12-31"), terms: Self.terms) == nil)
        #expect(Self.field(try Self.prepayment(on: "2027-01-01"), terms: Self.terms) == .date)
    }

    @Test("The date is checked first: a day outside the loan is the error, whatever else is wrong")
    func dateFirst() throws {
        var draft = try Self.prepayment(on: "2027-01-01")
        draft.amount = ""
        #expect(Self.field(draft, terms: Self.terms) == .date)
        #expect(Self.field(draft) == .amount)
    }
}
