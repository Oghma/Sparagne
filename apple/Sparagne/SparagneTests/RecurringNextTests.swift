import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The templates table's Prossima and the inspector's next dates.
struct RecurringNextTests {
    private static let today: NaiveDate = "2026-10-07"

    @Test("A due period beats any future date, the oldest one first")
    func dueBeatsFuture() {
        let rent = RecurringFixture.template(schedule: RecurringFixture.schedule(.monthly(day: 1)))
        let next = RecurringNext.of(
            rent,
            due: ["2026-10-01", "2026-09-01"],
            today: Self.today,
            occurrences: RecurringFixture.core
        )
        #expect(next == .due("2026-09-01"))
    }

    @Test("With nothing due, the first period after today")
    func upcoming() {
        let netflix = RecurringFixture.template(schedule: RecurringFixture.schedule(.monthly(day: 12)))
        let next = RecurringNext.of(netflix, due: [], today: Self.today, occurrences: RecurringFixture.core)
        #expect(next == .upcoming("2026-10-12"))
    }

    @Test("Today's period, recorded already, is not next; a paused template's still is")
    func todayRecordedOrPaused() {
        let schedule = RecurringFixture.schedule(.monthly(day: 7))
        let running = RecurringFixture.template(schedule: schedule)
        let paused = RecurringFixture.template(schedule: schedule, enabled: false)

        #expect(RecurringNext.of(running, due: [], today: Self.today, occurrences: RecurringFixture.core)
            == .upcoming("2026-11-07"))
        #expect(RecurringNext.of(paused, due: [], today: Self.today, occurrences: RecurringFixture.core)
            == .upcoming("2026-10-07"))
    }

    @Test("Nothing is left after the end date")
    func finishedAfterEndDate() {
        let ended = RecurringFixture.template(
            schedule: RecurringFixture.schedule(.monthly(day: 1), from: "2026-01-01", until: "2026-06-30")
        )
        let next = RecurringNext.of(ended, due: [], today: Self.today, occurrences: RecurringFixture.core)
        #expect(next == .finished)
    }

    @Test("An archived template has no next period, even with periods once due")
    func archived() {
        let gym = RecurringFixture.template(archived: true)
        let next = RecurringNext.of(gym, due: ["2026-10-01"], today: Self.today, occurrences: RecurringFixture.core)
        #expect(next == .archived)
    }

    @Test("The preview lists the due periods first, tagged, then the dates ahead")
    func previewDueFirst() throws {
        let schedule = RecurringFixture.schedule(.monthly(day: 1), from: "2025-11-01")
        let preview = try RecurringNext.preview(
            schedule: schedule,
            due: ["2026-10-01"],
            from: "2026-10-08",
            count: 4,
            occurrences: RecurringFixture.core
        )
        #expect(preview == [
            .init(date: "2026-10-01", isDue: true),
            .init(date: "2026-11-01", isDue: false),
            .init(date: "2026-12-01", isDue: false),
            .init(date: "2027-01-01", isDue: false),
        ])
    }

    @Test("A due period of today is listed once")
    func previewTodayOnce() throws {
        let schedule = RecurringFixture.schedule(.monthly(day: 7))
        let preview = try RecurringNext.preview(
            schedule: schedule,
            due: [Self.today],
            from: Self.today,
            count: 4,
            occurrences: RecurringFixture.core
        )
        #expect(preview.map(\.date) == ["2026-10-07", "2026-11-07", "2026-12-07", "2027-01-07"])
        #expect(preview.map(\.isDue) == [true, false, false, false])
    }

    @Test("A start moved back to January makes every month since then due")
    func dueSinceStart() throws {
        let schedule = RecurringFixture.schedule(.monthly(day: 16), from: "2026-01-01")
        let due = try RecurringNext.due(schedule: schedule, today: Self.today, handled: [], occurrences: RecurringFixture.core)
        #expect(due == [
            "2026-01-16", "2026-02-16", "2026-03-16", "2026-04-16", "2026-05-16",
            "2026-06-16", "2026-07-16", "2026-08-16", "2026-09-16",
        ])
    }

    @Test("Periods recorded or skipped are not due again, and today's still is")
    func dueLeavesHandledOut() throws {
        let schedule = RecurringFixture.schedule(.monthly(day: 7), from: "2026-07-01")
        let due = try RecurringNext.due(
            schedule: schedule,
            today: Self.today,
            handled: ["2026-07-07", "2026-09-07"],
            occurrences: RecurringFixture.core
        )
        #expect(due == ["2026-08-07", "2026-10-07"])
    }

    @Test("Nothing is due past the end date, nor from a start still ahead")
    func dueBounds() throws {
        let ended = RecurringFixture.schedule(.monthly(day: 1), from: "2026-01-01", until: "2026-03-31")
        #expect(try RecurringNext.due(schedule: ended, today: Self.today, handled: [], occurrences: RecurringFixture.core)
            == ["2026-01-01", "2026-02-01", "2026-03-01"])
        let ahead = RecurringFixture.schedule(.monthly(day: 1), from: "2026-11-01")
        #expect(try RecurringNext.due(schedule: ahead, today: Self.today, handled: [], occurrences: RecurringFixture.core)
            .isEmpty)
    }

    @Test("The preview of an invalid schedule throws the core's refusal")
    func previewInvalid() {
        #expect(throws: DomainError.self) {
            try RecurringNext.preview(
                schedule: RecurringFixture.schedule(.monthly(day: 32)),
                due: [],
                from: Self.today,
                count: 4,
                occurrences: RecurringFixture.core
            )
        }
    }
}

/// The status line's "Uscite fisse al mese" and "Entrate fisse".
struct RecurringMonthlyTests {
    @Test("Each cadence over the average month")
    func perMonth() {
        #expect(RecurringMonthly.perMonth(RecurringFixture.schedule(.monthly(day: 1))) == 1)
        #expect(RecurringMonthly.perMonth(RecurringFixture.schedule(.monthly(day: 1), every: 2)) == Decimal(1) / 2)
        #expect(RecurringMonthly.perMonth(RecurringFixture.schedule(.yearly(month: 3, day: 15))) == Decimal(1) / 12)
        #expect(RecurringMonthly.perMonth(RecurringFixture.schedule(.daily)) == Decimal(string: "30.4375"))
        #expect(RecurringMonthly.perMonth(RecurringFixture.schedule(.weekly(weekday: 6))) == Decimal(string: "4.348125"))
        #expect(RecurringMonthly.perMonth(RecurringFixture.schedule(.weekly(weekday: 6), every: 2)) == Decimal(string: "2.1740625"))
    }

    @Test("Totals of the running templates, rounded to the cent once")
    func totals() {
        let templates = [
            // 780,00 a month.
            RecurringFixture.template(amount: 78_000, schedule: RecurringFixture.schedule(.monthly(day: 1))),
            // 60,00 every two weeks: 130,44375 a month.
            RecurringFixture.template(amount: 6_000, schedule: RecurringFixture.schedule(.weekly(weekday: 6), every: 2)),
            // 412,00 a year: 34,333… a month.
            RecurringFixture.template(amount: 41_200, schedule: RecurringFixture.schedule(.yearly(month: 3, day: 15))),
            // 1,00 a day: 30,4375 a month.
            RecurringFixture.template(amount: 100, schedule: RecurringFixture.schedule(.daily)),
            RecurringFixture.template(kind: .income, amount: 240_000),
            RecurringFixture.template(kind: .income, amount: 185_000),
            // Neither a paused nor an archived template counts.
            RecurringFixture.template(amount: 4_500, enabled: false),
            RecurringFixture.template(amount: 4_500, archived: true),
        ]
        let totals = RecurringMonthly.totals(templates)

        // 78000 + 13044.375 + 3433.333… + 3043.75 = 97521.458… cents.
        #expect(totals.expenses == 97_521)
        #expect(totals.income == 425_000)
    }

    @Test("Half a cent rounds up")
    func roundsHalfUp() {
        #expect(RecurringMonthly.cents(Decimal(string: "10.5")!) == 11)
        #expect(RecurringMonthly.cents(Decimal(string: "10.49")!) == 10)
        // 1,00 every two days: 15,21875 a month.
        let everyOtherDay = RecurringFixture.template(amount: 100, schedule: RecurringFixture.schedule(.daily, every: 2))
        #expect(RecurringMonthly.totals([everyOtherDay]).expenses == 1_522)
    }

    @Test("No templates, nothing a month")
    func empty() {
        #expect(RecurringMonthly.totals([]) == .init(expenses: 0, income: 0))
    }
}
