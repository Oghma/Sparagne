import Testing
import Foundation
@testable import Sparagne

struct MoneyFormatterTests {
    @Test func positiveAmountItalianLocale() {
        let formatted = MoneyFormatter.format(minorUnits: 1050, currencyCode: "EUR", locale: Locale(identifier: "it_IT"))
        #expect(formatted.contains("10,50"))
    }

    @Test func positiveAmountUSLocale() {
        let formatted = MoneyFormatter.format(minorUnits: 1050, currencyCode: "EUR", locale: Locale(identifier: "en_US"))
        #expect(formatted.contains("10.50"))
    }

    @Test func negativeAmountKeepsMinusSign() {
        let formatted = MoneyFormatter.format(minorUnits: -1050, currencyCode: "EUR", locale: Locale(identifier: "en_US"))
        #expect(formatted.contains("-"))
        #expect(formatted.contains("10.50"))
    }

    @Test func zeroAmount() {
        let formatted = MoneyFormatter.format(minorUnits: 0, currencyCode: "EUR", locale: Locale(identifier: "en_US"))
        #expect(formatted.contains("0.00"))
    }

    @Test func zeroAmountItalianLocale() {
        let formatted = MoneyFormatter.format(minorUnits: 0, currencyCode: "EUR", locale: Locale(identifier: "it_IT"))
        #expect(formatted.contains("0,00"))
    }

}

struct DateFormattingTests {
    @Test func today() {
        let label = DateFormatting.relativeDay(Date(), locale: Locale(identifier: "en_US"))
        #expect(label == "Today")
    }

    @Test func yesterday() {
        let calendar = Calendar.current
        let now = Date()
        let yesterday = calendar.date(byAdding: .day, value: -1, to: now)!
        let label = DateFormatting.relativeDay(yesterday, now: now, locale: Locale(identifier: "en_US"))
        #expect(label == "Yesterday")
    }

    @Test func olderDateFallsBackToFormattedDate() {
        let calendar = Calendar.current
        let now = Date()
        let tenDaysAgo = calendar.date(byAdding: .day, value: -10, to: now)!
        let label = DateFormatting.relativeDay(tenDaysAgo, now: now, locale: Locale(identifier: "en_US"))
        #expect(label != "Today")
        #expect(label != "Yesterday")
        #expect(!label.isEmpty)
    }
}
