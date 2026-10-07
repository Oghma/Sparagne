import Foundation

/// Pure formatting for the VoiceOver labels used across the RIEPILOGO
/// (`SummaryView.swift`) and the ledger's summary panel
/// (`Views/Ledger/SummaryPanel.swift`, `docs/v2/UI.md` §2.1-§2.2): kept out
/// of the views themselves so the text can be checked without a window.
///
/// Every piece handed in here (labels, amounts) is already localized or
/// formatted; joining them with punctuation needs no further localization,
/// the same way the summary panel's cards join their own parts. A phrase that
/// adds a real word of its own (`fundGauge`'s "of") goes through
/// `String(localized:)` instead.
enum AccessibilityText {
    /// One of the RIEPILOGO's four month cards: "Income · September, €1,234.56, 2 people".
    static func card(title: String, value: String, caption: String?) -> String {
        [title, value, caption].compactMap(\.self).joined(separator: ", ")
    }

    /// A fund gauge's `accessibilityValue`: "99.8%, €29,931.00 of €30,000.00".
    static func fundGauge(percent: String, filled: String, cap: String) -> String {
        String(localized: "\(percent), \(filled) of \(cap)")
    }

    /// One row of the RIEPILOGO's month table (or one point of a chart),
    /// read as a sentence: "September, Income €9,200.00, Expenses €3,100.00".
    static func monthRow(month: String, figures: [(String, String)]) -> String {
        ([month] + figures.map { "\($0.0) \($0.1)" }).joined(separator: ", ")
    }

    /// One figure of a person x row table: "Income, Elisa: €1,200.00", or
    /// "Income: €1,200.00" when the table has no person column to add.
    static func figure(_ label: String, person: String? = nil, amount: String) -> String {
        guard let person else { return "\(label): \(amount)" }
        return "\(label), \(person): \(amount)"
    }
}
