import Foundation
import SparagneCore

/// One line of the Mastro's grid: a stored transaction, or a recurring period
/// that fell due and waits for a decision (`docs/v2/UI.md` §2.1, §2.5).
enum LedgerLine: Identifiable, Hashable, Sendable {
    case row(TransactionRow)
    case pending(DuePeriod)

    var id: String {
        switch self {
        case .row(let row): "row/\(row.id)"
        case .pending(let period): "due/\(period.id)"
        }
    }
}

/// The grid's lines in the order the eye reads a month: the due periods sit
/// among the rows at the day they fell due, so "mutuo, 1 Oct" is next to the
/// other rows of the first, not in a banner that has to be cross-checked.
///
/// Pure, so the placement is tested without a window
/// (`SparagneTests/LedgerLinesTests.swift`).
enum LedgerLines {
    /// `rows` as the store loaded them (ascending date), with the periods of
    /// `due` that belong on this sheet slotted in.
    ///
    /// A period belongs when its date is in `month` and its template's kind
    /// is in `direction`: an income template under ENTRATE, an expense one
    /// under USCITE. A template has no author (`RecurringView`), so the
    /// PERSONA filter cannot place it and it shows for everybody. A search
    /// narrows the periods as it narrows the rows, by note and category.
    ///
    /// Same day as stored rows: the period goes after them. It is not written
    /// yet, so it comes last among the day's entries, where the next row
    /// typed that day would land.
    static func interleave(
        rows: [TransactionRow],
        due: [DuePeriod],
        month: MonthKey,
        direction: LedgerDirection,
        search: String = "",
        timeZone: TimeZone = .current
    ) -> [LedgerLine] {
        let prefix = String(format: "%04d-%02d-", month.year, month.month)
        let needle = search.trimmingCharacters(in: .whitespacesAndNewlines)
        let pending = due
            .filter { period in
                period.date.hasPrefix(prefix)
                    && direction.kinds.contains(period.template.kind)
                    && matches(period.template, needle)
            }
            // `NaiveDate` is `yyyy-MM-dd`, so the strings sort as the days do;
            // the template id keeps two periods of one day in a fixed order.
            .sorted { ($0.date, $0.template.id) < ($1.date, $1.template.id) }

        var lines: [LedgerLine] = []
        lines.reserveCapacity(rows.count + pending.count)
        var next = 0
        for row in rows {
            let day = CoreDate.day(row.occurredAt, timeZone: timeZone)
            while next < pending.count, pending[next].date < day {
                lines.append(.pending(pending[next]))
                next += 1
            }
            lines.append(.row(row))
        }
        lines.append(contentsOf: pending[next...].map(LedgerLine.pending))
        return lines
    }

    private static func matches(_ template: RecurringView, _ needle: String) -> Bool {
        guard !needle.isEmpty else { return true }
        return [template.note, template.category]
            .compactMap { $0 }
            .contains { $0.range(of: needle, options: [.caseInsensitive, .diacriticInsensitive]) != nil }
    }
}

extension LedgerDate {
    /// `"01"`: the DATA column's day, two digits so the column lines up.
    static func dayNumber(_ date: Date, calendar: Calendar = .current) -> String {
        String(format: "%02d", calendar.component(.day, from: date))
    }

    /// `"gio"` in Italian, `"Thu"` in English: the weekday after the day
    /// number in the DATA column, in the locale's own case and without the
    /// trailing dot some locales add, as `shortMonth` does.
    static func weekday(_ date: Date, calendar: Calendar = .current, locale: Locale = .autoupdatingCurrent) -> String {
        var symbols = Calendar(identifier: .gregorian)
        symbols.locale = locale
        let names = symbols.shortStandaloneWeekdaySymbols
        let index = calendar.component(.weekday, from: date) - 1
        guard names.indices.contains(index) else { return "" }
        return names[index].replacingOccurrences(of: ".", with: "")
    }
}
