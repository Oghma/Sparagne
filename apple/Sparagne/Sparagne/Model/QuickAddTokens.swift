import Foundation
import SparagneCore

/// One chip under the ⌘K field: what the line will write, one field at a time
/// (`docs/v2/UI.md` §6). `label` is the small word in front ("category",
/// "envelope", "when"); the kind and the amount speak for themselves and have
/// none.
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

/// The parsed line as chips: `Uscita · 12,50 € · categoria Ristoranti · busta
/// Cash · quando oggi`. The old one-line preview (`QuickAddSummary`) packed the
/// same fields behind sigils; the chips name them, so a line that parsed into
/// something unexpected is spotted before ↩.
///
/// Parsing belongs to the core (`parseQuickAdd`); this only lays out what came
/// back. Pure, so it is tested without a window
/// (`SparagneTests/QuickAddTokensTests.swift`).
enum QuickAddTokens {
    /// `person` is who the line names, when the grammar carries one; the
    /// quick-add grammar has no person marker today, so the panel passes
    /// `nil` and the row goes to the author like any other.
    static func make(
        _ parsed: QuickAdd,
        currency: Currency,
        today: Date = Date(),
        person: String? = nil,
        timeZone: TimeZone = .current
    ) -> [QuickAddToken] {
        var tokens: [QuickAddToken]
        let when: DateSpec?
        switch parsed {
        case .entry(let kind, let amount, _, let category, let wallet, let flow, let date, _):
            tokens = [
                QuickAddToken(role: .kind, label: nil, value: kindName(kind)),
                QuickAddToken(role: .amount, label: nil, value: money(amount, currency)),
            ]
            if let category = nonEmpty(category) {
                tokens.append(QuickAddToken(role: .category, label: String(localized: "category"), value: category))
            } else {
                tokens.append(
                    QuickAddToken(
                        role: .category,
                        label: String(localized: "category"),
                        value: String(localized: "Uncategorized"),
                        isDefault: true
                    )
                )
            }
            if let flow = nonEmpty(flow) {
                tokens.append(QuickAddToken(role: .envelope, label: String(localized: "envelope"), value: flow))
            }
            if let wallet = nonEmpty(wallet) {
                tokens.append(QuickAddToken(role: .wallet, label: String(localized: "wallet"), value: wallet))
            }
            when = date

        case .transferWallet(let amount, let from, let to, _, let date):
            tokens = [
                QuickAddToken(role: .kind, label: nil, value: String(localized: "Transfer")),
                QuickAddToken(role: .amount, label: nil, value: money(amount, currency)),
                QuickAddToken(role: .wallet, label: String(localized: "wallet"), value: "\(from) \u{2192} \(to)"),
            ]
            when = date

        case .transferFlow(let amount, let from, let to, _, let date):
            tokens = [
                QuickAddToken(role: .kind, label: nil, value: String(localized: "Transfer")),
                QuickAddToken(role: .amount, label: nil, value: money(amount, currency)),
                QuickAddToken(role: .envelope, label: String(localized: "envelope"), value: "\(from) \u{2192} \(to)"),
            ]
            when = date
        }

        tokens.append(
            QuickAddToken(role: .when, label: String(localized: "when"), value: day(when, today: today, timeZone: timeZone))
        )
        if let person = nonEmpty(person) {
            tokens.append(QuickAddToken(role: .who, label: String(localized: "who"), value: person))
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
