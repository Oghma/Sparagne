import Foundation

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
