import SwiftUI

/// The 5-second undo window after a void: the row is already gone, a bar
/// counts down, and Undo (or ⌘Z) puts it back without ever touching the core
/// (docs/v2/DISTILLATO_V1.md §2.4).
struct UndoToast: View {
    let pending: PendingUndo
    let undo: () -> Void

    var body: some View {
        VStack(spacing: 6) {
            HStack(spacing: 12) {
                Text(String(localized: "Transaction voided"))
                Button(String(localized: "Undo"), action: undo)
                    .keyboardShortcut("z", modifiers: .command)
            }
            TimelineView(.animation) { context in
                ProgressView(value: 1 - pending.progress(at: context.date))
                    .progressViewStyle(.linear)
                    .frame(width: 200)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
        .shadow(radius: 8, y: 2)
    }
}
