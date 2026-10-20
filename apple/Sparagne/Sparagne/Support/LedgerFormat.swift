import Foundation

/// Money as the ledger writes it (`€2.448,53`), fixed whatever the locale: the layout depends on it.
enum LedgerMoney {
    private static let formatter: NumberFormatter = {
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        formatter.groupingSeparator = "."
        formatter.decimalSeparator = ","
        formatter.usesGroupingSeparator = true
        formatter.minimumFractionDigits = 2
        formatter.maximumFractionDigits = 2
        return formatter
    }()

    /// `"2.448,53"`. Negative amounts keep their minus sign.
    static func bare(_ minorUnits: Int64) -> String {
        let value = Decimal(minorUnits) / 100
        return formatter.string(from: value as NSDecimalNumber) ?? "0,00"
    }

    /// `"€2.448,53"`.
    static func amount(_ minorUnits: Int64, symbol: String = "€") -> String {
        let text = bare(abs(minorUnits))
        return minorUnits < 0 ? "-\(symbol)\(text)" : "\(symbol)\(text)"
    }

    /// A percentage as the cards show it: `"73,1%"`. `nil` when the base is
    /// zero, so the caller can print a dash instead of a fake `0,0%`.
    static func percent(_ value: Int64, of base: Int64, decimals: Int = 1) -> String? {
        guard base != 0 else { return nil }
        let ratio = (Decimal(value) / Decimal(base) * 100) as NSDecimalNumber
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        formatter.decimalSeparator = ","
        formatter.groupingSeparator = "."
        formatter.minimumFractionDigits = decimals
        formatter.maximumFractionDigits = decimals
        return formatter.string(from: ratio).map { "\($0)%" }
    }

    /// Savings as a share of income, as the summary card and the ledger's
    /// panel write it: `"73%"`, `"-26%"`, and `"< -100%"` once more than twice
    /// the income went out. Past that the figure only says how little came
    /// in: 0,07 € of interest against a month of expenses reads -394500%.
    /// `nil` with no income, so the caller can print a dash.
    static func savingsRate(_ savings: Int64, of income: Int64) -> String? {
        guard income > 0 else { return nil }
        guard savings >= -income else { return "< -100%" }
        return percent(savings, of: income, decimals: 0)
    }

    /// `"12.50"`: the plain text `parseMoney` accepts, for a grid cell being
    /// edited, and the figure of an exported CSV. No sign, no symbol, no
    /// grouping, never through `Double`.
    nonisolated static func editable(_ minorUnits: Int64) -> String {
        let absolute = minorUnits.magnitude
        return String(format: "%llu.%02llu", absolute / 100, absolute % 100)
    }
}

/// Dates as the ledger writes them (`01 ago`, `AGOSTO 2026`), in the user's locale.
enum LedgerDate {
    /// `"01 ago"`.
    static func day(_ date: Date, calendar: Calendar = .current, locale: Locale = .autoupdatingCurrent) -> String {
        let components = calendar.dateComponents([.day, .month], from: date)
        let day = components.day ?? 1
        let month = shortMonth(components.month ?? 1, locale: locale)
        return String(format: "%02d %@", day, month)
    }

    /// `"ago"`, lowercased and stripped of the trailing dot some locales add.
    static func shortMonth(_ month: Int, locale: Locale = .autoupdatingCurrent) -> String {
        monthName(month, style: .shortStandalone, locale: locale)
            .replacingOccurrences(of: ".", with: "")
            .lowercased()
    }

    /// `"AGOSTO"`.
    static func fullMonth(_ month: Int, locale: Locale = .autoupdatingCurrent) -> String {
        monthName(month, style: .standalone, locale: locale).uppercased()
    }

    /// A name in a date (`.format`), on its own (`.standalone`), or abbreviated on its own.
    enum NameStyle {
        case format, standalone, shortStandalone
    }

    /// The locale's name of a 1-based `month`, in the locale's own case; the
    /// number itself when it is out of range.
    static func monthName(_ month: Int, style: NameStyle, locale: Locale = .autoupdatingCurrent) -> String {
        let calendar = gregorian(locale)
        let symbols = switch style {
        case .format: calendar.monthSymbols
        case .standalone: calendar.standaloneMonthSymbols
        case .shortStandalone: calendar.shortStandaloneMonthSymbols
        }
        let index = month - 1
        return symbols.indices.contains(index) ? symbols[index] : "\(month)"
    }

    /// The locale's name of a `weekday` numbered as `Calendar` numbers them,
    /// Sunday = 1 … Saturday = 7, in the locale's own case; the number
    /// itself when it is out of range. An ISO weekday (Monday = 1 … Sunday
    /// = 7) is `iso % 7 + 1`.
    static func weekdayName(_ weekday: Int, style: NameStyle, locale: Locale = .autoupdatingCurrent) -> String {
        let calendar = gregorian(locale)
        let symbols = switch style {
        case .format: calendar.weekdaySymbols
        case .standalone: calendar.standaloneWeekdaySymbols
        case .shortStandalone: calendar.shortStandaloneWeekdaySymbols
        }
        let index = weekday - 1
        return symbols.indices.contains(index) ? symbols[index] : "\(weekday)"
    }

    private static func gregorian(_ locale: Locale) -> Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.locale = locale
        return calendar
    }

    /// `"A"`: the single letter under a bar in the twelve-month strip.
    static func monthInitial(_ month: Int, locale: Locale = .autoupdatingCurrent) -> String {
        String(shortMonth(month, locale: locale).prefix(1)).uppercased()
    }

    /// Reads what was typed in a DATA cell: `29`, `29/8` or `29/8/2026`.
    ///
    /// A bare day stays inside `month`, which is what filling a ledger month
    /// by month means; a day out of range for the month it lands in returns
    /// `nil` so the cell snaps back instead of silently moving.
    static func parseDay(_ text: String, in month: MonthKey, calendar: Calendar = .current) -> Date? {
        let parts = text
            .split(whereSeparator: { !$0.isNumber })
            .compactMap { Int($0) }
        guard let day = parts.first, day >= 1 else { return nil }
        let wanted = DateComponents(
            year: parts.count > 2 ? parts[2] : month.year,
            month: parts.count > 1 ? parts[1] : month.month,
            day: day
        )
        guard let date = calendar.date(from: wanted),
              calendar.component(.day, from: date) == day else { return nil }
        return date
    }

    /// `"12:04"`, for the "saved at" of the tab bar's status line.
    static func clock(_ date: Date, locale: Locale = .autoupdatingCurrent) -> String {
        date.formatted(
            Date.FormatStyle(date: .omitted, time: .shortened, locale: locale)
                .hour(.twoDigits(amPM: .abbreviated))
                .minute(.twoDigits)
        )
    }
}
