import Foundation

/// Best-effort, UI-only preview of the quick-add grammar
/// (docs/v2/DISTILLATO_V1.md §3.1: `[+|-|r] importo  [nota…]  [#categoria]
/// [@wallet]  [>busta]`). Real parsing and name resolution will happen in
/// the Rust core (`quick_add::parse` / `Core::resolve_quick_add`); this only
/// drives the live preview line while the field is being typed, so it is
/// deliberately permissive and never validates wallet/envelope/category names.
enum QuickAddPreview {
    private enum Kind {
        case expense
        case income
        case refund
    }

    /// `"▼ 15.00 EUR │ pizza │ #food │ >groceries │ @cash │ Today"`, or `nil`
    /// while the first token isn't a parseable amount yet.
    static func describe(_ input: String) -> String? {
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }

        var tokens = trimmed.split(separator: " ").map(String.init)
        guard let amountToken = tokens.first, let (kind, minorAbs) = parseAmount(amountToken) else { return nil }
        tokens.removeFirst()

        var category: String?
        var wallet: String?
        var envelope: String?
        var noteWords: [String] = []

        for token in tokens {
            if token.count > 1, let marker = token.first, "#@>".contains(marker) {
                let value = String(token.dropFirst())
                switch marker {
                case "#": category = value
                case "@": wallet = value
                case ">": envelope = value
                default: break
                }
            } else {
                noteWords.append(token)
            }
        }

        var parts = ["\(arrow(for: kind)) \(plainAmount(minorAbs))"]
        if !noteWords.isEmpty { parts.append(noteWords.joined(separator: " ")) }
        if let category { parts.append("#\(category)") }
        if let envelope { parts.append(">\(envelope)") }
        if let wallet { parts.append("@\(wallet)") }
        parts.append(String(localized: "Today"))

        return parts.joined(separator: " │ ")
    }

    private static func arrow(for kind: Kind) -> String {
        switch kind {
        case .expense: "▼"
        case .income: "▲"
        case .refund: "↺"
        }
    }

    /// `1234` minor units -> `"12.34 EUR"`, independent of locale (matches
    /// the grammar's own notation, not the system currency format).
    private static func plainAmount(_ minorAbs: Int64) -> String {
        let major = minorAbs / 100
        let minor = minorAbs % 100
        return String(format: "%d.%02d %@", major, minor, SampleData.currencyCode)
    }

    private static func parseAmount(_ token: String) -> (Kind, Int64)? {
        var text = token
        var kind: Kind = .expense

        if text.hasPrefix("r") {
            kind = .refund
            text.removeFirst()
        } else if text.hasPrefix("+") {
            kind = .income
            text.removeFirst()
        } else if text.hasPrefix("-") {
            kind = .expense
            text.removeFirst()
        }

        guard !text.isEmpty else { return nil }
        let normalized = text.replacingOccurrences(of: ",", with: ".")
        guard let value = Decimal(string: normalized), value >= 0 else { return nil }

        let minorDecimal = value * 100
        let minorAbs = Int64(truncating: NSDecimalNumber(decimal: minorDecimal))
        return (kind, minorAbs)
    }
}
