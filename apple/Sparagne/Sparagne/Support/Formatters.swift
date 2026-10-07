import Foundation
import SparagneCore

/// Formats `Int64` minor units (e.g. cents) as currency. Amounts are never
/// floats anywhere in the app (docs/v2/DISTILLATO_V1.md §1.4); this is the
/// only place minor units get converted to displayable text, always through
/// `Decimal`.
///
/// Note: this assumes 2 minor-unit decimal places (true for EUR, the only
/// currency v2.0 supports per docs/v2/ARCH.md §7). A future multi-currency
/// core would need to expose the exponent per currency instead of assuming it.
enum MoneyFormatter {
    private static let minorUnitsPerMajor: Int64 = 100

    /// `"1 234,56 €"` (locale-formatted, unsigned display of the sign already in the number).
    static func format(minorUnits: Int64, currencyCode: String, locale: Locale = .autoupdatingCurrent) -> String {
        decimalValue(minorUnits).formatted(.currency(code: currencyCode).locale(locale))
    }

    /// `1250` -> `"12.50"`: the plain major-unit text `parseMoney` accepts,
    /// with no sign, grouping separator or currency code.
    static func editable(minorUnits: Int64) -> String {
        let absolute = minorUnits.magnitude
        return String(format: "%llu.%02llu", absolute / 100, absolute % 100)
    }

    private static func decimalValue(_ minorUnits: Int64) -> Decimal {
        Decimal(minorUnits) / Decimal(minorUnitsPerMajor)
    }
}

/// Formats dates the way the transactions table wants them: "Today"/
/// "Yesterday" for the last two days, a locale-formatted date otherwise
/// (docs/v2/DISTILLATO_V1.md §3.4 "Today / Yesterday / date").
enum DateFormatting {
    /// `"Today"`, `"Yesterday"`, or a locale-formatted date (e.g. `"Sep 9, 2026"`).
    static func relativeDay(_ date: Date, now: Date = Date(), calendar: Calendar = .current, locale: Locale = .autoupdatingCurrent) -> String {
        if calendar.isDate(date, inSameDayAs: now) {
            return String(localized: "Today", locale: locale)
        }
        if let yesterday = calendar.date(byAdding: .day, value: -1, to: now), calendar.isDate(date, inSameDayAs: yesterday) {
            return String(localized: "Yesterday", locale: locale)
        }
        return date.formatted(Date.FormatStyle(date: .abbreviated, time: .omitted, locale: locale))
    }

    /// `"Today, 14:32"` / `"Yesterday, 9:05 AM"` / `"Sep 3, 2026, 18:00"`.
    static func relativeDayAndTime(_ date: Date, now: Date = Date(), calendar: Calendar = .current, locale: Locale = .autoupdatingCurrent) -> String {
        let day = relativeDay(date, now: now, calendar: calendar, locale: locale)
        let time = date.formatted(Date.FormatStyle(date: .omitted, time: .shortened, locale: locale))
        return "\(day), \(time)"
    }
}

extension Currency {
    /// ISO 4217 code, for `MoneyFormatter`. One currency per vault
    /// (docs/v2/ARCH.md §7); this switch grows with the enum.
    var code: String {
        switch self {
        case .eur: "EUR"
        }
    }
}

/// Sentences that carry a count. Each is one catalog key with a plural
/// variation (`"%lld rows"`, one and other in English and Italian), never a
/// number glued to a word: that reads "1 rows", and a translation cannot move
/// the number.
///
/// `bundle` is the catalog to read the key from: the app's own, or one a
/// test builds so the plural forms can be checked before the catalog has them.
enum CountText {
    /// `"3 to confirm"`: the top bar's due pill.
    static func toConfirm(_ count: Int, bundle: Bundle = .main, locale: Locale = .autoupdatingCurrent) -> String {
        String(localized: "\(count) to confirm", bundle: bundle, locale: locale)
    }

    /// The undo toast, for one row or a selection.
    static func voided(_ count: Int, bundle: Bundle = .main, locale: Locale = .autoupdatingCurrent) -> String {
        String(localized: "\(count) transactions deleted", bundle: bundle, locale: locale)
    }

    /// `"5 days ago"` under a due period's date, `"today"` for one due today.
    static func daysAgo(_ count: Int, bundle: Bundle = .main, locale: Locale = .autoupdatingCurrent) -> String {
        count <= 0
            ? String(localized: "today", bundle: bundle, locale: locale)
            : String(localized: "\(count) days ago", bundle: bundle, locale: locale)
    }
}

/// Days as the Ricorrenze tab writes them (`docs/v2/UI.md` §2.5): `"gio 1
/// ott"` in Italian, `"Thu, Oct 1"` in English, the order and the
/// abbreviations the locale's own.
///
/// A `NaiveDate` is a calendar day with no time zone, so it is read and
/// written in UTC on both sides: the local zone could put its midnight on
/// the day before.
enum RecurringDayText {
    private static let utc = TimeZone(identifier: "UTC") ?? .gmt

    private static func style(_ locale: Locale) -> Date.FormatStyle {
        var calendar = Calendar(identifier: .gregorian)
        calendar.locale = locale
        return Date.FormatStyle(locale: locale, calendar: calendar, timeZone: utc)
    }

    private static func date(_ day: NaiveDate) -> Date? {
        CoreDate.localDay(day, timeZone: utc)
    }

    /// `"gio 1 ott"`: a due period, a tile of the agenda.
    static func weekday(_ day: NaiveDate, locale: Locale = .autoupdatingCurrent) -> String {
        guard let date = date(day) else { return day }
        return date.formatted(style(locale).weekday(.abbreviated).day().month(.abbreviated))
    }

    /// `"gio 1 ott 2026"`: the inspector's next dates, which run into the
    /// years ahead.
    static func weekdayWithYear(_ day: NaiveDate, locale: Locale = .autoupdatingCurrent) -> String {
        guard let date = date(day) else { return day }
        return date.formatted(style(locale).weekday(.abbreviated).day().month(.abbreviated).year())
    }

    /// `"1 ott"`, or `"15 mar 2027"` outside the year of `today`: the
    /// templates table's Prossima.
    static func short(_ day: NaiveDate, today: NaiveDate, locale: Locale = .autoupdatingCurrent) -> String {
        guard let date = date(day) else { return day }
        let base = style(locale).day().month(.abbreviated)
        return day.prefix(4) == today.prefix(4) ? date.formatted(base) : date.formatted(base.year())
    }

    /// `"1 nov 2025"`: a start or end date.
    static func full(_ day: NaiveDate, locale: Locale = .autoupdatingCurrent) -> String {
        guard let date = date(day) else { return day }
        return date.formatted(style(locale).day().month(.abbreviated).year())
    }
}

/// What a schedule's interval counts.
enum ScheduleUnit {
    case day
    case week
    case month
    case year
}

/// Describes a recurring template's `Schedule` (`docs/v2/ARCH.md` §4:
/// `frequency` + `interval`, day/weekday/month clamped to the calendar by
/// the core) for the Recurring panel's list and edit form.
///
/// Every description is one whole sentence in the catalog. An interval of
/// one and an interval of more are two sentences rather than one plural key:
/// a plural key holds a single `%lld` (`scripts/catalog.py`), these hold a
/// name or a day beside it, and above one both English and Italian use the
/// same form anyway.
enum ScheduleFormatting {
    static func describe(
        _ schedule: Schedule,
        bundle: Bundle = .main,
        locale: Locale = .autoupdatingCurrent
    ) -> String {
        let interval = Int(schedule.interval)
        switch schedule.frequency {
        case .daily:
            return every(interval, .day, bundle: bundle, locale: locale)
        case .weekly(let weekday):
            let name = weekdayName(weekday, locale: locale)
            return interval == 1
                ? String(localized: "Every \(name)", bundle: bundle, locale: locale)
                : String(localized: "Every \(interval) weeks on \(name)", bundle: bundle, locale: locale)
        case .monthly(let day):
            let day = Int(day)
            return interval == 1
                ? String(localized: "Every month on day \(day)", bundle: bundle, locale: locale)
                : String(localized: "Every \(interval) months on day \(day)", bundle: bundle, locale: locale)
        case .yearly(let month, let day):
            let name = monthName(month, locale: locale)
            let day = Int(day)
            return interval == 1
                ? String(localized: "Every year on \(name) \(day)", bundle: bundle, locale: locale)
                : String(localized: "Every \(interval) years on \(name) \(day)", bundle: bundle, locale: locale)
        }
    }

    /// `"Every day"`, `"Every 3 weeks"`: the interval with its unit, one
    /// plural key per unit. The edit form's stepper reads it as it counts.
    static func every(
        _ interval: Int,
        _ unit: ScheduleUnit,
        bundle: Bundle = .main,
        locale: Locale = .autoupdatingCurrent
    ) -> String {
        switch unit {
        case .day: String(localized: "Every \(interval) days", bundle: bundle, locale: locale)
        case .week: String(localized: "Every \(interval) weeks", bundle: bundle, locale: locale)
        case .month: String(localized: "Every \(interval) months", bundle: bundle, locale: locale)
        case .year: String(localized: "Every \(interval) years", bundle: bundle, locale: locale)
        }
    }

    /// The localized weekday name for an ISO weekday (Monday = 1 ... Sunday
    /// = 7), used both by `describe` and by the weekday picker.
    static func weekdayName(_ isoWeekday: UInt8, locale: Locale = .autoupdatingCurrent) -> String {
        var calendar = Calendar(identifier: .gregorian)
        calendar.locale = locale
        let symbols = calendar.weekdaySymbols
        let index = Int(isoWeekday) % 7
        return symbols.indices.contains(index) ? symbols[index] : "\(isoWeekday)"
    }

    /// The localized month name for a 1-based month, used both by
    /// `describe` and by the yearly-schedule picker.
    static func monthName(_ month: UInt8, locale: Locale = .autoupdatingCurrent) -> String {
        var calendar = Calendar(identifier: .gregorian)
        calendar.locale = locale
        let symbols = calendar.monthSymbols
        let index = Int(month) - 1
        return symbols.indices.contains(index) ? symbols[index] : "\(month)"
    }
}
