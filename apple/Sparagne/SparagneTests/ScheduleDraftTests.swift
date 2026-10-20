import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The Quando both inspectors edit (`ScheduleDraft`): what a new schedule
/// starts from, what a saved one reads as, what is sent, what the core would
/// refuse, and the End field's day.
struct ScheduleDraftTests {
    // MARK: - From today

    @Test("A new schedule is monthly, on today's day, from today, with no end")
    func fromToday() {
        let draft = ScheduleDraft(today: "2026-10-10")
        #expect(draft.schedule == RecurringFixture.schedule(.monthly(day: 10), from: "2026-10-10"))
        #expect(draft.isValid)
        #expect(draft.cadence == .monthly)
        #expect(draft.interval == 1)
        // Saturday, for a switch to weekly; 10 October, for one to yearly.
        #expect(draft.weekday == 6)
        #expect(draft.yearMonth == 10)
        #expect(draft.yearDay == 10)
        #expect(draft.endDate == nil)
        #expect(!draft.hasEndDate)
    }

    // MARK: - A saved schedule

    @Test("Every cadence survives the round trip, its interval and end date with it")
    func roundTrip() {
        let schedules = [
            RecurringFixture.schedule(.daily, every: 3),
            RecurringFixture.schedule(.weekly(weekday: 5), every: 2),
            RecurringFixture.schedule(.weekly(weekday: 6), every: 2),
            RecurringFixture.schedule(.monthly(day: 27), from: "2026-06-27"),
            RecurringFixture.schedule(.monthly(day: 31)),
            RecurringFixture.schedule(.yearly(month: 3, day: 15), until: "2030-12-31"),
            RecurringFixture.schedule(.daily, from: "2026-01-01", until: "2026-01-01"),
        ]
        for schedule in schedules {
            let draft = ScheduleDraft(schedule: schedule)
            #expect(draft.schedule == schedule)
            #expect(draft.hasEndDate == (schedule.endDate != nil))
            #expect(draft.isValid)
        }
    }

    @Test("A saved schedule's other cadences start from its start date")
    func savedSeedsTheOtherCadences() {
        let weekly = ScheduleDraft(schedule: RecurringFixture.schedule(.weekly(weekday: 2), every: 2, from: "2026-06-27"))
        #expect(weekly.cadence == .weekly)
        #expect(weekly.interval == 2)
        #expect(weekly.weekday == 2)
        #expect(weekly.monthDay == 27)
        #expect(weekly.yearMonth == 6)
        #expect(weekly.yearDay == 27)

        let yearly = ScheduleDraft(schedule: RecurringFixture.schedule(.yearly(month: 3, day: 15), from: "2026-06-27"))
        #expect(yearly.cadence == .yearly)
        // Saturday 27 June.
        #expect(yearly.weekday == 6)
        #expect(yearly.monthDay == 27)
        #expect(yearly.yearMonth == 3)
        #expect(yearly.yearDay == 15)
    }

    // MARK: - What the core would take

    @Test("An interval of zero, a day out of range or an end before the start are not a schedule")
    func validity() {
        var draft = ScheduleDraft(today: "2026-10-10")
        draft.interval = 0
        #expect(!draft.isValid)
        draft.interval = 2
        draft.monthDay = 32
        #expect(!draft.isValid)
        draft.monthDay = 31
        #expect(draft.isValid)
        draft.cadence = .weekly
        draft.weekday = 8
        #expect(!draft.isValid)
        draft.weekday = 7
        draft.endDate = "2026-10-01"
        #expect(!draft.isValid)
    }

    @Test("Each cadence checks its own days, and only those")
    func validityOfEachCadence() {
        var draft = ScheduleDraft(today: "2026-10-10")
        draft.weekday = 0
        draft.monthDay = 0
        draft.yearMonth = 13
        draft.yearDay = 32

        draft.cadence = .daily
        #expect(draft.isValid)
        draft.cadence = .weekly
        #expect(!draft.isValid)
        draft.weekday = 1
        #expect(draft.isValid)
        draft.cadence = .monthly
        #expect(!draft.isValid)
        draft.monthDay = 1
        #expect(draft.isValid)
        draft.cadence = .yearly
        #expect(!draft.isValid)
        draft.yearMonth = 12
        #expect(!draft.isValid)
        draft.yearDay = 31
        #expect(draft.isValid)
        draft.yearMonth = 0
        #expect(!draft.isValid)
    }

    @Test("An end on the start day is a schedule, one before it is not, and an end switched off does not count")
    func validityOfTheEnd() {
        var draft = ScheduleDraft(today: "2026-10-10")
        draft.hasEndDate = true
        #expect(draft.endDate == "2026-10-10")
        #expect(draft.isValid)
        draft.shownEndDate = "2026-10-09"
        #expect(!draft.isValid)
        draft.hasEndDate = false
        #expect(draft.isValid)
    }

    @Test("Whatever isValid says of a schedule, the core says too")
    func validMeansTheCoreAgrees() {
        let edits: [(inout ScheduleDraft) -> Void] = [
            { _ in },
            { $0.interval = 0 },
            { $0.interval = -1 },
            { $0.monthDay = 32 },
            { $0.monthDay = 300 },
            { $0.cadence = .daily; $0.monthDay = 0 },
            { $0.cadence = .weekly; $0.weekday = 7 },
            { $0.cadence = .weekly; $0.weekday = 8 },
            { $0.cadence = .yearly; $0.yearMonth = 12; $0.yearDay = 31 },
            { $0.cadence = .yearly; $0.yearMonth = 13 },
            { $0.cadence = .yearly; $0.yearDay = 0 },
            { $0.endDate = "2026-10-07" },
            { $0.endDate = "2026-10-06" },
        ]
        for edit in edits {
            var draft = ScheduleDraft(today: "2026-10-07")
            edit(&draft)
            let accepted = (try? RecurringFixture.core(draft.schedule, draft.startDate, 1)) != nil
            #expect(draft.isValid == accepted, "\(draft.schedule)")
        }
    }

    // MARK: - What is sent

    @Test("Each cadence sends its own frequency, with the day fields of that cadence")
    func scheduleOfEachCadence() {
        var draft = ScheduleDraft(today: "2026-10-07")
        draft.interval = 2
        draft.weekday = 6
        draft.monthDay = 27
        draft.yearMonth = 3
        draft.yearDay = 15

        draft.cadence = .daily
        #expect(draft.frequency == .daily)
        #expect(draft.schedule == RecurringFixture.schedule(.daily, every: 2, from: "2026-10-07"))
        draft.cadence = .weekly
        #expect(draft.schedule == RecurringFixture.schedule(.weekly(weekday: 6), every: 2, from: "2026-10-07"))
        draft.cadence = .monthly
        #expect(draft.schedule == RecurringFixture.schedule(.monthly(day: 27), every: 2, from: "2026-10-07"))
        draft.cadence = .yearly
        #expect(draft.schedule == RecurringFixture.schedule(.yearly(month: 3, day: 15), every: 2, from: "2026-10-07"))

        draft.endDate = "2027-12-31"
        #expect(draft.schedule == RecurringFixture.schedule(
            .yearly(month: 3, day: 15), every: 2, from: "2026-10-07", until: "2027-12-31"
        ))
    }

    @Test("What is out of range goes through as typed, clamped to fit, for isValid and the core to refuse")
    func outOfRangeGoesThrough() {
        var draft = ScheduleDraft(today: "2026-10-07")
        draft.interval = -3
        draft.monthDay = 300
        #expect(draft.schedule == RecurringFixture.schedule(.monthly(day: 255), every: 0, from: "2026-10-07"))
        #expect(!draft.isValid)
    }

    // MARK: - The End field

    @Test("Switched off, the End field keeps its day for when it comes back on")
    func endField() {
        var draft = ScheduleDraft(today: "2026-10-07")
        draft.hasEndDate = true
        draft.shownEndDate = "2027-06-30"
        #expect(draft.endDate == "2027-06-30")

        draft.hasEndDate = false
        #expect(draft.schedule.endDate == nil)
        #expect(draft.shownEndDate == "2027-06-30")

        // A day picked while it says Mai waits there too.
        draft.shownEndDate = "2027-12-31"
        #expect(draft.endDate == nil)
        draft.hasEndDate = true
        #expect(draft.schedule.endDate == "2027-12-31")
    }

    @Test("A saved end reads as the End field's day, and stays there when switched off")
    func savedEnd() {
        var draft = ScheduleDraft(schedule: RecurringFixture.schedule(.monthly(day: 1), until: "2030-12-01"))
        #expect(draft.hasEndDate)
        #expect(draft.shownEndDate == "2030-12-01")
        draft.hasEndDate = false
        #expect(draft.shownEndDate == "2030-12-01")
        #expect(draft.isValid)
    }

    @Test("Until the End field sets a day it offers the start, wherever the start moves")
    func endFieldFollowsTheStartUntilSet() {
        var draft = ScheduleDraft(today: "2026-10-07")
        #expect(draft.shownEndDate == "2026-10-07")
        draft.startDate = "2026-11-01"
        #expect(draft.shownEndDate == "2026-11-01")

        draft.shownEndDate = "2026-12-01"
        draft.startDate = "2026-11-15"
        #expect(draft.shownEndDate == "2026-12-01")
    }

    @Test("Drafts of one schedule are equal, from today or saved, until the End field keeps a day of its own")
    func equality() {
        let saved = RecurringFixture.schedule(.monthly(day: 10), from: "2026-10-10")
        #expect(ScheduleDraft(today: "2026-10-10") == ScheduleDraft(schedule: saved))

        var touched = ScheduleDraft(schedule: saved)
        touched.shownEndDate = "2026-12-31"
        #expect(touched.schedule == saved)
        #expect(touched != ScheduleDraft(schedule: saved))
    }
}
