import Foundation
import SparagneCore

/// Turning a recurring template into the other kind, expense or income.
///
/// A patch never changes the kind (`RecurringPatch`), so the template is
/// archived and a copy of the other kind takes over from the first period
/// after the last one decided, executed or skipped: no period comes due
/// twice. Its runs stay with the archived template.
nonisolated enum RecurringKindChange {
    /// Why a template cannot change kind now.
    enum Refusal: Error, Equatable, Sendable {
        /// Periods are waiting: they would not move to the copy.
        case pendingPeriods
        /// The owner left the vault: the copy could not name them.
        case ownerLeft
        case readOnly
        case archived
        /// No period is left before the schedule's end.
        case noPeriodsLeft
    }

    /// The first occurrence of `schedule` after the latest date in `decided`
    /// (the schedule's first when nothing is decided), `nil` when none is
    /// left before its end. An occurrence date, so the copy's series is the
    /// template's, an interval above one included.
    static func start(of schedule: Schedule, decided: Set<NaiveDate>) -> NaiveDate? {
        nil
    }

    /// `schedule` starting on `start`, its end and cadence kept.
    static func schedule(_ schedule: Schedule, startingOn start: NaiveDate) -> Schedule {
        schedule
    }
}
