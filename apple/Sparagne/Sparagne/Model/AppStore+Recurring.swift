import Foundation
import SparagneCore

/// What the Ricorrenze tab reads off the store and the one list it loads
/// itself, the agenda of the next 30 days.
extension AppStore {
    /// How far ahead "Prossimi 30 giorni" looks.
    static let agendaDays = 30

    /// Every period of the running templates from the day after `today` to
    /// 30 days on, into `upcomingRecurring` (`RecurringAgenda`). Reads the
    /// templates already loaded (`loadRecurringTemplates`): the tab calls it
    /// again whenever they change, whenever the due list does, and when the
    /// day turns.
    ///
    /// `scheduleOccurrences` is pure arithmetic in the core, with no database
    /// behind it, so it runs here rather than on `CoreActor`.
    func loadUpcomingRecurring(today: NaiveDate = CoreDate.day(Date())) async {
        guard currentVault != nil else {
            upcomingRecurring = []
            return
        }
        upcomingRecurring = RecurringAgenda.build(
            templates: recurringTemplates,
            today: today,
            days: Self.agendaDays,
            occurrences: CoreSchedule.occurrences
        ).tiles
    }

    /// Prossima of every template, by id (`RecurringNext.of`), for the
    /// templates table. The tab works it out in the same task as the agenda,
    /// from the same inputs, and keeps it: each answer is a call into the
    /// core, and the table draws every row again on each hover.
    func nextRecurring(today: NaiveDate) -> [Uuid: RecurringNext] {
        let due = Dictionary(
            pendingRecurringItems.map { ($0.template.id, $0.due) },
            uniquingKeysWith: +
        )
        let next = recurringTemplates.map { template in
            (
                template.id,
                RecurringNext.of(
                    template,
                    due: due[template.id] ?? [],
                    today: today,
                    occurrences: CoreSchedule.occurrences
                )
            )
        }
        return Dictionary(next, uniquingKeysWith: { first, _ in first })
    }

    /// The table's order: the templates that run or are paused, as the core
    /// lists them, then the archived ones at the bottom, still there to be
    /// restored.
    var recurringTemplatesInTableOrder: [RecurringView] {
        recurringTemplates.filter { !$0.archived } + recurringTemplates.filter(\.archived)
    }

    /// The periods of one template still waiting for a decision, oldest first.
    func dueDates(of templateId: Uuid) -> [NaiveDate] {
        pendingRecurringItems.first { $0.template.id == templateId }?.due.sorted() ?? []
    }

    /// The periods of one template already recorded or skipped: the ones the
    /// inspector leaves out when it works out the due periods of a schedule
    /// being edited. Empty without a vault, or when the core refuses.
    func handledPeriods(of templateId: Uuid) async -> Set<NaiveDate> {
        guard let vault = currentVault else { return [] }
        let runs = (try? await core.recurringRuns(vaultId: vault.id, recurringId: templateId)) ?? []
        return Set(runs.map(\.periodDate))
    }

    /// The Attiva switch: pauses or resumes a template, sending `enabled`
    /// and nothing else. A paused template is never due.
    func setRecurringEnabled(_ templateId: Uuid, _ enabled: Bool) async {
        await updateRecurring(templateId, patch: RecurringPatch(enabled: enabled))
    }
}
