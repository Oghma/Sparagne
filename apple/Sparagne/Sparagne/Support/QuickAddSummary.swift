import Foundation
import SparagneCore

/// Renders a parsed quick-add line as the live preview the v1 TUI showed:
/// `▼ 15.00 EUR │ pizza │ #food │ >groceries │ @cash │ Today`
/// (docs/v2/DISTILLATO_V1.md §3.1).
///
/// Parsing itself belongs to the core (`parseQuickAdd`); this only formats
/// what came back.
enum QuickAddSummary {
    static func describe(_ parsed: QuickAdd, currency: Currency, today: Date = Date()) -> String {
        switch parsed {
        case .entry(let kind, let amount, let note, let category, let wallet, let flow, let date):
            var parts = ["\(marker(kind)) \(formatMoney(minor: amount, currency: currency))"]
            if let note, !note.isEmpty { parts.append(note) }
            if let category, !category.isEmpty { parts.append("#\(category)") }
            if let flow, !flow.isEmpty { parts.append(">\(flow)") }
            if let wallet, !wallet.isEmpty { parts.append("@\(wallet)") }
            parts.append(dateLabel(date, today: today))
            return parts.joined(separator: " │ ")

        case .transferWallet(let amount, let from, let to, let note, let date):
            return transfer(amount: amount, from: "@\(from)", to: "@\(to)", note: note, date: date, currency: currency, today: today)

        case .transferFlow(let amount, let from, let to, let note, let date):
            return transfer(amount: amount, from: ">\(from)", to: ">\(to)", note: note, date: date, currency: currency, today: today)
        }
    }

    private static func transfer(
        amount: Int64,
        from: String,
        to: String,
        note: String?,
        date: DateSpec?,
        currency: Currency,
        today: Date
    ) -> String {
        var parts = ["⇄ \(formatMoney(minor: amount, currency: currency))", "\(from) → \(to)"]
        if let note, !note.isEmpty { parts.append(note) }
        parts.append(dateLabel(date, today: today))
        return parts.joined(separator: " │ ")
    }

    private static func marker(_ kind: TransactionKind) -> String {
        switch kind {
        case .income: "▲"
        case .expense: "▼"
        case .refund: "↺"
        case .transferWallet, .transferFlow: "⇄"
        }
    }

    /// Resolves the relative date token against today, through the core, so
    /// the preview agrees with what will be written.
    private static func dateLabel(_ spec: DateSpec?, today: Date) -> String {
        guard let spec,
              let resolved = try? resolveDateSpec(spec: spec, today: CoreDate.day(today)),
              let date = CoreDate.localDay(resolved)
        else {
            return String(localized: "Today")
        }
        return DateFormatting.relativeDay(date, now: today)
    }
}
