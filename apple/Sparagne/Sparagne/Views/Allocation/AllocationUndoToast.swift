import SwiftUI

/// The toast after Distribuisci: "Distribuiti 4.250,00 · Annulla", with the
/// bar that counts its window down. Unlike the void's `UndoToast`, nothing is
/// held back: the transfers are written already, and Annulla sends
/// `ReopenAllocation`, as the history's Annulla does. The window elapsing
/// only closes the toast.
///
/// The same chrome as `UndoToast`, so the two read as one kind of thing.
struct AllocationUndoToast: View {
    let undo: AllocationUndo
    let store: AppStore

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 14) {
                Text(String(localized: "Distributed \(LedgerMoney.bare(undo.distributed))"))
                    .font(Face.row)
                    .foregroundStyle(Ink.text)
                Spacer(minLength: 0)
                Button(String(localized: "allocation.undo", defaultValue: "Undo")) {
                    Task { await store.undoAllocation() }
                }
                .buttonStyle(.plain)
                .font(Face.row)
                .foregroundStyle(Ink.accent)
                // One reopen at a time, whichever Annulla started it.
                .disabled(store.allocationReopening)
            }
            TimelineView(.animation) { context in
                MeterBar(fraction: 1 - undo.progress(at: context.date), tint: Ink.accent, height: 2)
            }
            .frame(width: 200)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .background(Ink.card, in: RoundedRectangle(cornerRadius: Metrics.cardRadius))
        .overlay(RoundedRectangle(cornerRadius: Metrics.cardRadius).strokeBorder(Ink.line, lineWidth: 1))
        .shadow(radius: 8, y: 2)
        .task(id: undo.id) {
            // What is left of the window: a toast drawn again, after a trip
            // to another vault, does not start it over.
            let left = max(undo.deadline.timeIntervalSinceNow, 0)
            guard (try? await Task.sleep(for: .seconds(left))) != nil else { return }
            store.dismissAllocationUndo(undo.id)
        }
    }
}
