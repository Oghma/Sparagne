import Testing
import Foundation
import SparagneCore
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

// MARK: - Counts and schedules

/// A string catalog built on disk for one language, holding the plural forms
/// these tests need. The helpers read it through their `bundle` parameter, so
/// these tests check the keys and the plural rules whatever the app's own
/// catalog holds.
private enum TestCatalog {
    static func bundle(
        language: String,
        plurals: [String: (one: String, other: String)],
        strings: [String: String] = [:]
    ) throws -> Bundle {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "sparagne-catalog-\(UUID().uuidString).bundle", directoryHint: .isDirectory)
        let contents = root.appending(path: "Contents", directoryHint: .isDirectory)
        let lproj = contents.appending(path: "Resources/\(language).lproj", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: lproj, withIntermediateDirectories: true)
        let info: [String: Any] = [
            "CFBundleIdentifier": "it.oghma.sparagne.tests.catalog.\(language)",
            "CFBundleDevelopmentRegion": language,
            "CFBundlePackageType": "BNDL",
        ]
        try plist(info).write(to: contents.appending(path: "Info.plist"))
        var rules: [String: Any] = [:]
        for (key, forms) in plurals {
            rules[key] = [
                "NSStringLocalizedFormatKey": "%#@count@",
                "count": [
                    "NSStringFormatSpecTypeKey": "NSStringPluralRuleType",
                    "NSStringFormatValueTypeKey": "lld",
                    "one": forms.one,
                    "other": forms.other,
                ],
            ]
        }
        try plist(rules).write(to: lproj.appending(path: "Localizable.stringsdict"))
        try plist(strings).write(to: lproj.appending(path: "Localizable.strings"))
        return try #require(Bundle(url: root))
    }

    private static func plist(_ value: Any) throws -> Data {
        try PropertyListSerialization.data(fromPropertyList: value, format: .xml, options: 0)
    }

    static func english() throws -> Bundle {
        try bundle(language: "en", plurals: [
            "%lld to confirm": ("%lld to confirm", "%lld to confirm"),
            "%lld transactions deleted": ("Transaction deleted", "%lld transactions deleted"),
            "Every %lld days": ("Every day", "Every %lld days"),
            "Every %lld weeks": ("Every week", "Every %lld weeks"),
            "Every %lld months": ("Every month", "Every %lld months"),
            "Every %lld years": ("Every year", "Every %lld years"),
        ])
    }

    static func italian() throws -> Bundle {
        try bundle(
            language: "it",
            plurals: [
                "%lld to confirm": ("%lld da confermare", "%lld da confermare"),
                "%lld transactions deleted": ("Transazione eliminata", "%lld transazioni eliminate"),
                "Every %lld weeks": ("Ogni settimana", "Ogni %lld settimane"),
            ],
            strings: [
                "Every year on %@ %lld": "Ogni anno il %2$lld %1$@",
            ]
        )
    }
}

struct CountTextTests {
    private static let english = Locale(identifier: "en_US")
    private static let italian = Locale(identifier: "it_IT")

    @Test("A count reads as one or many in English, never \"1 rows\"")
    func englishCounts() throws {
        let catalog = try TestCatalog.english()
        #expect(CountText.toConfirm(1, bundle: catalog, locale: Self.english) == "1 to confirm")
        #expect(CountText.toConfirm(3, bundle: catalog, locale: Self.english) == "3 to confirm")
        #expect(CountText.voided(1, bundle: catalog, locale: Self.english) == "Transaction deleted")
        #expect(CountText.voided(3, bundle: catalog, locale: Self.english) == "3 transactions deleted")
    }

    @Test("The same counts in Italian")
    func italianCounts() throws {
        let catalog = try TestCatalog.italian()
        #expect(CountText.toConfirm(1, bundle: catalog, locale: Self.italian) == "1 da confermare")
        #expect(CountText.toConfirm(3, bundle: catalog, locale: Self.italian) == "3 da confermare")
        #expect(CountText.voided(1, bundle: catalog, locale: Self.italian) == "Transazione eliminata")
        #expect(CountText.voided(3, bundle: catalog, locale: Self.italian) == "3 transazioni eliminate")
    }
}

struct ScheduleFormattingTests {
    private static let english = Locale(identifier: "en_US")

    private static func schedule(_ frequency: Frequency, every interval: UInt32) -> Schedule {
        Schedule(frequency: frequency, interval: interval, startDate: "2026-01-01", endDate: nil)
    }

    @Test("The interval stepper names its unit, singular at one")
    func intervalWithItsUnit() throws {
        let catalog = try TestCatalog.english()
        #expect(ScheduleFormatting.every(1, .week, bundle: catalog, locale: Self.english) == "Every week")
        #expect(ScheduleFormatting.every(3, .week, bundle: catalog, locale: Self.english) == "Every 3 weeks")
        #expect(ScheduleFormatting.every(1, .day, bundle: catalog, locale: Self.english) == "Every day")
        #expect(ScheduleFormatting.every(3, .month, bundle: catalog, locale: Self.english) == "Every 3 months")
        #expect(ScheduleFormatting.every(3, .year, bundle: catalog, locale: Self.english) == "Every 3 years")

        let italian = try TestCatalog.italian()
        let locale = Locale(identifier: "it_IT")
        #expect(ScheduleFormatting.every(1, .week, bundle: italian, locale: locale) == "Ogni settimana")
        #expect(ScheduleFormatting.every(3, .week, bundle: italian, locale: locale) == "Ogni 3 settimane")
    }

    @Test("A schedule is described as one sentence, interval included")
    func describedSchedules() throws {
        let catalog = try TestCatalog.english()
        func describe(_ schedule: Schedule) -> String {
            ScheduleFormatting.describe(schedule, bundle: catalog, locale: Self.english)
        }
        #expect(describe(Self.schedule(.daily, every: 1)) == "Every day")
        #expect(describe(Self.schedule(.daily, every: 3)) == "Every 3 days")
        #expect(describe(Self.schedule(.weekly(weekday: 1), every: 1)) == "Every Monday")
        #expect(describe(Self.schedule(.weekly(weekday: 1), every: 3)) == "Every 3 weeks on Monday")
        #expect(describe(Self.schedule(.monthly(day: 15), every: 1)) == "Every month on day 15")
        #expect(describe(Self.schedule(.monthly(day: 15), every: 3)) == "Every 3 months on day 15")
        #expect(describe(Self.schedule(.yearly(month: 9, day: 5), every: 1)) == "Every year on September 5")
        // The yearly interval used to be dropped from the description.
        #expect(describe(Self.schedule(.yearly(month: 9, day: 5), every: 3)) == "Every 3 years on September 5")
    }

    @Test("A translation can put the day before the month")
    func italianOrder() throws {
        let catalog = try TestCatalog.italian()
        let text = ScheduleFormatting.describe(
            Self.schedule(.yearly(month: 9, day: 5), every: 1),
            bundle: catalog,
            locale: Locale(identifier: "it_IT")
        )
        #expect(text == "Ogni anno il 5 settembre")
    }
}

/// The month and weekday names the date labels are made of, one source for
/// all of them.
struct CalendarNameTests {
    private static let english = Locale(identifier: "en_US")
    private static let italian = Locale(identifier: "it_IT")

    @Test("A month is named in the locale's own case, in each style")
    func monthNames() {
        #expect(LedgerDate.monthName(9, style: .format, locale: Self.english) == "September")
        #expect(LedgerDate.monthName(9, style: .standalone, locale: Self.italian) == "settembre")
        #expect(LedgerDate.monthName(8, style: .shortStandalone, locale: Self.english) == "Aug")
        #expect(LedgerDate.shortMonth(8, locale: Self.italian) == "ago")
        #expect(LedgerDate.fullMonth(8, locale: Self.italian) == "AGOSTO")
        for month in 1...12 {
            #expect(LedgerDate.monthName(month, style: .format, locale: Self.italian)
                == ScheduleFormatting.monthName(UInt8(month), locale: Self.italian))
            #expect(LedgerDate.monthName(month, style: .standalone, locale: Self.italian)
                == SummaryText.monthName(month, locale: Self.italian))
        }
    }

    @Test("A weekday counts from Sunday, as Calendar does, and an ISO weekday maps onto it")
    func weekdayNames() {
        #expect(LedgerDate.weekdayName(1, style: .format, locale: Self.english) == "Sunday")
        #expect(LedgerDate.weekdayName(2, style: .format, locale: Self.italian) == "lunedì")
        #expect(LedgerDate.weekdayName(5, style: .shortStandalone, locale: Self.english) == "Thu")
        for iso in UInt8(1)...7 {
            #expect(LedgerDate.weekdayName(Int(iso) % 7 + 1, style: .format, locale: Self.italian)
                == ScheduleFormatting.weekdayName(iso, locale: Self.italian))
        }
    }

    @Test("A number out of range is written as the number")
    func outOfRange() {
        #expect(LedgerDate.monthName(13, style: .format, locale: Self.english) == "13")
        #expect(LedgerDate.monthName(0, style: .standalone, locale: Self.english) == "0")
        #expect(LedgerDate.weekdayName(8, style: .format, locale: Self.english) == "8")
        #expect(LedgerDate.shortMonth(13, locale: Self.english) == "13")
        #expect(LedgerDate.fullMonth(0, locale: Self.english) == "0")
    }
}

/// Savings as a share of income, on the summary card and the ledger's panel.
struct SavingsRateTests {
    @Test("A rate is a whole percent; with no income there is none, for a dash")
    func ordinaryRates() {
        #expect(LedgerMoney.savingsRate(7_300, of: 10_000) == "73%")
        #expect(LedgerMoney.savingsRate(-2_600, of: 10_000) == "-26%")
        #expect(LedgerMoney.savingsRate(-10_000, of: 10_000) == "-100%")
        #expect(LedgerMoney.savingsRate(-500, of: 0) == nil)
    }

    @Test("Below -100% the figure only says how little came in, so it reads as less than -100%")
    func tinyIncome() {
        // September: 0,07 € of interest against 276,22 € of expenses.
        #expect(LedgerMoney.savingsRate(-27_615, of: 7) == "< -100%")
        #expect(LedgerMoney.savingsRate(-10_001, of: 10_000) == "< -100%")
    }
}
