import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Where the copy of the other kind starts (`RecurringKindChange`), and what
/// it is created with. The dates are the core's own.
struct RecurringKindChangeTests {
    /// The first `count` periods of `schedule` on or after `from`.
    private static func dates(_ schedule: Schedule, from: NaiveDate, count: UInt32 = 6) throws -> [NaiveDate] {
        try RecurringFixture.core(schedule, from, count)
    }

    // MARK: - The start

    @Test("With nothing decided, the copy starts on the schedule's first period")
    func nothingDecided() {
        let schedule = RecurringFixture.schedule(.monthly(day: 15), from: "2026-01-10")
        #expect(RecurringKindChange.start(of: schedule, decided: []) == "2026-01-15")
    }

    @Test("The copy starts on the first period after the latest one decided, a gap before it left behind")
    func afterTheLatestDecided() {
        let schedule = RecurringFixture.schedule(.monthly(day: 1), from: "2026-01-01")
        #expect(RecurringKindChange.start(of: schedule, decided: ["2026-01-01", "2026-02-01", "2026-03-01"]) == "2026-04-01")
        #expect(RecurringKindChange.start(of: schedule, decided: ["2026-03-01", "2026-01-01"]) == "2026-04-01")
    }

    @Test("A day decided that the schedule no longer has, edited since, is followed by its next period")
    func decidedOffTheSchedule() {
        let schedule = RecurringFixture.schedule(.monthly(day: 15), from: "2026-01-01")
        #expect(RecurringKindChange.start(of: schedule, decided: ["2026-03-01"]) == "2026-03-15")
    }

    @Test("No start once the periods run out before the end date")
    func nothingLeft() {
        let ended = RecurringFixture.schedule(.monthly(day: 1), from: "2026-01-01", until: "2026-06-30")
        #expect(RecurringKindChange.start(of: ended, decided: ["2026-06-01"]) == nil)
        #expect(RecurringKindChange.start(of: ended, decided: ["2026-05-01"]) == "2026-06-01")
        let none = RecurringFixture.schedule(.monthly(day: 15), from: "2026-01-10", until: "2026-01-12")
        #expect(RecurringKindChange.start(of: none, decided: []) == nil)
    }

    @Test("The copy's series is the template's from its start, every second month and the month's end included")
    func sameSeries() throws {
        let cases: [(Schedule, Set<NaiveDate>, NaiveDate)] = [
            // Every two months on the 31st: the 31st, or the last day of a
            // shorter month, never a day that drifted.
            (RecurringFixture.schedule(.monthly(day: 31), every: 2, from: "2026-01-31"), ["2026-01-31", "2026-03-31"], "2026-05-31"),
            (RecurringFixture.schedule(.monthly(day: 31), from: "2026-11-30"), ["2026-12-31", "2027-01-31"], "2027-02-28"),
            (RecurringFixture.schedule(.weekly(weekday: 3), every: 2, from: "2026-10-07"), ["2026-10-07"], "2026-10-21"),
            (RecurringFixture.schedule(.daily, every: 3, from: "2026-10-01"), ["2026-10-01", "2026-10-04"], "2026-10-07"),
            (RecurringFixture.schedule(.yearly(month: 2, day: 29), from: "2028-02-29"), ["2028-02-29"], "2029-02-28"),
        ]
        for (schedule, decided, expected) in cases {
            let start = try #require(RecurringKindChange.start(of: schedule, decided: decided))
            #expect(start == expected, "\(schedule)")
            let copy = RecurringKindChange.schedule(schedule, startingOn: start)
            #expect(try Self.dates(copy, from: start) == Self.dates(schedule, from: start), "\(schedule)")
        }
    }

    @Test("The copy's schedule moves the start only")
    func scheduleKeepsTheRest() {
        let schedule = RecurringFixture.schedule(.weekly(weekday: 5), every: 2, from: "2026-01-02", until: "2027-01-01")
        #expect(RecurringKindChange.schedule(schedule, startingOn: "2026-10-16")
            == RecurringFixture.schedule(.weekly(weekday: 5), every: 2, from: "2026-10-16", until: "2027-01-01"))
    }

    // MARK: - The copy

    @Test("The copy is the other kind, with the same amount, wallet, envelope, category, note and cadence")
    func copyKeepsTheFields() {
        let rent = RecurringFixture.template(
            "rent",
            kind: .expense,
            amount: 78_000,
            walletId: "wallet-conto",
            flowId: "flow-casa",
            category: "Casa",
            note: "mutuo",
            schedule: RecurringFixture.schedule(.monthly(day: 1), every: 2, from: "2025-11-01", until: "2045-10-01"),
            owner: "elisa"
        )
        let copy = RecurringKindChange.copy(of: rent, startingOn: "2026-11-01", owner: "elisa")
        #expect(copy == .createRecurring(
            transactionKind: .income,
            amount: 78_000,
            walletId: "wallet-conto",
            flowId: "flow-casa",
            category: "Casa",
            note: "mutuo",
            schedule: RecurringFixture.schedule(.monthly(day: 1), every: 2, from: "2026-11-01", until: "2045-10-01"),
            owner: "elisa"
        ))
        #expect(RecurringKindChange.otherKind(of: .income) == .expense)
        #expect(RecurringKindChange.otherKind(of: .expense) == .income)
    }
}
