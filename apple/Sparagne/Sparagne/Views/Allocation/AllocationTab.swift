import SwiftUI
import SparagneCore

/// The Riparto tab: the allocation plan, the period waiting to be shared out
/// of Unallocated and the periods already decided. On the left the card of
/// the period due ("Da distribuire"), the plan as an editable table, whose
/// columns are the preview of what each line gets, and the history; on the
/// right the inspector of the plan's schedule.
///
/// Nothing is ever moved by itself: the card is where a period is shared out
/// (Distribuisci) or skipped (Salta).
struct AllocationTab: View {
    let store: AppStore

    /// The day everything on the tab counts from, taken again when the
    /// calendar day turns.
    @State private var today = CoreDate.day(Date())
    /// The card's total as typed, seeded from the base and seeded again
    /// whenever the base or the period change.
    @State private var totalText = ""
    /// The plan worked out on the total (`previewAllocation`), asked of the
    /// core when what it depends on changes (`PreviewInputs`) rather than on
    /// every render. `nil` until the first answer, and while the core
    /// refuses.
    @State private var preview: AllocationPreview?
    /// The lines last sent, until the save comes back: an edit made in the
    /// meantime builds on them rather than on the plan as it was before.
    @State private var sentLines: [AllocationLine]?
    @State private var sends = 0
    /// The inspector's Quando and Inizio, which a new plan is also created
    /// with.
    @State private var schedule = AllocationScheduleDraft(today: CoreDate.day(Date()))
    /// A row of the table is open: ↩ and esc are its, not the inspector's
    /// Salva and Annulla.
    @State private var rowOpen = false

    var body: some View {
        HStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    AllocationDueCard(
                        store: store,
                        today: today,
                        totalText: $totalText,
                        total: total,
                        preview: preview,
                        lines: lines,
                        saving: sentLines != nil
                    )
                    AllocationPlanTable(
                        store: store,
                        lines: lines,
                        preview: preview,
                        due: due != nil,
                        rowOpen: $rowOpen,
                        save: save
                    )
                    if store.allocationPlan != nil {
                        AllocationHistory(store: store)
                    }
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .topLeading)
            }
            Rectangle().fill(Ink.line).frame(width: 1)
            AllocationInspector(
                store: store,
                draft: $schedule,
                lines: lines,
                preview: preview,
                due: due != nil,
                today: today,
                shortcuts: !rowOpen
            )
            .frame(width: RecurringTab.inspectorWidth)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task(id: previewInputs) {
            let inputs = previewInputs
            let answer = inputs.lines.isEmpty
                ? nil
                : await store.previewAllocation(lines: inputs.lines, total: inputs.total)
            guard !Task.isCancelled else { return }
            preview = answer
        }
        // The history follows every decision, made here or by a sync: each
        // one moves the period due and the base.
        .task(id: RunsKey(
            vaultId: store.currentVault?.id,
            planId: store.allocationPlan?.id,
            pending: store.pendingAllocation,
            base: store.allocationBase.total,
            snapshot: store.snapshot
        )) {
            await store.loadAllocationRuns()
        }
        // A new period, or incomes that came in: the total starts again from
        // what the base says.
        .onChange(of: TotalSeed(pending: store.pendingAllocation, base: store.allocationBase.total), initial: true) { _, _ in
            totalText = LedgerMoney.bare(store.allocationBase.total)
        }
        // The saved schedule moved (a save, a sync, another vault): an
        // untouched draft follows it, an edited one keeps the user's edits.
        .onChange(of: store.allocationPlan?.schedule, initial: true) { old, new in
            let saved = old.map(AllocationScheduleDraft.init(schedule:)) ?? AllocationScheduleDraft(today: today)
            guard schedule == saved || old == new else { return }
            schedule = new.map(AllocationScheduleDraft.init(schedule:)) ?? AllocationScheduleDraft(today: today)
        }
        .onChange(of: store.currentVault?.id) { _, _ in
            sentLines = nil
            schedule = store.allocationPlan.map { AllocationScheduleDraft(schedule: $0.schedule) }
                ?? AllocationScheduleDraft(today: today)
        }
        // Midnight: the calendar's own message.
        .task {
            for await _ in NotificationCenter.default.messages(of: Calendar.self, for: .calendarDayChanged) {
                today = CoreDate.day(Date())
            }
        }
    }

    // MARK: - What the tab works on

    /// The plan's lines in their order, or the ones just sent.
    private var lines: [AllocationLine] {
        sentLines ?? store.allocationPlan?.lines ?? []
    }

    /// The period waiting for a decision, when it is the plan on screen's.
    private var due: PendingAllocation? {
        guard let pending = store.pendingAllocation, pending.planId == store.allocationPlan?.id else { return nil }
        return pending
    }

    /// The total the preview is worked out on: the card's while a period is
    /// due, `nil` while what is typed there is not an amount; the base
    /// otherwise, what would be shared out if the period were today.
    private var total: Int64? {
        guard due != nil else { return store.allocationBase.total }
        guard let typed = AllocationMoney.amount(totalText, currency: store.currency), typed >= 0 else { return nil }
        return typed
    }

    private var previewInputs: PreviewInputs {
        PreviewInputs(
            vaultId: store.currentVault?.id,
            lines: lines,
            total: total ?? store.allocationBase.total,
            flows: store.snapshot?.flows ?? []
        )
    }

    /// Sends `new` as the plan's whole list, which the table shows from now
    /// on; the first one creates the plan on the inspector's schedule, and is
    /// refused while that one is not a schedule. The task says whether it
    /// went through.
    private func save(_ new: [AllocationLine]) -> Task<Bool, Never> {
        if store.allocationPlan == nil, !schedule.isValid {
            store.presentedError = AppError(
                code: "invalid_command",
                message: String(localized: "Correct When and Start on the right, then press \u{21A9} on the line again."),
                headline: String(localized: "These settings do not make a schedule")
            )
            return Task { false }
        }
        sentLines = new
        sends += 1
        let send = sends
        let start = schedule.schedule
        return Task {
            let saved = await store.saveAllocationLines(new, schedule: start)
            if send == sends { sentLines = nil }
            return saved
        }
    }
}

/// What the preview is worked out from, as one value for `.task(id:)`. The
/// envelopes are in it for their balances: a sync that moves one changes
/// what the lines can get.
private struct PreviewInputs: Equatable {
    let vaultId: Uuid?
    let lines: [AllocationLine]
    let total: Int64
    let flows: [FlowView]
}

/// When the history is loaded again: whenever the vault does, since a
/// decision, a reopen or a transfer of a run voided by a sync all come with a
/// reload.
private struct RunsKey: Equatable {
    let vaultId: Uuid?
    let planId: Uuid?
    let pending: PendingAllocation?
    let base: Int64
    let snapshot: VaultSnapshot?
}

/// When the card's total starts again from the base.
private struct TotalSeed: Equatable {
    let pending: PendingAllocation?
    let base: Int64
}
