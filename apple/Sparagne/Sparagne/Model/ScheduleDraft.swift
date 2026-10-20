import Foundation
import SparagneCore

/// The Quando of an inspector: a `Schedule` as typed, its cadence, interval,
/// day, start and end. A template in Ricorrenze (`RecurringDraft.when`) and
/// the allocation in Riparto (`AllocationScheduleDraft`) edit their schedule
/// the same way, so they share this draft and its rules. A plain struct with
/// no view in it, so what gets sent can be tested on its own.
///
/// The day fields of the other cadences are kept while the user tries them,
/// so switching back loses nothing.
struct ScheduleDraft: Equatable {
    /// `Frequency` as a flat choice, the segmented Giorno / Settimana / Mese
    /// / Anno.
    enum Cadence: String, CaseIterable, Identifiable, Hashable {
        case daily, weekly, monthly, yearly

        var id: String { rawValue }

        /// What the interval counts: "ogni 2 settimane".
        var unit: ScheduleUnit {
            switch self {
            case .daily: .day
            case .weekly: .week
            case .monthly: .month
            case .yearly: .year
            }
        }
    }

    var cadence: Cadence = .monthly
    var interval = 1
    /// ISO: Monday = 1 ... Sunday = 7.
    var weekday = 1
    var monthDay = 1
    var yearMonth = 1
    var yearDay = 1
    var startDate: NaiveDate
    /// The end the schedule is sent with, `nil` for none. Riparto has no
    /// field for it, so an allocation's goes through as it is.
    var endDate: NaiveDate?
    /// The End field's day while its switch says Mai, so turning it back on
    /// brings the same day back. `nil`, with the start standing in, until
    /// the field sets it: a draft with no End field, Riparto's, then
    /// compares by the fields it shows (`AllocationTab` tells an untouched
    /// draft that way).
    private var keptEndDate: NaiveDate?

    /// Monthly, on today's day, from today, with no end. Starting today makes
    /// today the first period: a new template or allocation never opens with
    /// periods already behind it.
    init(today: NaiveDate) {
        startDate = today
        if let parts = NaiveDay.components(today) {
            weekday = parts.isoWeekday
            monthDay = parts.day
            yearMonth = parts.month
            yearDay = parts.day
        }
    }

    /// The fields of a saved schedule, as they are. The other cadences' days
    /// start from the start date, as a new draft's do from today.
    init(schedule: Schedule) {
        self.init(today: schedule.startDate)
        endDate = schedule.endDate
        interval = Int(schedule.interval)
        switch schedule.frequency {
        case .daily:
            cadence = .daily
        case .weekly(let day):
            cadence = .weekly
            weekday = Int(day)
        case .monthly(let day):
            cadence = .monthly
            monthDay = Int(day)
        case .yearly(let month, let day):
            cadence = .yearly
            yearMonth = Int(month)
            yearDay = Int(day)
        }
    }

    /// "Fine: Mai / Il giorno". Turned off, the day stays in the End field.
    var hasEndDate: Bool {
        get { endDate != nil }
        set {
            let day = shownEndDate
            keptEndDate = day
            endDate = newValue ? day : nil
        }
    }

    /// The day in the End field, whether its switch is on or off.
    var shownEndDate: NaiveDate {
        get { endDate ?? keptEndDate ?? startDate }
        set {
            keptEndDate = newValue
            if endDate != nil { endDate = newValue }
        }
    }

    var frequency: Frequency {
        switch cadence {
        case .daily: .daily
        case .weekly: .weekly(weekday: Self.byte(weekday))
        case .monthly: .monthly(day: Self.byte(monthDay))
        case .yearly: .yearly(month: Self.byte(yearMonth), day: Self.byte(yearDay))
        }
    }

    /// The schedule as typed. Not checked here: an interval of zero or a
    /// day 32 go through as they are, for `isValid` to say no first and the
    /// core to refuse (`scheduleOccurrences`).
    var schedule: Schedule {
        Schedule(
            frequency: frequency,
            interval: UInt32(clamping: max(interval, 0)),
            startDate: startDate,
            endDate: endDate
        )
    }

    /// Whether the core would take the schedule: the rules of
    /// `Schedule::validate` in `core/src/recurring.rs`. Only the day fields
    /// of the chosen cadence count.
    var isValid: Bool {
        guard interval >= 1 else { return false }
        switch cadence {
        case .daily: break
        case .weekly: guard (1...7).contains(weekday) else { return false }
        case .monthly: guard (1...31).contains(monthDay) else { return false }
        case .yearly: guard (1...12).contains(yearMonth), (1...31).contains(yearDay) else { return false }
        }
        if let endDate, endDate < startDate { return false }
        return true
    }

    /// The day fields are small; anything out of range is caught by
    /// `isValid` before it is sent, and clamping keeps the conversion from
    /// trapping on the way.
    private static func byte(_ value: Int) -> UInt8 { UInt8(clamping: value) }
}
