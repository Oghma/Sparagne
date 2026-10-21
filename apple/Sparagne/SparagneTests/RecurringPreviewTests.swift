import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The Ricorrenze inspector's next four dates (`RecurringPreview`): which
/// due periods it shows, from where it looks ahead, and what it counts.
struct RecurringPreviewTests {
    private static let today: NaiveDate = "2026-10-07"
    private static let monthly = RecurringFixture.schedule(.monthly(day: 1), from: "2026-07-01")

    private static func inputs(
        _ schedule: Schedule = monthly,
        template: RecurringView?,
        savedDue: [NaiveDate] = [],
        handled: Set<NaiveDate> = []
    ) -> RecurringPreview.Inputs {
        RecurringPreview.Inputs(schedule: schedule, template: template, savedDue: { _ in savedDue }, handled: handled, today: today)
    }

    @Test("The saved schedule reads the saved due periods; an edited one works them out, less those handled")
    func savedOrWorkedOut() throws {
        let saved = RecurringFixture.template(schedule: Self.monthly)
        let unchanged = Self.inputs(template: saved, savedDue: ["2026-10-01"])
        #expect(unchanged.savedDue == ["2026-10-01"])
        #expect(unchanged.running)
        let preview = try RecurringPreview.of(unchanged, occurrences: RecurringFixture.core).get()
        #expect(preview.lines.map(\.date) == ["2026-10-01", "2026-11-01", "2026-12-01", "2027-01-01"])
        #expect(preview.lines.map(\.isDue) == [true, false, false, false])
        #expect(preview.moreDue == 0)

        let moved = RecurringFixture.schedule(.monthly(day: 1), from: "2026-01-01")
        let edited = Self.inputs(moved, template: saved, savedDue: ["2026-10-01"], handled: ["2026-02-01"])
        #expect(edited.savedDue == nil)
        let worked = try RecurringPreview.of(edited, occurrences: RecurringFixture.core).get()
        // January and March to October are due; four lines show, five do not fit.
        #expect(worked.lines.map(\.date) == ["2026-01-01", "2026-03-01", "2026-04-01", "2026-05-01"])
        #expect(worked.moreDue == 5)
    }

    @Test("A paused or archived template has nothing due and looks from today; a new one runs")
    func pausedArchivedOrNew() throws {
        let schedule = RecurringFixture.schedule(.monthly(day: 7), from: "2026-07-01")
        let paused = Self.inputs(schedule, template: RecurringFixture.template(schedule: schedule, enabled: false))
        #expect(!paused.running)
        let preview = try RecurringPreview.of(paused, occurrences: RecurringFixture.core).get()
        #expect(preview.lines.first == .init(date: Self.today, isDue: false))
        #expect(!Self.inputs(template: RecurringFixture.template(archived: true)).running)

        let new = Self.inputs(schedule, template: nil)
        #expect(new.running)
        #expect(new.savedDue == nil)
        // July to October are due for a template that runs from July.
        #expect(try RecurringPreview.of(new, occurrences: RecurringFixture.core).get().lines.allSatisfy(\.isDue))
    }

    @Test("A schedule the core refuses is a failure, not an empty list")
    func refused() {
        let inputs = Self.inputs(RecurringFixture.schedule(.monthly(day: 32)), template: nil)
        #expect(throws: DomainError.self) { try RecurringPreview.of(inputs, occurrences: RecurringFixture.core).get() }
    }
}
