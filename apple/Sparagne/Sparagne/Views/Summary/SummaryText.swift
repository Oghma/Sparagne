import Foundation

/// Small pure pieces of text the RIEPILOGO shares between its parts.
enum SummaryText {
    /// `"ottobre"`, `"October"`: the locale's own casing. The ledger's
    /// `LedgerDate.fullMonth` shouts for the grid's headings; running text
    /// ("fino a ottobre", "a settembre") reads wrong shouted
    /// (`docs/v2/UI.md` §5: sentence case).
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
}
