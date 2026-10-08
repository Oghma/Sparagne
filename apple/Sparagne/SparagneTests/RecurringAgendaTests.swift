import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Templates made up for the Ricorrenze tab's pure models, without a vault.
enum RecurringFixture {
    static func schedule(
        _ frequency: Frequency = .monthly(day: 1),
        every interval: UInt32 = 1,
        from start: NaiveDate = "2026-01-01",
        until end: NaiveDate? = nil
    ) -> Schedule {
        Schedule(frequency: frequency, interval: interval, startDate: start, endDate: end)
    }

    static func template(
        _ id: Uuid = UUID().uuidString,
        kind: TransactionKind = .expense,
        amount: Int64 = 1_000,
        walletId: Uuid? = nil,
        flowId: Uuid? = nil,
        category: String? = "Casa",
        note: String? = "mutuo",
        schedule: Schedule = schedule(),
        enabled: Bool = true,
        archived: Bool = false
    ) -> RecurringView {
        RecurringView(
            id: id,
            kind: kind,
            amount: amount,
            walletId: walletId,
            flowId: flowId,
            category: category,
            note: note,
            schedule: schedule,
            enabled: enabled,
            archived: archived,
            owner: "matteo"
        )
    }

    /// The core's own dates.
    static func core(_ schedule: Schedule, _ from: NaiveDate, _ limit: UInt32) throws -> [NaiveDate] {
        try CoreSchedule.occurrences(schedule, from, limit)
    }
}

struct NaiveDayTests {
    @Test("Day arithmetic crosses months, years and leap days")
    func adding() {
        #expect(NaiveDay.adding(1, to: "2026-10-07") == "2026-10-08")
        #expect(NaiveDay.adding(30, to: "2026-10-07") == "2026-11-06")
        #expect(NaiveDay.adding(1, to: "2026-12-31") == "2027-01-01")
        #expect(NaiveDay.adding(1, to: "2028-02-28") == "2028-02-29")
        #expect(NaiveDay.adding(-1, to: "2026-03-01") == "2026-02-28")
        // The last Sunday of March, when Italy loses an hour.
        #expect(NaiveDay.adding(1, to: "2026-03-29") == "2026-03-30")
        #expect(NaiveDay.adding(1, to: "not a day") == nil)
    }

    @Test("Whole days between two dates")
    func days() {
        #expect(NaiveDay.days(from: "2026-10-01", to: "2026-10-07") == 6)
        #expect(NaiveDay.days(from: "2026-10-07", to: "2026-10-07") == 0)
        #expect(NaiveDay.days(from: "2026-10-07", to: "2026-10-01") == -6)
    }

    @Test("The parts of a day, weekday counted from Monday")
    func components() throws {
        let thursday = try #require(NaiveDay.components("2026-10-01"))
        #expect(thursday.year == 2026)
        #expect(thursday.month == 10)
        #expect(thursday.day == 1)
        #expect(thursday.isoWeekday == 4)
        #expect(NaiveDay.components("2026-10-04")?.isoWeekday == 7)
        #expect(NaiveDay.components("2026-10-05")?.isoWeekday == 1)
    }
}

struct RecurringAgendaTests {
    private static let today: NaiveDate = "2026-10-07"

    @Test("The window runs from tomorrow to thirty days from today, both included")
    func windowBounds() {
        var asked: [(NaiveDate, UInt32)] = []
        let agenda = RecurringAgenda.build(
            templates: [RecurringFixture.template()],
            today: Self.today,
            days: 30
        ) { _, from, limit in
            asked.append((from, limit))
            // More than the core would hand back: today, the window's two
            // ends and the day after it.
            return ["2026-10-07", "2026-10-08", "2026-10-20", "2026-11-06", "2026-11-07"]
        }

        #expect(asked.count == 1)
        #expect(asked.first?.0 == "2026-10-08")
        #expect(asked.first?.1 == 31)
        #expect(agenda.tiles.map(\.date) == ["2026-10-08", "2026-10-20", "2026-11-06"])
    }

    @Test("A daily template fills every day of the window once, and never today")
    func dailyWithTheCore() {
        let daily = RecurringFixture.template(schedule: RecurringFixture.schedule(.daily, from: "2026-09-01"))
        let agenda = RecurringAgenda.build(
            templates: [daily],
            today: Self.today,
            days: 30,
            occurrences: RecurringFixture.core
        )

        #expect(agenda.tiles.count == 30)
        #expect(agenda.tiles.first?.date == "2026-10-08")
        #expect(agenda.tiles.last?.date == "2026-11-06")
        #expect(!agenda.tiles.contains { $0.date == Self.today })
    }

    @Test("Totals add expenses and income apart")
    func totals() {
        let rent = RecurringFixture.template(amount: 78_000, note: "mutuo", schedule: RecurringFixture.schedule(.monthly(day: 1)))
        let netflix = RecurringFixture.template(amount: 1_299, note: "Netflix", schedule: RecurringFixture.schedule(.monthly(day: 12)))
        let salary = RecurringFixture.template(
            kind: .income,
            amount: 240_000,
            note: "Stipendio",
            schedule: RecurringFixture.schedule(.monthly(day: 27))
        )
        let agenda = RecurringAgenda.build(
            templates: [rent, netflix, salary],
            today: Self.today,
            occurrences: RecurringFixture.core
        )

        // 12 Oct Netflix, 27 Oct the salary, 1 Nov the rent.
        #expect(agenda.tiles.map(\.date) == ["2026-10-12", "2026-10-27", "2026-11-01"])
        #expect(agenda.expenses == 78_000 + 1_299)
        #expect(agenda.income == 240_000)
    }

    @Test("Disabled and archived templates have no future")
    func pausedAndArchivedAreLeftOut() {
        let running = RecurringFixture.template(note: "running")
        let paused = RecurringFixture.template(note: "paused", enabled: false)
        let archived = RecurringFixture.template(note: "archived", archived: true)
        let agenda = RecurringAgenda.build(
            templates: [running, paused, archived],
            today: Self.today,
            occurrences: RecurringFixture.core
        )

        #expect(agenda.tiles.map(\.template.note) == ["running"])
        #expect(agenda.expenses == running.amount)
    }

    @Test("A schedule the core refuses leaves out that template, not the agenda")
    func refusedScheduleIsSkipped() {
        let broken = RecurringFixture.template(note: "broken", schedule: RecurringFixture.schedule(every: 0))
        let fine = RecurringFixture.template(note: "fine")
        let agenda = RecurringAgenda.build(
            templates: [broken, fine],
            today: Self.today,
            occurrences: RecurringFixture.core
        )

        #expect(agenda.tiles.map(\.template.note) == ["fine"])
    }

    @Test("The same day is ordered by title")
    func sameDayByTitle() {
        let rent = RecurringFixture.template(note: "mutuo", schedule: RecurringFixture.schedule(.monthly(day: 1)))
        let salary = RecurringFixture.template(kind: .income, note: "Stipendio Elisa", schedule: RecurringFixture.schedule(.monthly(day: 1)))
        let agenda = RecurringAgenda.build(
            templates: [salary, rent],
            today: Self.today,
            occurrences: RecurringFixture.core
        )

        #expect(agenda.tiles.map(\.template.note) == ["mutuo", "Stipendio Elisa"])
    }

    @Test("A template is called by its note, then its category")
    func titles() {
        #expect(RecurringTitle.of(RecurringFixture.template(category: "Casa", note: "mutuo")) == "mutuo")
        #expect(RecurringTitle.of(RecurringFixture.template(category: "Casa", note: " ")) == "Casa")
        #expect(RecurringTitle.of(RecurringFixture.template(category: nil, note: nil)) == TransactionRow.placeholder)
    }
}

/// `AppStore.loadUpcomingRecurring` on a real vault.
@MainActor
struct UpcomingRecurringStoreTests {
    @Test("The store's agenda is the running templates' next thirty days")
    func storeAgenda() async throws {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.upcoming.\(UUID().uuidString)"))
        let store = AppStore(core: try CoreActor.inMemory(author: "tester"), defaults: defaults)
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Cash", openingBalance: 10_000)
        let today = CoreDate.day(Date())
        await store.createRecurring(
            kind: .expense,
            amount: 500,
            walletId: nil,
            flowId: nil,
            category: "Bills",
            note: "Weekly",
            schedule: Schedule(frequency: .daily, interval: 7, startDate: today, endDate: nil)
        )
        #expect(store.presentedError == nil)

        await store.loadUpcomingRecurring()

        // Today is due, so the agenda starts a week from now, and four
        // weeks fit in the thirty days.
        let dates = store.upcomingRecurring.map(\.date)
        #expect(dates.first == NaiveDay.adding(7, to: today))
        #expect(dates.count == 4)
        #expect(!dates.contains(today))
        #expect(store.duePeriods.map(\.date) == [today])
    }

    @Test("The table's next dates are worked out per template: the oldest due period, or the first one ahead")
    func storeNextDates() async throws {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.upcoming.\(UUID().uuidString)"))
        let store = AppStore(core: try CoreActor.inMemory(author: "tester"), defaults: defaults)
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Cash", openingBalance: 10_000)
        let today = CoreDate.day(Date())
        let twoDaysAgo = try #require(NaiveDay.adding(-2, to: today))
        let inAWeek = try #require(NaiveDay.adding(7, to: today))
        let rent = try #require(await store.createRecurring(
            kind: .expense,
            amount: 500,
            walletId: nil,
            flowId: nil,
            category: "Bills",
            note: "Rent",
            schedule: Schedule(frequency: .daily, interval: 1, startDate: twoDaysAgo, endDate: nil)
        ))
        let gym = try #require(await store.createRecurring(
            kind: .expense,
            amount: 300,
            walletId: nil,
            flowId: nil,
            category: "Sport",
            note: "Gym",
            schedule: Schedule(frequency: .daily, interval: 7, startDate: inAWeek, endDate: nil)
        ))
        #expect(store.presentedError == nil)

        let next = store.nextRecurring(today: today)

        #expect(next.count == 2)
        #expect(next[rent] == .due(twoDaysAgo))
        #expect(next[gym] == .upcoming(inAWeek))
    }
}
