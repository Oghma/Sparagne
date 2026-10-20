import SwiftUI

/// The undo window after a void, which the window calls a delete: Undo puts the rows back.
struct UndoToast: View {
    let pending: PendingUndo
    let undo: () -> Void

    /// No shortcut on Undo: ⌘Z does the same through Edit ▸ Undo
    /// (`LedgerHistory.recordPendingVoid`), and one here would take ⌘Z from
    /// a cell being typed into while the toast is up.
    var body: some View {
        CountdownToast(
            text: CountText.voided(pending.ids.count),
            undoTitle: String(localized: "Undo"),
            progress: pending.progress(at:),
            undo: undo
        )
    }
}

/// A toast that offers Undo while a bar counts its window down, on a card
/// rather than the system material, so it reads as part of the window.
struct CountdownToast: View {
    let text: String
    let undoTitle: String
    var disabled = false
    /// How much of the window has gone, 0...1, at a moment.
    let progress: (Date) -> Double
    let undo: () -> Void

    var body: some View {
        Panel(padding: 0) {
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 14) {
                    Text(text)
                        .font(Face.row)
                        .foregroundStyle(Ink.text)
                    Spacer(minLength: 0)
                    Button(undoTitle, action: undo)
                        .buttonStyle(.plain)
                        .font(Face.row)
                        .foregroundStyle(Ink.accent)
                        .disabled(disabled)
                }
                TimelineView(.animation) { context in
                    MeterBar(fraction: 1 - progress(context.date), tint: Ink.accent, height: 2)
                }
                .frame(width: 200)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
        }
        .shadow(radius: 8, y: 2)
    }
}
