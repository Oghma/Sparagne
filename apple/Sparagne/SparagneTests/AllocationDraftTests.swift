import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The drafts behind the Riparto tab: what a percentage or an amount typed
/// in a cell reads as, what an open row commits into the plan's list, and
/// what the inspector's schedule sends. Plain structs, so none of this needs
/// a view or a vault.
struct AllocationDraftTests {
    private static let rent: Uuid = "flow-affitto"
    private static let savings: Uuid = "flow-risparmi"
    private static let food: Uuid = "flow-spesa"

    /// Affitto 800,00, Risparmi 10%, Spesa up to the cap.
    private static let lines = [
        AllocationLine(flowId: rent, rule: .fixed(amount: 80_000)),
        AllocationLine(flowId: savings, rule: .percent(basisPoints: 1_000)),
        AllocationLine(flowId: food, rule: .fillToCap),
    ]

    // MARK: - Percentages

    @Test("A percentage reads with either decimal separator, up to two decimals, the sign allowed")
    func percentParses() {
        #expect(AllocationPercent.parse("10") == 1_000)
        #expect(AllocationPercent.parse("12,5") == 1_250)
        #expect(AllocationPercent.parse("12.5") == 1_250)
        #expect(AllocationPercent.parse("12,25") == 1_225)
        #expect(AllocationPercent.parse(" 12,5 % ") == 1_250)
        #expect(AllocationPercent.parse("0,01") == 1)
        #expect(AllocationPercent.parse(",5") == 50)
        #expect(AllocationPercent.parse("100") == 10_000)
    }

    @Test("Zero, more than 100% and anything that is not a number are no percentage")
    func percentRefuses() {
        #expect(AllocationPercent.basisPoints("0") == 0)
        #expect(AllocationPercent.parse("0") == nil)
        #expect(AllocationPercent.basisPoints("101") == 10_100)
        #expect(AllocationPercent.parse("101") == nil)
        #expect(AllocationPercent.parse("100,01") == nil)
        for garbage in ["", " ", "%", "abc", "1,2,3", "12,345", "-5", "+5", "1e3", "１０", ".", "12,5,"] {
            #expect(AllocationPercent.parse(garbage) == nil, "\(garbage)")
        }
    }

    @Test("A percentage is written with the decimal comma and no trailing zeros")
    func percentText() {
        #expect(AllocationPercent.text(1_000) == "10")
        #expect(AllocationPercent.text(1_250) == "12,5")
        #expect(AllocationPercent.text(1_225) == "12,25")
        #expect(AllocationPercent.text(1_205) == "12,05")
        #expect(AllocationPercent.text(1) == "0,01")
        #expect(AllocationPercent.text(10_000) == "100")
        #expect(AllocationPercent.label(1_500) == "15%")
        for basisPoints: UInt32 in [1, 5, 50, 999, 1_250, 3_333, 10_000] {
            #expect(AllocationPercent.parse(AllocationPercent.text(basisPoints)) == basisPoints)
        }
    }

    // MARK: - Money

    @Test("An amount reads as the window writes it, with dots between thousands, or plain")
    func money() {
        #expect(AllocationMoney.amount("4.250,00", currency: .eur) == 425_000)
        #expect(AllocationMoney.amount("1.234.567,89", currency: .eur) == 123_456_789)
        #expect(AllocationMoney.amount("4250,00", currency: .eur) == 425_000)
        #expect(AllocationMoney.amount("4250.5", currency: .eur) == 425_050)
        #expect(AllocationMoney.amount("1.40", currency: .eur) == 140)
        #expect(AllocationMoney.amount("4250", currency: .eur) == 425_000)
        #expect(AllocationMoney.amount(" 0 ", currency: .eur) == 0)
        #expect(AllocationMoney.amount("abc", currency: .eur) == nil)
        #expect(AllocationMoney.amount("", currency: .eur) == nil)
        #expect(AllocationMoney.amount(LedgerMoney.bare(425_000), currency: .eur) == 425_000)
    }

    @Test("Without a comma, dots that each start three digits are thousands, as an Italian 1.400 is")
    func thousandsWithoutDecimals() {
        #expect(AllocationMoney.amount("1.400", currency: .eur) == 140_000)
        #expect(AllocationMoney.amount("12.500.000", currency: .eur) == 1_250_000_000)
        #expect(AllocationMoney.amount("-1.400", currency: .eur) == -140_000)
        // Not thousands: the first group is too long, or a group is not three
        // digits, so the dot is a decimal point with too many decimals.
        #expect(AllocationMoney.amount("1234.567", currency: .eur) == nil)
        #expect(AllocationMoney.amount("1.4000", currency: .eur) == nil)
        #expect(AllocationMoney.amount("1.400.5", currency: .eur) == nil)
        #expect(AllocationMoney.amount(".400", currency: .eur) == nil)
    }

    // MARK: - A line

    @Test("A saved line opens with its own values, and commits back unchanged")
    func lineRoundTrip() throws {
        for line in Self.lines {
            let draft = AllocationLineDraft(line: line)
            #expect(try draft.line(currency: .eur) == line)
            #expect(try draft.committing(into: Self.lines, replacing: line.flowId, currency: .eur) == Self.lines)
        }
        #expect(AllocationLineDraft(line: Self.lines[0]).amount == "800,00")
        #expect(AllocationLineDraft(line: Self.lines[1]).percent == "10")
    }

    @Test("Each kind reads its own value; switching kinds keeps the other's")
    func kinds() throws {
        var draft = AllocationLineDraft()
        draft.flowId = Self.rent
        draft.value = "800"
        #expect(try draft.line(currency: .eur).rule == .fixed(amount: 80_000))
        draft.kind = .percent
        #expect(draft.value == "")
        draft.value = "12,5"
        #expect(try draft.line(currency: .eur).rule == .percent(basisPoints: 1_250))
        draft.kind = .fillToCap
        #expect(draft.value == "")
        draft.value = "ignored"
        #expect(try draft.line(currency: .eur).rule == .fillToCap)
        draft.kind = .fixed
        #expect(draft.value == "800")
        #expect(draft.percent == "12,5")
    }

    @Test("A line without an envelope, or with a value its rule cannot take, names the cell at fault")
    func refusals() {
        var draft = AllocationLineDraft()
        draft.amount = "800"
        #expect(Self.field(of: draft) == .envelope)

        draft.flowId = Self.rent
        for amount in ["", "0", "-5", "abc", "1,234"] {
            draft.amount = amount
            #expect(Self.field(of: draft) == .value, "\(amount)")
            #expect(!draft.isValid(currency: .eur))
        }
        draft.kind = .percent
        for percent in ["", "0", "101", "abc"] {
            draft.percent = percent
            #expect(Self.field(of: draft) == .value, "\(percent)")
        }
        draft.percent = "5"
        #expect(draft.isValid(currency: .eur))
    }

    @Test("An untouched empty row is blank; a picked envelope or a typed value is not")
    func blank() {
        var draft = AllocationLineDraft()
        #expect(draft.isBlank)
        draft.amount = "  "
        #expect(draft.isBlank)
        draft.amount = "5"
        #expect(!draft.isBlank)
        var picked = AllocationLineDraft()
        picked.flowId = Self.rent
        #expect(!picked.isBlank)
    }

    @Test("The empty row goes to the end of the list, an open row stays in its place")
    func committing() throws {
        let holiday: Uuid = "flow-vacanze"
        var newLine = AllocationLineDraft()
        newLine.flowId = holiday
        newLine.kind = .percent
        newLine.percent = "5"
        let appended = try newLine.committing(into: Self.lines, replacing: nil, currency: .eur)
        #expect(appended == Self.lines + [AllocationLine(flowId: holiday, rule: .percent(basisPoints: 500))])

        var edited = AllocationLineDraft(line: Self.lines[1])
        edited.percent = "12,5"
        let replaced = try edited.committing(into: Self.lines, replacing: Self.savings, currency: .eur)
        #expect(replaced.map(\.flowId) == Self.lines.map(\.flowId))
        #expect(replaced[1].rule == .percent(basisPoints: 1_250))

        // Another envelope for the same row: still in its place.
        edited.flowId = holiday
        let moved = try edited.committing(into: Self.lines, replacing: Self.savings, currency: .eur)
        #expect(moved.map(\.flowId) == [Self.rent, holiday, Self.food])

        // The first line of a plan.
        #expect(try newLine.committing(into: [], replacing: nil, currency: .eur).count == 1)
    }

    @Test("An envelope is in a plan once: another line's is refused on the envelope cell")
    func duplicateEnvelope() {
        var newLine = AllocationLineDraft()
        newLine.flowId = Self.rent
        newLine.amount = "100"
        #expect(Self.field { try newLine.committing(into: Self.lines, replacing: nil, currency: .eur) } == .envelope)

        var edited = AllocationLineDraft(line: Self.lines[1])
        edited.flowId = Self.food
        #expect(Self.field { try edited.committing(into: Self.lines, replacing: Self.savings, currency: .eur) } == .envelope)
    }

    private static func field(of draft: AllocationLineDraft) -> AllocationField? {
        field { try draft.line(currency: .eur) }
    }

    private static func field<T>(_ work: () throws -> T) -> AllocationField? {
        do {
            _ = try work()
            return nil
        } catch let failure as AllocationDraftError {
            return failure.field
        } catch {
            return nil
        }
    }

    // MARK: - The schedule

    @Test("A new plan is monthly, on today's day, from today")
    func newSchedule() {
        let draft = AllocationScheduleDraft(today: "2026-10-10")
        #expect(draft.schedule == RecurringFixture.schedule(.monthly(day: 10), from: "2026-10-10"))
        #expect(draft.isValid)
        // Saturday, for a switch to weekly.
        #expect(draft.weekday == 6)
    }

    @Test("Every cadence survives the round trip, its end date with it")
    func scheduleRoundTrip() {
        let schedules = [
            RecurringFixture.schedule(.daily, every: 3),
            RecurringFixture.schedule(.weekly(weekday: 5), every: 2),
            RecurringFixture.schedule(.monthly(day: 27), from: "2026-06-27"),
            RecurringFixture.schedule(.yearly(month: 3, day: 15), until: "2030-12-31"),
        ]
        for schedule in schedules {
            #expect(AllocationScheduleDraft(schedule: schedule).schedule == schedule)
        }
    }

    @Test("An interval of zero, a day out of range or an end before the start are not a schedule")
    func scheduleValidity() {
        var draft = AllocationScheduleDraft(today: "2026-10-10")
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

    @Test("The inspector sends the schedule only when it changed")
    func schedulePatch() {
        let plan = AllocationPlanView(
            id: "plan",
            schedule: RecurringFixture.schedule(.monthly(day: 27), from: "2026-06-27"),
            lines: Self.lines,
            enabled: true,
            createdBy: "matteo"
        )
        var draft = AllocationScheduleDraft(schedule: plan.schedule)
        #expect(draft.patch(against: plan).isEmpty)
        #expect(!draft.isDirty(against: plan))

        draft.cadence = .weekly
        #expect(draft.isDirty(against: plan))
        // Back to the saved cadence: its day was kept.
        draft.cadence = .monthly
        #expect(!draft.isDirty(against: plan))

        draft.monthDay = 1
        draft.startDate = "2026-11-01"
        let patch = draft.patch(against: plan)
        #expect(patch.schedule == RecurringFixture.schedule(.monthly(day: 1), from: "2026-11-01"))
        #expect(patch.lines == nil)
        #expect(patch.enabled == nil)
    }
}
