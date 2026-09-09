import Foundation
import SparagneCore

/// The date window the transactions table and the totals line cover.
///
/// Bounds are half-open `[from, to)`, matching `TransactionFilter`
/// (`core/src/query.rs`), and cross the FFI as UTC RFC 3339 strings.
enum Period: String, CaseIterable, Identifiable, Sendable {
    case thisMonth
    case last30Days
    case all

    var id: String { rawValue }

    var label: String {
        switch self {
        case .thisMonth: String(localized: "This month")
        case .last30Days: String(localized: "Last 30 days")
        case .all: String(localized: "All")
        }
    }

    /// `nil` bounds mean "unfiltered": both the transaction filter and
    /// `period_totals` take open ends.
    func bounds(now: Date = Date(), calendar: Calendar = .current) -> (from: UtcDateTime?, to: UtcDateTime?) {
        switch self {
        case .all:
            return (nil, nil)
        case .thisMonth, .last30Days:
            let range = dateRange(now: now, calendar: calendar)
            return (CoreDate.utcString(range.lowerBound), CoreDate.utcString(range.upperBound))
        }
    }

    private func dateRange(now: Date, calendar: Calendar) -> Range<Date> {
        let startOfToday = calendar.startOfDay(for: now)
        let tomorrow = calendar.date(byAdding: .day, value: 1, to: startOfToday) ?? now

        switch self {
        case .thisMonth:
            let start = calendar.date(from: calendar.dateComponents([.year, .month], from: now)) ?? startOfToday
            let end = calendar.date(byAdding: .month, value: 1, to: start) ?? tomorrow
            return start..<max(end, tomorrow)
        case .last30Days:
            let start = calendar.date(byAdding: .day, value: -30, to: startOfToday) ?? startOfToday
            return start..<tomorrow
        case .all:
            // Unreachable: `.all` never asks for a range, its bounds are open.
            return startOfToday..<tomorrow
        }
    }
}
