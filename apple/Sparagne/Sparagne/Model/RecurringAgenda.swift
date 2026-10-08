import Foundation
import SparagneCore

/// `scheduleOccurrences(schedule:from:limit:)`, the core's list of a
/// schedule's next dates, passed in rather than called directly so the
/// agenda and the inspector's preview can be tested on a schedule of their
/// own making.
typealias ScheduleOccurrences = (Schedule, NaiveDate, UInt32) throws -> [NaiveDate]

/// The core's `scheduleOccurrences`, in the shape `ScheduleOccurrences` takes
/// (the generated function has argument labels). Pure arithmetic with no
/// database behind it, so it is called on the main actor directly rather
/// than through `CoreActor`.
enum CoreSchedule {
    static func occurrences(_ schedule: Schedule, _ from: NaiveDate, _ limit: UInt32) throws -> [NaiveDate] {
        try scheduleOccurrences(schedule: schedule, from: from, limit: limit)
    }
}

/// Day arithmetic on the core's `yyyy-MM-dd` dates. A `NaiveDate` has no time
/// zone, so the arithmetic must not have one either: a Gregorian calendar in
/// UTC, where a day is always 24 hours and a daylight-saving change never
/// moves midnight onto the day before.
enum NaiveDay {
    private static let utc = TimeZone(identifier: "UTC") ?? .gmt

    private static var calendar: Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = utc
        return calendar
    }

    /// `"2026-10-07"` plus `days`, or `nil` for a string that is not a day.
    static func adding(_ days: Int, to day: NaiveDate) -> NaiveDate? {
        guard let date = CoreDate.localDay(day, timeZone: utc),
              let moved = calendar.date(byAdding: .day, value: days, to: date)
        else { return nil }
        return CoreDate.day(moved, timeZone: utc)
    }

    /// Whole days from `start` to `end`: positive when `end` is later.
    static func days(from start: NaiveDate, to end: NaiveDate) -> Int? {
        guard let from = CoreDate.localDay(start, timeZone: utc),
              let to = CoreDate.localDay(end, timeZone: utc)
        else { return nil }
        return calendar.dateComponents([.day], from: from, to: to).day
    }

    /// The year, month and day of a `yyyy-MM-dd`, for seeding a new
    /// template's day of the month or of the week from today.
    static func components(_ day: NaiveDate) -> (year: Int, month: Int, day: Int, isoWeekday: Int)? {
        guard let date = CoreDate.localDay(day, timeZone: utc) else { return nil }
        let parts = calendar.dateComponents([.year, .month, .day, .weekday], from: date)
        guard let year = parts.year, let month = parts.month, let dayOfMonth = parts.day,
              let weekday = parts.weekday
        else { return nil }
        // Foundation counts Sunday = 1; the core counts Monday = 1.
        let iso = weekday == 1 ? 7 : weekday - 1
        return (year, month, dayOfMonth, iso)
    }
}

/// The Ricorrenze tab's "Prossimi 30 giorni": every
/// period of the templates that run, from tomorrow to `days` days from today,
/// with what they add up to on each side.
///
/// It starts tomorrow because today belongs to "Da confermare": a period of
/// today is either due, and listed there, or already recorded or skipped. A
/// day is never counted in both cards.
struct RecurringAgenda: Equatable {
    /// Soonest first; on the same day, by the template's title.
    let tiles: [UpcomingPeriod]

    init(tiles: [UpcomingPeriod]) {
        self.tiles = tiles
    }

    /// The expenses expected in the window, in minor units.
    var expenses: Int64 { total(of: .expense) }
    /// The income expected in the window, in minor units.
    var income: Int64 { total(of: .income) }

    private func total(of kind: TransactionKind) -> Int64 {
        tiles.reduce(0) { $1.template.kind == kind ? $0 + $1.template.amount : $0 }
    }

    /// The agenda of `templates` from the day after `today` to `today + days`.
    ///
    /// A disabled or archived template has no future: the core never makes it
    /// due, so it is left out. A schedule the core refuses (it cannot be
    /// saved, but a synced one could predate a rule) is left out too, rather
    /// than emptying the whole agenda.
    static func build(
        templates: [RecurringView],
        today: NaiveDate,
        days: Int = 30,
        occurrences: ScheduleOccurrences
    ) -> RecurringAgenda {
        guard days > 0,
              let first = NaiveDay.adding(1, to: today),
              let last = NaiveDay.adding(days, to: today)
        else { return RecurringAgenda(tiles: []) }
        // One more than the days in the window would ever need: a daily
        // template has at most `days` dates in it.
        let limit = UInt32(days + 1)
        var tiles: [UpcomingPeriod] = []
        for template in templates where template.enabled && !template.archived {
            guard let dates = try? occurrences(template.schedule, first, limit) else { continue }
            for date in dates where date >= first && date <= last {
                tiles.append(UpcomingPeriod(template: template, date: date))
            }
        }
        tiles.sort { lhs, rhs in
            if lhs.date != rhs.date { return lhs.date < rhs.date }
            let left = RecurringTitle.of(lhs.template)
            let right = RecurringTitle.of(rhs.template)
            if left != right { return left.localizedStandardCompare(right) == .orderedAscending }
            return lhs.template.id < rhs.template.id
        }
        return RecurringAgenda(tiles: tiles)
    }
}

/// What a template is called on screen: its note, or its category when it has
/// no note, as the canvas names "mutuo" and "Netflix". Neither is required,
/// so the last resort is the placeholder dash.
enum RecurringTitle {
    static func of(_ template: RecurringView) -> String {
        if let note = template.note?.trimmingCharacters(in: .whitespaces), !note.isEmpty { return note }
        if let category = template.category?.trimmingCharacters(in: .whitespaces), !category.isEmpty {
            return category
        }
        return TransactionRow.placeholder
    }
}
