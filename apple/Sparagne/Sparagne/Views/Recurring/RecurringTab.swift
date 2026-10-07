import SwiftUI

/// The Ricorrenze tab (`docs/v2/UI.md` §2.5): what is waiting to be
/// confirmed, and the way to the templates. For now it opens the two sheets
/// that already do the work, the due periods (`DueRecurringSheet`) and the
/// templates (`RecurringPanel`).
struct RecurringTab: View {
    let store: AppStore
    let engine: SyncEngine?
    /// Requests one of `MainWindow`'s sheets.
    let present: (MainWindow.SheetKind) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Panel {
                VStack(alignment: .leading, spacing: 10) {
                    SectionLabel(text: String(localized: "To confirm"))
                    HStack(spacing: 12) {
                        Text(dueText)
                            .font(Face.ui(13))
                            .foregroundStyle(store.dueRecurringCount > 0 ? Ink.text : Ink.text3)
                        Spacer(minLength: 12)
                        Button(String(localized: "Review")) { present(.dueRecurring) }
                            .buttonStyle(.chrome(.primary))
                            .disabled(store.dueRecurringCount == 0)
                    }
                }
            }
            .frame(maxWidth: 560)

            Button(String(localized: "Manage Templates\u{2026}")) { present(.recurring) }
                .buttonStyle(.chrome())

            Spacer(minLength: 0)
        }
        .padding(Metrics.gutter)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        // The templates feed the status line's counts; the panel reloads
        // them on its own when it opens.
        .task(id: store.currentVault?.id) { await store.loadRecurringTemplates() }
    }

    private var dueText: String {
        let count = store.dueRecurringCount
        return count > 0 ? CountText.recurringDue(count) : String(localized: "Nothing is due")
    }
}
