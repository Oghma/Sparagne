import SwiftUI

/// The toast after Distribuisci, whose Annulla reopens the period: its transfers are already written.
struct AllocationUndoToast: View {
    let undo: AllocationUndo
    let store: AppStore

    var body: some View {
        CountdownToast(
            text: String(localized: "Distributed \(LedgerMoney.bare(undo.distributed))"),
            undoTitle: String(localized: "allocation.undo", defaultValue: "Undo"),
            // One reopen at a time, whichever Annulla started it.
            disabled: store.allocationReopening,
            progress: undo.progress(at:)
        ) {
            Task { await store.undoAllocation() }
        }
        .task(id: undo.id) {
            // What is left of the window: a toast drawn again, after a trip
            // to another vault, does not start it over.
            let left = max(undo.deadline.timeIntervalSinceNow, 0)
            guard (try? await Task.sleep(for: .seconds(left))) != nil else { return }
            store.dismissAllocationUndo(undo.id)
        }
    }
}
