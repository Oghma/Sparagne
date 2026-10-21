import Foundation
import SparagneCore

/// Changing a recurring template's kind (`RecurringKindChange`).
extension AppStore {
    /// Why the template with `templateId` cannot change kind now; `nil` when
    /// it can.
    func recurringKindRefusal(_ templateId: Uuid) async -> RecurringKindChange.Refusal? {
        guard case .failure(let refusal) = await recurringKindStart(templateId) else { return nil }
        return refusal
    }

    /// The day the copy of the template with `templateId` would start on, or
    /// why it cannot change kind now: the inspector's menu item and its
    /// question. `nil` when the core could not say; the change itself puts
    /// that error up.
    func recurringKindStart(_ templateId: Uuid) async -> Result<NaiveDate, RecurringKindChange.Refusal>? {
        try? await recurringKindPlan(templateId).map(\.start)
    }

    /// Archives the template and creates its copy of the other kind, paused
    /// if it was, in one batch. Returns the copy's id; `nil` when refused,
    /// with the alert up.
    ///
    /// The copy is minted first: `CreateRecurring` has no `enabled`, and the
    /// pause that follows it in the batch names the id the core gives the
    /// template, the command's own. Recurring templates have no undo, so
    /// neither has this.
    @discardableResult
    func changeRecurringKind(_ templateId: Uuid) async -> Uuid? {
        guard let vault = currentVault, !refusedAsReadOnly() else { return nil }
        let plan: RecurringKindPlan
        do {
            switch try await recurringKindPlan(templateId) {
            case .success(let found):
                plan = found
            case .failure(let refusal):
                presentedError = AppError(
                    code: "invalid_command",
                    message: refusal.reason,
                    headline: String(localized: "The template keeps its kind")
                )
                return nil
            }
        } catch {
            report(error)
            return nil
        }
        let old = plan.template
        let copy = await core.envelope(
            vaultId: vault.id,
            RecurringKindChange.copy(of: old, startingOn: plan.start, owner: explicitPerson(old.owner))
        )
        var envelopes = [await core.envelope(vaultId: vault.id, .archiveRecurring(recurringId: old.id)), copy]
        if !old.enabled {
            envelopes.append(await core.envelope(
                vaultId: vault.id,
                .updateRecurring(recurringId: copy.id, patch: RecurringPatch(enabled: false))
            ))
        }
        return await apply(envelopes: envelopes, in: vault.id) ? copy.id : nil
    }

    /// What a kind change of the template with `templateId` would write from:
    /// the template and its copy's first day, or the refusal. Throws when
    /// the core cannot list the periods already decided: taking none would
    /// start the copy where the template did, and its recorded periods
    /// would come due again.
    ///
    /// Only a running template has periods waiting. A paused one's past
    /// periods are not on the due list until it runs again: those after the
    /// last one decided go to the copy, paused too, and come due there when
    /// it is resumed, as they would have on the template. One before it
    /// would be lost, since the copy starts after that, so it holds the
    /// change back until the template is resumed and it is decided.
    private func recurringKindPlan(
        _ templateId: Uuid,
        today: NaiveDate = CoreDate.day(Date())
    ) async throws -> Result<RecurringKindPlan, RecurringKindChange.Refusal> {
        guard let vault = currentVault, canWrite else { return .failure(.readOnly) }
        // Gone from the list is another vault's template, or one no device
        // will see again: nothing to change either way.
        guard let template = recurringTemplates.first(where: { $0.id == templateId }), !template.archived else {
            return .failure(.archived)
        }
        if ownerHasLeft(template) { return .failure(.ownerLeft) }
        let runs = try await core.recurringRuns(vaultId: vault.id, recurringId: templateId)
        let decided = Set(runs.map(\.periodDate))
        let undecided = try RecurringNext.due(
            schedule: template.schedule,
            today: today,
            handled: decided,
            occurrences: CoreSchedule.occurrences
        )
        if template.enabled, !undecided.isEmpty { return .failure(.pendingPeriods) }
        if !template.enabled, let latest = decided.max(), undecided.contains(where: { $0 < latest }) {
            return .failure(.periodsBehindPaused)
        }
        guard let start = RecurringKindChange.start(of: template.schedule, decided: decided) else {
            return .failure(.noPeriodsLeft)
        }
        return .success(RecurringKindPlan(template: template, start: start))
    }
}

/// A template whose kind may change, and the day its copy starts on.
private struct RecurringKindPlan {
    let template: RecurringView
    let start: NaiveDate
}
