import Foundation
import SparagneCore

/// Small pure pieces of text the RIEPILOGO shares between its parts.
enum SummaryText {
    /// `"ottobre"`, `"October"`: the locale's own casing. The ledger's
    /// `LedgerDate.fullMonth` shouts for the grid's headings; running text
    /// ("fino a ottobre", "a settembre") reads wrong shouted
    /// (sentence case).
    static func monthName(_ month: Int, locale: Locale = .autoupdatingCurrent) -> String {
        var calendar = Calendar(identifier: .gregorian)
        calendar.locale = locale
        let symbols = calendar.standaloneMonthSymbols
        guard symbols.indices.contains(month - 1) else { return "\(month)" }
        return symbols[month - 1]
    }

    /// The same with a capital, for a title: `"Agosto 2026"`.
    static func monthTitle(_ month: Int, year: Int, locale: Locale = .autoupdatingCurrent) -> String {
        "\(capitalized(monthName(month, locale: locale))) \(year)"
    }

    /// Only the first letter, unlike `capitalized`, which would also lower
    /// the rest of a name.
    static func capitalized(_ text: String) -> String {
        text.prefix(1).uppercased() + text.dropFirst()
    }

    /// `"+9.140,06"`: a growth always carries its sign; `bare` already
    /// prints the minus.
    static func signed(_ text: String, positive: Bool) -> String {
        positive ? "+\(text)" : text
    }

    // MARK: - Shares

    /// `"38%"`: a fraction of one written as the cards write a share, the
    /// ledger's comma and no decimals unless asked (`LedgerMoney.percent`,
    /// which takes two amounts where the model hands a ratio).
    static func percent(_ fraction: Double, decimals: Int = 0) -> String {
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        formatter.decimalSeparator = ","
        formatter.groupingSeparator = "."
        formatter.minimumFractionDigits = decimals
        formatter.maximumFractionDigits = decimals
        return (formatter.string(from: NSNumber(value: fraction * 100)) ?? "0") + "%"
    }

    /// TASSO as the savings-rate card writes it (`LedgerMoney.savingsRate`):
    /// `"73%"`, `"-26%"`, and `"< -100%"` once more than twice the income
    /// went out, where the figure would only say how little came in. The
    /// model hands the ratio already worked out (`YearMonth.rate`,
    /// `PersonYear.rate`); `nil` without income, so the caller prints a
    /// dash.
    static func rate(_ rate: Double?) -> String? {
        guard let rate else { return nil }
        guard rate >= -1 else { return "< -100%" }
        return percent(rate)
    }

    // MARK: - Categories

    /// The core names its two system categories in English; the RIEPILOGO
    /// says them in the user's language, as the Mastro's panel does
    /// (`CategoryBreakdown.name`). The line that folds the lighter
    /// categories is "Other".
    static func categoryName(_ name: String, isSystem: Bool, isOther: Bool = false) -> String {
        if isOther { return String(localized: "Other") }
        guard isSystem else { return name }
        switch name.lowercased() {
        case "uncategorized": return String(localized: "Uncategorized")
        case "opening": return String(localized: "Opening")
        default: return name
        }
    }

    // MARK: - Things to do

    /// A chip of the strip under the heading: `"2 ricorrenze da decidere"`,
    /// `"Riparto in attesa · dom 27 set · 2 precedenti"`, `"14 righe senza
    /// categoria"`. The period is written as the Riparto's due card writes
    /// it (`RecurringDayText.weekday`).
    ///
    /// `bundle` is the catalog to read the plural keys from, as `CountText`
    /// takes it.
    static func todo(
        _ kind: SummaryTodoItem.Kind, bundle: Bundle = .main, locale: Locale = .autoupdatingCurrent
    ) -> String {
        switch kind {
        case .dueRecurring(let count):
            return String(localized: "\(count) recurring to decide", bundle: bundle, locale: locale)
        case .pendingAllocation(let period, let missed):
            let day = RecurringDayText.weekday(period, locale: locale)
            let due = String(localized: "Allocation due \u{00B7} \(day)", bundle: bundle, locale: locale)
            guard missed > 0 else { return due }
            return "\(due) \u{00B7} \(String(localized: "\(missed) earlier", bundle: bundle, locale: locale))"
        case .uncategorized(let count):
            return String(localized: "\(count) rows without a category", bundle: bundle, locale: locale)
        }
    }
}
