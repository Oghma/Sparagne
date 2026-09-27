import Foundation
import SparagneCore

/// Conversions between `Date` and the string scalars the core expects
/// (`core/src/ffi.rs`): `OffsetDateTime` is RFC 3339 with the system offset,
/// `UtcDateTime` is RFC 3339 in UTC, `NaiveDate` is a local `yyyy-MM-dd`.
enum CoreDate {
    private static func style(_ timeZone: TimeZone) -> Date.ISO8601FormatStyle {
        Date.ISO8601FormatStyle(
            dateSeparator: .dash,
            dateTimeSeparator: .standard,
            timeSeparator: .colon,
            timeZoneSeparator: .colon,
            includingFractionalSeconds: false,
            timeZone: timeZone
        )
    }

    private static let utc = TimeZone(identifier: "UTC") ?? .gmt

    /// `2026-03-01T12:00:00+01:00` in the system time zone.
    static func offset(_ date: Date, timeZone: TimeZone = .current) -> OffsetDateTime {
        date.formatted(style(timeZone))
    }

    /// `2026-03-01T11:00:00Z`.
    static func utcString(_ date: Date) -> UtcDateTime {
        date.formatted(style(utc))
    }

    /// Parses either flavour; the offset in the string wins over the style's.
    static func date(_ string: String) -> Date? {
        try? Date(string, strategy: style(utc))
    }

    /// The calendar day of `date` in the given time zone, as `yyyy-MM-dd`.
    static func day(_ date: Date, timeZone: TimeZone = .current) -> NaiveDate {
        date.formatted(dayStyle(timeZone))
    }

    /// A bare `yyyy-MM-dd` read back as local midnight.
    static func localDay(_ day: NaiveDate, timeZone: TimeZone = .current) -> Date? {
        try? Date(day, strategy: dayStyle(timeZone))
    }

    private static func dayStyle(_ timeZone: TimeZone) -> Date.ISO8601FormatStyle {
        Date.ISO8601FormatStyle(dateSeparator: .dash, timeZone: timeZone).year().month().day()
    }
}
