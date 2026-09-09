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

    /// Same as `format`, but always shows a leading `+` for positive amounts
    /// (negative amounts already carry their own sign).
    static func formatSigned(minorUnits: Int64, currencyCode: String, locale: Locale = .autoupdatingCurrent) -> String {
        let formatted = format(minorUnits: minorUnits, currencyCode: currencyCode, locale: locale)
        return minorUnits > 0 ? "+\(formatted)" : formatted
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

/// Describes a recurring template's `Schedule` (`docs/v2/ARCH.md` §4:
/// `frequency` + `interval`, day/weekday/month clamped to the calendar by
/// the core) for the Recurring panel's list and edit form.
enum ScheduleFormatting {
    static func describe(_ schedule: Schedule, locale: Locale = .autoupdatingCurrent) -> String {
        frequencyText(schedule.frequency, interval: schedule.interval, locale: locale)
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

    private static func frequencyText(_ frequency: Frequency, interval: UInt32, locale: Locale) -> String {
        switch frequency {
        case .daily:
            return interval == 1
                ? String(localized: "Daily")
                : String(localized: "Every") + " \(interval) " + String(localized: "days")
        case .weekly(let weekday):
            let name = weekdayName(weekday, locale: locale)
            return interval == 1
                ? String(localized: "Weekly on") + " " + name
                : String(localized: "Every") + " \(interval) " + String(localized: "weeks on") + " " + name
        case .monthly(let day):
            return interval == 1
                ? String(localized: "Monthly on day") + " \(day)"
                : String(localized: "Every") + " \(interval) " + String(localized: "months on day") + " \(day)"
        case .yearly(let month, let day):
            let name = monthName(month, locale: locale)
            return String(localized: "Yearly on") + " \(name) \(day)"
        }
    }
}
