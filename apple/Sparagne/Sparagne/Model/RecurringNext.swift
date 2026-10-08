import Foundation
import SparagneCore

/// When a template next wants something: the
/// "Prossima" column of the templates table, and the inspector's "Prossime
/// date".
enum RecurringNext: Equatable {
    /// The oldest period still waiting for a decision. It beats any future
    /// date: until it is recorded or skipped, it is what the template is
    /// waiting on.
    case due(NaiveDate)
    /// The first period ahead.
    case upcoming(NaiveDate)
    /// The schedule has run out (past its end date) or cannot be read.
    case finished
    /// Archived templates have no next period until they are restored.
    case archived

    /// `due` is the template's periods waiting for a decision, from
    /// `AppStore.pendingRecurringItems`.
    ///
    /// Today's period of a running template that is not due has already been
    /// recorded or skipped, so the next one is looked for from tomorrow. A
    /// disabled template is never due, so its period of today is still ahead
    /// of it.
    static func of(
        _ template: RecurringView,
        due: [NaiveDate],
        today: NaiveDate,
        occurrences: ScheduleOccurrences
    ) -> RecurringNext {
        if template.archived { return .archived }
        if let oldest = due.min() { return .due(oldest) }
        let from = template.enabled ? (NaiveDay.adding(1, to: today) ?? today) : today
        guard let next = try? occurrences(template.schedule, from, 1).first else { return .finished }
        return .upcoming(next)
    }

    /// One line of the inspector's "Prossime date".
    struct Preview: Equatable, Hashable {
        let date: NaiveDate
        /// Tagged "da confermare".
        let isDue: Bool
    }

    /// The next `count` dates of a schedule, the periods still due first.
    ///
    /// `due` is the saved template's due periods, and only means something
    /// while the schedule on screen is the saved one: the caller passes `[]`
    /// once the draft's schedule differs, since the core works the due list
    /// out again from the schedule that gets saved. `from` is where the
    /// future starts (today, or tomorrow when today's period has been
    /// decided, as in `of`).
    ///
    /// Throws the core's refusal of an invalid schedule, which the inspector
    /// shows in place of the dates.
    static func preview(
        schedule: Schedule,
        due: [NaiveDate],
        from: NaiveDate,
        count: Int,
        occurrences: ScheduleOccurrences
    ) throws -> [Preview] {
        let ahead = try occurrences(schedule, from, UInt32(max(count, 0)))
        let pending = Set(due)
        let dates = due.sorted().map { Preview(date: $0, isDue: true) }
            + ahead.filter { !pending.contains($0) }.map { Preview(date: $0, isDue: false) }
        return Array(dates.prefix(count))
    }
}
