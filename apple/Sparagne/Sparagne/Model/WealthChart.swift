import Foundation
import SparagneCore

/// The points of a loan's residual-debt chart: the debt from the month
/// before the first installment to the last, split at today into the part
/// behind (drawn stronger) and the part ahead, with a dot on each event.
struct LoanChart: Equatable, Sendable {
    struct Point: Equatable, Sendable {
        let day: NaiveDate
        let residual: Int64
    }

    /// From the start up to today, today's point last. Empty when the loan
    /// has not started yet.
    let past: [Point]
    /// From today's point, shared with `past` so the two areas meet under
    /// the «oggi» rule, to the end. Empty once the loan is over.
    let future: [Point]
    /// Where the «oggi» rule stands, while the loan runs.
    let today: Point?
    /// One per event the schedule applied: its date, and the debt it left.
    let events: [Point]
    /// The highest debt drawn: the top of the scale.
    var peak: Int64 { (past + future).map(\.residual).max() ?? 0 }

    static func build(loan: LoanView, schedule: LoanSchedule, today: NaiveDate) -> LoanChart {
        let opening = LoanFacts.opening(loan, schedule: schedule)
        let start = Point(day: Self.monthBefore(loan.terms.firstDue), residual: loan.terms.principal)
        let rows = schedule.rows.map { Point(day: $0.dueDate, residual: $0.residual) }
        // Repayments before the first installment drop the line at once.
        let first = opening == loan.terms.principal ? [start] : [start, Point(day: start.day, residual: opening)]
        let line = first + rows
        guard let end = line.last, today >= start.day, today < end.day else {
            let started = today >= start.day
            return LoanChart(
                past: started ? line : [],
                future: started ? [] : line,
                today: nil,
                events: events(loan: loan, schedule: schedule, opening: opening)
            )
        }
        let behind = line.filter { $0.day <= today }
        let now = Point(day: today, residual: behind.last?.residual ?? opening)
        return LoanChart(
            past: behind + [now],
            future: [now] + line.filter { $0.day > today },
            today: now,
            events: events(loan: loan, schedule: schedule, opening: opening)
        )
    }

    /// Each event at its date, at the debt left by the installment it
    /// follows and itself: on the line for a rate change, at the foot of the
    /// drop for a repayment. An event the schedule never reached (after an
    /// early end) has no dot.
    private static func events(loan: LoanView, schedule: LoanSchedule, opening: Int64) -> [Point] {
        let before = Set(schedule.eventIdsBeforeFirst)
        return loan.events.compactMap { view in
            if before.contains(view.id) {
                return Point(day: view.event.effectiveDate, residual: opening)
            }
            guard let row = schedule.rows.first(where: { $0.eventIds.contains(view.id) }) else { return nil }
            return Point(day: view.event.effectiveDate, residual: row.residual)
        }
    }

    /// The same day a month earlier, the last of the month when it is
    /// shorter: where the debt stands whole, before the first installment.
    static func monthBefore(_ day: NaiveDate) -> NaiveDate {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = CoreDate.utc
        guard let date = CoreDate.localDay(day, timeZone: CoreDate.utc),
              let earlier = calendar.date(byAdding: .month, value: -1, to: date)
        else { return day }
        return CoreDate.day(earlier, timeZone: CoreDate.utc)
    }
}

extension LoanEvent {
    /// The day the event takes effect, as the core's `LoanEvent::date`.
    var effectiveDate: NaiveDate {
        switch self {
        case .rateChange(let date, _, _), .prepayment(let date, _, _, _): date
        }
    }
}
