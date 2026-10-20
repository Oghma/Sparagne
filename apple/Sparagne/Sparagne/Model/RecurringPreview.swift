import Foundation
import SparagneCore

/// The Ricorrenze inspector's next four dates, and how many due periods did
/// not fit among them.
struct RecurringPreview {
    let lines: [RecurringNext.Preview]
    let moreDue: Int

    /// What the dates are worked out from, as one value for `.onChange`: the
    /// schedule on screen, saved or not; the saved due periods while it is
    /// the saved one, `nil` otherwise; the periods already handled; and
    /// whether the template runs, which makes its past periods due and
    /// today, when not among them, decided (`RecurringNext.of`).
    struct Inputs: Equatable {
        let schedule: Schedule
        let savedDue: [NaiveDate]?
        let handled: Set<NaiveDate>
        let today: NaiveDate
        let running: Bool

        /// The inputs of `template`'s inspector, or of a new template's,
        /// which is created running. `savedDue` is read only while `schedule`
        /// is the saved one.
        init(
            schedule: Schedule,
            template: RecurringView?,
            savedDue: (Uuid) -> [NaiveDate],
            handled: Set<NaiveDate>,
            today: NaiveDate
        ) {
            self.schedule = schedule
            self.savedDue = template.flatMap { $0.schedule == schedule ? savedDue($0.id) : nil }
            self.handled = handled
            self.today = today
            running = template.map { $0.enabled && !$0.archived } ?? true
        }
    }

    /// The next four dates of `inputs`. A schedule being edited has its due
    /// periods worked out from it, so a start moved back shows the periods
    /// since then before they are saved as due.
    static func of(
        _ inputs: Inputs,
        occurrences: ScheduleOccurrences
    ) -> Result<RecurringPreview, Error> {
        let from = inputs.running ? (NaiveDay.adding(1, to: inputs.today) ?? inputs.today) : inputs.today
        return Result {
            let due = try inputs.savedDue ?? (inputs.running
                ? RecurringNext.due(
                    schedule: inputs.schedule,
                    today: inputs.today,
                    handled: inputs.handled,
                    occurrences: occurrences
                )
                : [])
            let lines = try RecurringNext.preview(
                schedule: inputs.schedule,
                due: due,
                from: from,
                count: 4,
                occurrences: occurrences
            )
            return RecurringPreview(lines: lines, moreDue: due.count - lines.filter(\.isDue).count)
        }
    }
}
