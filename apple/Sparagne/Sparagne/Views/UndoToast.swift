import SwiftUI

/// The 5-second undo window after a void, which the window calls a delete:
/// the row is already gone, a bar counts down, and Undo ("Ripristina")
/// puts it back without ever touching the core (docs/v2/DISTILLATO_V1.md
/// §2.4).
///
/// ⌘Z does the same through Edit ▸ Undo, where the store registers the void
/// (`LedgerHistory.recordPendingVoid`). The button has no shortcut of its own:
/// one would take ⌘Z away from a cell being typed into while the toast is up.
///
/// Drawn with the ledger's own chrome (`Panel`'s rounded, hairline-bordered
/// card) rather than the system material, so it reads as part of the finance
/// terminal instead of a generic macOS alert (`docs/v2/UI.md` §5).
struct UndoToast: View {
    let pending: PendingUndo
    let undo: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 14) {
                Text(CountText.voided(pending.ids.count))
                    .font(Face.row)
                    .foregroundStyle(Ink.text)
                Spacer(minLength: 0)
                Button(String(localized: "Undo"), action: undo)
                    .buttonStyle(.plain)
                    .font(Face.row)
                    .foregroundStyle(Ink.accent)
            }
            TimelineView(.animation) { context in
                MeterBar(fraction: 1 - pending.progress(at: context.date), tint: Ink.accent, height: 2)
            }
            .frame(width: 200)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .background(Ink.card, in: RoundedRectangle(cornerRadius: Metrics.cardRadius))
        .overlay(RoundedRectangle(cornerRadius: Metrics.cardRadius).strokeBorder(Ink.line, lineWidth: 1))
        .shadow(radius: 8, y: 2)
    }
}
