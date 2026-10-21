import Foundation
import SparagneCore

/// One chip under the ⌘K field; `label` is the word in front, none for the kind and the amount.
struct QuickAddToken: Identifiable, Hashable, Sendable {
    enum Role: Hashable, Sendable {
        case kind, amount, category, envelope, wallet, when, who
    }

    let role: Role
    let label: String?
    let value: String
    /// A value the line did not say and the default fills in (Uncategorized
    /// when there is no `#category`): drawn dimmer, so it reads as a gap the
    /// ⇥ hint can fill rather than as a choice.
    var isDefault = false

    var id: Role { role }
}

/// The parsed line as chips, so a line that parsed into something unexpected shows before ↩.
enum QuickAddTokens {
    /// The "who" chip shows the `!name` of the line, and nothing when there
    /// is none: the row is then the author's, like any other. `person`, when
    /// given, is that name as it resolves (`eli` → Elisa), which is what the
    /// row will say.
    static func make(
        _ parsed: QuickAdd,
        currency: Currency,
        today: Date = Date(),
        person: String? = nil,
        timeZone: TimeZone = .current
    ) -> [QuickAddToken] {
        let kind: TransactionKind
        let amount: Int64
        let when: DateSpec?
        var fields: [QuickAddToken] = []
        var who = person
        switch parsed {
        case .entry(let entryKind, let entryAmount, _, let category, let wallet, let flow, let date, let named):
            (kind, amount, when) = (entryKind, entryAmount, date)
            who = nonEmpty(person) ?? named
            fields.append(
                QuickAddToken(
                    role: .category,
                    label: String(localized: "category"),
                    value: nonEmpty(category) ?? String(localized: "Uncategorized"),
                    isDefault: nonEmpty(category) == nil
                )
            )
            if let flow = nonEmpty(flow) {
                fields.append(QuickAddToken(role: .envelope, label: String(localized: "envelope"), value: flow))
            }
            if let wallet = nonEmpty(wallet) {
                fields.append(QuickAddToken(role: .wallet, label: String(localized: "wallet"), value: wallet))
            }
        case .transferWallet(let transferAmount, let from, let to, _, let date):
            (kind, amount, when) = (.transferWallet, transferAmount, date)
            fields = [QuickAddToken(role: .wallet, label: String(localized: "wallet"), value: "\(from) \u{2192} \(to)")]
        case .transferFlow(let transferAmount, let from, let to, _, let date):
            (kind, amount, when) = (.transferFlow, transferAmount, date)
            fields = [QuickAddToken(role: .envelope, label: String(localized: "envelope"), value: "\(from) \u{2192} \(to)")]
        }

        var tokens = [
            QuickAddToken(role: .kind, label: nil, value: kindName(kind)),
            QuickAddToken(role: .amount, label: nil, value: money(amount, currency)),
        ] + fields
        tokens.append(
            QuickAddToken(role: .when, label: String(localized: "when"), value: day(when, today: today, timeZone: timeZone))
        )
        if let who = nonEmpty(who) {
            tokens.append(QuickAddToken(role: .who, label: String(localized: "who"), value: who))
        }
        return tokens
    }

    /// The kind as one entry, singular: "Uscita", not the list's "Uscite".
    static func kindName(_ kind: TransactionKind) -> String {
        switch kind {
        case .expense: String(localized: "Expense")
        case .income: String(localized: "Income entry")
        case .refund: String(localized: "Refund")
        case .transferWallet, .transferFlow: String(localized: "Transfer")
        }
    }

    /// `"12,50 €"`: the grid's figures with the symbol after, as the canvas
    /// writes an amount that stands alone.
    private static func money(_ amount: Int64, _ currency: Currency) -> String {
        let symbol = switch currency {
        case .eur: "€"
        }
        return "\(LedgerMoney.bare(amount)) \(symbol)"
    }

    /// "today", or the day the date token resolves to (`"05 ott"`), through
    /// the core so the chip agrees with what will be written.
    private static func day(_ spec: DateSpec?, today: Date, timeZone: TimeZone) -> String {
        let todayKey = CoreDate.day(today, timeZone: timeZone)
        guard let spec,
              let resolved = try? resolveDateSpec(spec: spec, today: todayKey),
              resolved != todayKey,
              let date = CoreDate.localDay(resolved, timeZone: timeZone)
        else {
            return String(localized: "today")
        }
        var calendar = Calendar.current
        calendar.timeZone = timeZone
        return LedgerDate.day(date, calendar: calendar)
    }

    private static func nonEmpty(_ text: String?) -> String? {
        guard let trimmed = text?.trimmingCharacters(in: .whitespacesAndNewlines), !trimmed.isEmpty else { return nil }
        return trimmed
    }
}
