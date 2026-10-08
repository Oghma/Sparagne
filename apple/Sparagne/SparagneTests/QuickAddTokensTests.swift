import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The chips under the ⌘K field (`QuickAddTokens.make`): one per field the
/// parsed line will write.
struct QuickAddTokensTests {
    private static func tokens(_ parsed: QuickAdd, person: String? = nil, today: Date = Date()) -> [QuickAddToken] {
        QuickAddTokens.make(parsed, currency: .eur, today: today, person: person)
    }

    private static func value(_ role: QuickAddToken.Role, in tokens: [QuickAddToken]) -> String? {
        tokens.first { $0.role == role }?.value
    }

    @Test("An entry reads kind, amount, category, envelope and wallet as said, and today")
    func entry() {
        let parsed = QuickAdd.entry(
            kind: .expense, amount: 1_250, note: "pizza", category: "Ristoranti", wallet: "Conto", flow: "Cash", date: nil
        )
        let tokens = Self.tokens(parsed)
        #expect(tokens.map(\.role) == [.kind, .amount, .category, .envelope, .wallet, .when])
        #expect(tokens.map(\.value) == [
            String(localized: "Expense"), "12,50 €", "Ristoranti", "Cash", "Conto", String(localized: "today"),
        ])
        #expect(tokens.allSatisfy { !$0.isDefault })
        #expect(tokens[0].label == nil && tokens[1].label == nil)
        #expect(tokens[2].label == String(localized: "category"))
    }

    @Test("Each kind has its own name, in the singular, and a transfer shows its two ends")
    func kinds() {
        let income = Self.tokens(.entry(kind: .income, amount: 100, note: nil, category: "Stipendio", wallet: nil, flow: nil, date: nil))
        #expect(Self.value(.kind, in: income) == String(localized: "Income entry"))
        let refund = Self.tokens(.entry(kind: .refund, amount: 100, note: nil, category: "Salute", wallet: nil, flow: nil, date: nil))
        #expect(Self.value(.kind, in: refund) == String(localized: "Refund"))

        let wallets = Self.tokens(.transferWallet(amount: 5_000, from: "Conto", to: "Cash", note: nil, date: nil))
        #expect(wallets.map(\.role) == [.kind, .amount, .wallet, .when])
        #expect(Self.value(.kind, in: wallets) == String(localized: "Transfer"))
        #expect(Self.value(.wallet, in: wallets) == "Conto \u{2192} Cash")

        let envelopes = Self.tokens(.transferFlow(amount: 5_000, from: "Cash", to: "Vacanze", note: nil, date: nil))
        #expect(envelopes.map(\.role) == [.kind, .amount, .envelope, .when])
        #expect(Self.value(.envelope, in: envelopes) == "Cash \u{2192} Vacanze")
    }

    @Test("A line with no category says Uncategorized, marked as the default rather than a choice")
    func missingCategory() throws {
        let tokens = Self.tokens(.entry(kind: .expense, amount: 300, note: "coop", category: nil, wallet: nil, flow: nil, date: nil))
        let category = try #require(tokens.first { $0.role == .category })
        #expect(category.value == String(localized: "Uncategorized"))
        #expect(category.isDefault)
        // No envelope or wallet on the line: the sticky defaults apply, and
        // no chip pretends to know which.
        #expect(!tokens.contains { $0.role == .envelope || $0.role == .wallet })
    }

    @Test("A date other than today is written as the day it resolves to")
    func otherDate() throws {
        let today = try #require(Calendar.current.date(from: DateComponents(year: 2026, month: 10, day: 7, hour: 12)))
        let threeDaysAgo = try #require(Calendar.current.date(byAdding: .day, value: -3, to: today))
        let parsed = QuickAdd.entry(kind: .expense, amount: 300, note: nil, category: nil, wallet: nil, flow: nil, date: .daysAgo(3))
        #expect(Self.value(.when, in: Self.tokens(parsed, today: today)) == LedgerDate.day(threeDaysAgo))

        let explicitToday = QuickAdd.entry(kind: .expense, amount: 300, note: nil, category: nil, wallet: nil, flow: nil, date: .today)
        #expect(Self.value(.when, in: Self.tokens(explicitToday, today: today)) == String(localized: "today"))
    }

    @Test("The who chip is the line's !name, or the name it resolves to, last")
    func personFromTheLine() {
        let parsed = QuickAdd.entry(
            kind: .expense, amount: 2_400, note: "cena", category: "Ristoranti", wallet: nil, flow: nil, date: nil,
            person: "eli"
        )
        let typed = Self.tokens(parsed)
        #expect(typed.last?.role == .who)
        #expect(typed.last?.label == String(localized: "who"))
        #expect(Self.value(.who, in: typed) == "eli")
        #expect(Self.value(.who, in: Self.tokens(parsed, person: "elisa")) == "elisa")
    }

    @Test("A person shows only when the line names one")
    func person() {
        let parsed = QuickAdd.entry(kind: .expense, amount: 300, note: nil, category: "Bar", wallet: nil, flow: nil, date: nil)
        #expect(!Self.tokens(parsed).contains { $0.role == .who })
        #expect(Self.value(.who, in: Self.tokens(parsed, person: "Matteo")) == "Matteo")
        #expect(!Self.tokens(parsed, person: "  ").contains { $0.role == .who })
    }
}
