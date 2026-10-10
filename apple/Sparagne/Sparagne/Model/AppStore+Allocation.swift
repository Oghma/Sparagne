import Foundation
import SparagneCore

/// The Riparto tab's commands and the lists it loads itself. The plan, the
/// period due and the base come with every `reload()`; the decided periods
/// (`allocationRuns`) are loaded by the tab and again after every decision.
///
/// Every command goes through `apply`, one command as a batch of one: the
/// read-only refusal, the alert, "saved at" and the reload are the same as
/// any other write's. None of them is a step of Edit ▸ Undo: Distribuisci is
/// undone by its own toast and by the history's Annulla, as a command.
extension AppStore {
    /// How many decided periods the history lists.
    static let allocationRunsLimit: UInt32 = 24
    /// How long the toast after Distribuisci offers Annulla.
    static let allocationUndoWindow: Duration = .seconds(8)

    /// The count on the Riparto tab: one while a period waits for a decision,
    /// whatever the periods it would close with it. None on a vault the
    /// account only reads, where nothing can be decided.
    var actionableAllocationCount: Int {
        isReadOnly || pendingAllocation == nil ? 0 : 1
    }

    /// The decided periods of the plan, most recent first, into
    /// `allocationRuns`. Silent on failure like the other lists a tab loads
    /// for itself: an empty history is better than an alert over a tab only
    /// read.
    func loadAllocationRuns() async {
        guard let vault = currentVault, allocationPlan != nil else {
            allocationRuns = []
            return
        }
        guard let runs = try? await core.allocationRuns(vaultId: vault.id, limit: Self.allocationRunsLimit),
              currentVault?.id == vault.id
        else { return }
        allocationRuns = runs
    }

    /// `lines` worked out on `total` against the envelopes as they are now,
    /// or `nil` when the core refuses: the table then shows the lines without
    /// what they would get.
    func previewAllocation(lines: [AllocationLine], total: Int64) async -> AllocationPreview? {
        guard let vault = currentVault else { return nil }
        return try? await core.previewAllocation(vaultId: vault.id, lines: lines, total: total)
    }

    // MARK: - The plan

    /// Every line of the plan, in its order: the first commit makes the plan
    /// on `schedule`, the next ones replace its list. Says whether it went
    /// through; a refusal is already on the alert.
    @discardableResult
    func saveAllocationLines(_ lines: [AllocationLine], schedule: Schedule) async -> Bool {
        guard let vault = currentVault, !lines.isEmpty else { return false }
        guard let plan = allocationPlan else {
            return await apply([.createAllocationPlan(schedule: schedule, lines: lines)], in: vault.id)
        }
        guard lines != plan.lines else { return true }
        return await updateAllocationPlan(AllocationPlanPatch(lines: lines))
    }

    /// Quando and Inizio, saved from the inspector.
    func setAllocationSchedule(_ schedule: Schedule) async {
        guard let plan = allocationPlan, schedule != plan.schedule else { return }
        await updateAllocationPlan(AllocationPlanPatch(schedule: schedule))
    }

    /// The Attivo switch. A paused plan has no period due.
    func setAllocationEnabled(_ enabled: Bool) async {
        guard let plan = allocationPlan, enabled != plan.enabled else { return }
        await updateAllocationPlan(AllocationPlanPatch(enabled: enabled))
    }

    @discardableResult
    private func updateAllocationPlan(_ patch: AllocationPlanPatch) async -> Bool {
        guard let vault = currentVault, let plan = allocationPlan, !patch.isEmpty else { return false }
        return await apply([.updateAllocationPlan(planId: plan.id, patch: patch)], in: vault.id)
    }

    // MARK: - Deciding the period

    /// Distribuisci: `lines`, the plan as the tab shows it, worked out on
    /// `total`, the total on screen, and the amounts they give sent as they
    /// are, so a replay moves the same money whatever the plan says by then.
    /// The lines are the tab's rather than the saved plan's: an edit still on
    /// its way would otherwise be shown and not sent. Worked out again here
    /// rather than taken from the card, which may still be showing the answer
    /// for the total before the last keystroke. `nil` is the saved plan.
    ///
    /// The transfers are dated on the period's day at the time of the click,
    /// as a recorded recurring period is. A period where no line gets
    /// anything is skipped (`AllocationDecision`). A refusal reloads, so the
    /// card shows what the core sees now; one that goes through leaves the
    /// toast that undoes it.
    func executeAllocation(total: Int64, lines: [AllocationLine]? = nil, now: Date = Date()) async {
        guard let vault = currentVault, let plan = allocationPlan,
              let pending = pendingAllocation, pending.planId == plan.id
        else { return }
        let preview: AllocationPreview
        do {
            preview = try await core.previewAllocation(vaultId: vault.id, lines: lines ?? plan.lines, total: total)
        } catch {
            report(error)
            return
        }
        let command: Command
        switch AllocationDecision(preview: preview) {
        case .skip:
            command = .skipAllocation(planId: plan.id, periodDate: pending.periodDate)
        case .execute(let moves):
            command = .executeAllocation(
                planId: plan.id,
                periodDate: pending.periodDate,
                occurredAt: Self.stamp(day: CoreDate.localDay(pending.periodDate) ?? now, likeTimeOf: now),
                total: total,
                moves: moves,
                note: nil
            )
        }
        let applied = await apply([command], in: vault.id)
        if applied, case .executeAllocation = command {
            allocationUndo = AllocationUndo(
                vaultId: vault.id,
                planId: plan.id,
                periodDate: pending.periodDate,
                distributed: preview.distributed,
                startedAt: now,
                duration: Self.allocationUndoWindow
            )
        } else if !applied, !isReadOnly {
            explainRefusal(AllocationRefusal.decision)
            await reload()
        }
        await loadAllocationRuns()
    }

    /// Salta: the period is decided with nothing moved, and its incomes stay
    /// in Unallocated, out of the next period's total. A refusal reloads, as
    /// Distribuisci's does.
    func skipAllocation() async {
        guard let vault = currentVault, let pending = pendingAllocation else { return }
        let applied = await apply([.skipAllocation(planId: pending.planId, periodDate: pending.periodDate)], in: vault.id)
        if !applied, !isReadOnly {
            explainRefusal(AllocationRefusal.decision)
            await reload()
        }
        await loadAllocationRuns()
    }

    /// Annulla, from the toast or the history: the latest decided period goes
    /// back to waiting, its transfers deleted and its incomes back in the
    /// total. The core refuses any period but the latest.
    func reopenAllocation(periodDate: NaiveDate) async {
        guard let vault = currentVault, let plan = allocationPlan else { return }
        await reopenAllocation(planId: plan.id, periodDate: periodDate, in: vault.id)
    }

    /// The toast's Annulla, in the vault the allocation was made in.
    func undoAllocation() async {
        guard let undo = allocationUndo else { return }
        await reopenAllocation(planId: undo.planId, periodDate: undo.periodDate, in: undo.vaultId)
    }

    /// The toast's window elapsed, or it was closed.
    func dismissAllocationUndo(_ id: UUID) {
        if allocationUndo?.id == id { allocationUndo = nil }
    }

    /// One reopen at a time (`allocationReopening`): the toast and the
    /// history can both offer the same period, and a second click would only
    /// be refused. A refusal (someone undid it, or decided a later period,
    /// in the meantime) takes the toast away, since what it would undo is no
    /// longer there, and reloads to show what is.
    private func reopenAllocation(planId: Uuid, periodDate: NaiveDate, in vaultId: Uuid) async {
        guard !allocationReopening else { return }
        allocationReopening = true
        defer { allocationReopening = false }
        let applied = await apply([.reopenAllocation(planId: planId, periodDate: periodDate)], in: vaultId)
        if applied {
            if allocationUndo?.planId == planId, allocationUndo?.periodDate == periodDate { allocationUndo = nil }
        } else if !isReadOnly {
            explainRefusal(AllocationRefusal.reopen)
            allocationUndo = nil
            if vaultId == currentVault?.id { await reload() }
        }
        await loadAllocationRuns()
    }

    /// Puts the household's words on the refusal `apply` has just shown,
    /// when `explain` has some for it.
    private func explainRefusal(_ explain: (AppError) -> AppError?) {
        guard let shown = presentedError, let better = explain(shown) else { return }
        presentedError = better
    }
}
