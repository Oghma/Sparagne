import Foundation
import SparagneCore

/// The Patrimonio's words and figures as they are written: rates, durations,
/// the warnings and the events. Kept out of the views so they can be checked
/// without a window.
enum WealthText {
    /// `"2,85%"`: a yearly rate in millionths, as the ledger writes numbers.
    static func rate(_ ppm: UInt32, decimals: Int = 2) -> String {
        LedgerMoney.number(Decimal(ppm) / 10_000 as NSDecimalNumber, decimals: decimals) + "%"
    }

    /// `"948,42 €"`: a figure of the sheets, where the canvas writes the
    /// currency after it.
    static func euro(_ minorUnits: Int64) -> String {
        "\(LedgerMoney.bare(minorUnits)) \u{20AC}"
    }

    /// `"feb 2049"`: the table's Fine.
    static func monthYear(_ day: NaiveDate, locale: Locale = .autoupdatingCurrent) -> String {
        guard let date = CoreDate.localDay(day, timeZone: CoreDate.utc) else { return day }
        var calendar = Calendar(identifier: .gregorian)
        calendar.locale = locale
        let style = Date.FormatStyle(locale: locale, calendar: calendar, timeZone: CoreDate.utc)
            .month(.abbreviated)
            .year()
        return date.formatted(style).replacingOccurrences(of: ".", with: "")
    }

    /// `"25 anni"`, or `"18 mesi"` when the months make no whole year.
    static func duration(months: UInt32) -> String {
        let months = Int(months)
        return months % 12 == 0
            ? String(localized: "\(months / 12) years")
            : String(localized: "\(months) months")
    }

    /// `"25 anni · 300 rate"`, or `"18 rate"` when the months make no whole
    /// year and the count says it all.
    static func durationWithCount(months: UInt32) -> String {
        let count = String(localized: "\(Int(months)) installments")
        return months % 12 == 0 ? "\(duration(months: months)) \u{00B7} \(count)" : count
    }

    /// The inspector's subtitle: `"Tasso variabile · 165.000,00 · 25 anni"`.
    static func summary(_ loan: LoanView) -> String {
        let kind = LoanFacts.isVariable(loan)
            ? String(localized: "Variable rate")
            : String(localized: "Fixed rate")
        return [kind, LedgerMoney.bare(loan.terms.principal), duration(months: loan.terms.months)]
            .joined(separator: " \u{00B7} ")
    }

    /// The Liquidità card's caption: `"Conto 43.806,23 · Contanti 360,00"`.
    static func wallets(_ wallets: [WealthFigures.Wallet]) -> String {
        wallets.map { "\($0.name) \(LedgerMoney.bare($0.balance))" }.joined(separator: " \u{00B7} ")
    }

    // MARK: - Warnings

    /// One issue as a sentence, for the table's icon and the inspector.
    static func issue(_ issue: LoanIssue) -> String {
        switch issue {
        case .schedule(let warning):
            return self.warning(warning)
        case .unregistered(let registered, let due):
            let missing = String(localized: "\(Int(due - registered)) installments not recorded in the ledger")
            return "\(missing): \(String(localized: "\(Int(registered)) of \(Int(due))"))"
        }
    }

    static func warning(_ warning: LoanWarning) -> String {
        switch warning {
        case .installmentBelowInterest(let number):
            String(localized: "From installment \(Int(number)) on, the installment does not cover the interest: the debt never goes down.")
        case .balloon(let amount):
            String(localized: "A balloon of \(LedgerMoney.bare(amount)) is left for the last installment.")
        case .prepaymentClipped:
            String(localized: "An early repayment was larger than the debt and was cut to it.")
        case .eventAfterEnd:
            String(localized: "An event falls after the end of the loan.")
        }
    }

    // MARK: - Events

    static func kind(_ event: LoanEvent) -> String {
        switch event {
        case .rateChange: String(localized: "Rate change")
        case .prepayment: String(localized: "Early repayment")
        }
    }

    /// Riduci la durata / Riduci la rata.
    static func mode(_ mode: PrepaymentMode) -> String {
        switch mode {
        case .shorten: String(localized: "Shorten the term")
        case .lower: String(localized: "Lower the installment")
        }
    }

    /// What an event changed, after its kind and date: `"3,25% → 2,85%"`, or
    /// `"10.000,00 · riduci la durata"`, with the penalty when there is one.
    static func detail(_ event: LoanEvent, previousRate: UInt32?) -> String {
        switch event {
        case .rateChange(_, let ppm, let installment):
            let change = previousRate.map { "\(rate($0)) \u{2192} \(rate(ppm))" } ?? rate(ppm)
            guard case .contract(let amount) = installment else { return change }
            return "\(change) \u{00B7} \(String(localized: "installment \(LedgerMoney.bare(amount))"))"
        case .prepayment(_, let amount, let mode, let fee):
            var parts = [LedgerMoney.bare(amount), self.mode(mode).lowercased()]
            if fee > 0 { parts.append(String(localized: "penalty \(LedgerMoney.bare(fee))")) }
            return parts.joined(separator: " \u{00B7} ")
        }
    }

    /// `"Cambio tasso · 1 gen 2026 · 3,25% → 2,85%"`: an event on one line.
    static func event(_ event: LoanEvent, previousRate: UInt32?) -> String {
        [kind(event), RecurringDayText.full(event.effectiveDate), detail(event, previousRate: previousRate)]
            .joined(separator: " \u{00B7} ")
    }

    // MARK: - Costs

    /// What the Costo totale counts, under it: the interest alone, the
    /// interest and the upfront fees, or the interest and every fee and
    /// penalty together.
    static func costNote(totalCost: Int64, totalInterest: Int64, upfrontFees: Int64) -> String {
        let extra = totalCost - totalInterest
        if extra == 0 { return String(localized: "Interest over the whole schedule.") }
        if extra == upfrontFees {
            return String(localized: "Interest over the whole schedule plus the upfront fees of \(LedgerMoney.bare(upfrontFees)).")
        }
        return String(localized: "Interest over the whole schedule plus \(LedgerMoney.bare(extra)) of fees and penalties.")
    }
}
