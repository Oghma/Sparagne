import Foundation
import SparagneCore

/// What the Ricorrenze tab reads off the store and the one list it loads
/// itself, the agenda of the next 30 days (`docs/v2/UI.md` §2.5).
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

    /// The Attiva switch: pauses or resumes a template, sending `enabled`
    /// and nothing else. A paused template is never due.
    func setRecurringEnabled(_ templateId: Uuid, _ enabled: Bool) async {
        await updateRecurring(templateId, patch: RecurringPatch(enabled: enabled))
    }
}
