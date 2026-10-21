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
        /// A paused template has a past period it never decided before the
        /// last one it did: the copy starts after that one, and would never
        /// bring it back. Only a resumed template lists it, to be decided.
        case periodsBehindPaused
        /// The owner left the vault: the copy could not name them.
        case ownerLeft
        case readOnly
        case archived
        /// No period is left before the schedule's end.
        case noPeriodsLeft

        /// What the inspector's menu says under its greyed item, and the
        /// alert when the change is asked for anyway.
        var reason: String {
            switch self {
            case .pendingPeriods: String(localized: "Record or skip the pending periods first")
            case .periodsBehindPaused: String(localized: "Resume it, then record or skip its pending periods")
            case .ownerLeft: String(localized: "Its owner is no longer a member")
            case .readOnly: String(localized: "This vault is shared with you to read, not to change.")
            case .archived: String(localized: "Restore the template first")
            case .noPeriodsLeft: String(localized: "No periods left before its end")
            }
        }
    }

    /// The kind the copy takes: a template is an expense or an income, never
    /// a refund or a transfer.
    static func otherKind(of kind: TransactionKind) -> TransactionKind {
        kind == .income ? .expense : .income
    }

    /// The first occurrence of `schedule` after the latest date in `decided`
    /// (the schedule's first when nothing is decided), `nil` when none is
    /// left before its end. An occurrence date, so the copy's series is the
    /// template's, an interval above one included.
    ///
    /// The core lists the occurrences on or after a day, so the latest
    /// decided date comes back first while it still is one; a date the
    /// schedule no longer has, edited since, does not.
    static func start(of schedule: Schedule, decided: Set<NaiveDate>) -> NaiveDate? {
        let latest = decided.max()
        let ahead = (try? scheduleOccurrences(schedule: schedule, from: latest ?? schedule.startDate, limit: 2)) ?? []
        guard let latest else { return ahead.first }
        return ahead.first { $0 > latest }
    }

    /// `schedule` starting on `start`, its end and cadence kept.
    static func schedule(_ schedule: Schedule, startingOn start: NaiveDate) -> Schedule {
        var moved = schedule
        moved.startDate = start
        return moved
    }

    /// The command that creates `template`'s copy: the other kind, with the
    /// same amount, wallet, envelope, category, note and cadence, from
    /// `start`. `owner` is what the command names, `nil` for the author
    /// (`AppStore.explicitPerson`).
    static func copy(of template: RecurringView, startingOn start: NaiveDate, owner: String?) -> Command {
        .createRecurring(
            transactionKind: otherKind(of: template.kind),
            amount: template.amount,
            walletId: template.walletId,
            flowId: template.flowId,
            category: template.category,
            note: template.note,
            schedule: schedule(template.schedule, startingOn: start),
            owner: owner
        )
    }
}
