import SwiftUI
import SparagneCore

/// The modal sheets of the CATEGORIE table (`docs/v2/UI.md` §2.3). Moved here
/// from the old Categories window, which the SETUP view replaces.

/// The subject of the merge sheet. `.sheet(item:)` needs an `Identifiable`
/// and the core's `CategoryView` is not one.
struct MergeSubject: Identifiable {
    let category: CategoryView
    var id: Uuid { category.id }
}

/// Merges `source` into a chosen target, previewing conflicts first and
/// disabling Merge when the preview is not `ok`
/// (`Core::preview_merge`, `docs/v2/ARCH.md` §4).
///
/// System styling on purpose: a modal sheet is a dialog, not part of the
/// spreadsheet surface the table draws.
struct MergeCategorySheet: View {
    let store: AppStore
    let source: CategoryView
    let targets: [CategoryView]

    @Environment(\.dismiss) private var dismiss
    @State private var targetId: Uuid?
    @State private var preview: MergePreview?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(String(localized: "Merge \u{201C}\(source.name)\u{201D} into"))
                .font(.headline)
            Picker(String(localized: "Target category"), selection: $targetId) {
                Text(String(localized: "Choose…")).tag(Optional<Uuid>.none)
                ForEach(targets, id: \.id) { target in
                    Text(target.name).tag(Optional(target.id))
                }
            }
            .task(id: targetId) {
                guard let targetId else {
                    preview = nil
                    return
                }
                preview = await store.previewCategoryMerge(sourceId: source.id, targetId: targetId)
            }
            if let preview, !preview.conflicts.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(Array(preview.conflicts.enumerated()), id: \.offset) { _, conflict in
                        Label(conflictText(conflict), systemImage: "exclamationmark.triangle")
                            .foregroundStyle(Ink.accent)
                            .font(.caption)
                    }
                }
            }
            HStack {
                Spacer()
                Button(String(localized: "Cancel"), role: .cancel) { dismiss() }
                Button(String(localized: "Merge")) {
                    guard let targetId else { return }
                    Task { await store.mergeCategory(sourceId: source.id, targetId: targetId) }
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(targetId == nil || preview?.ok == false)
            }
        }
        .padding(20)
        .frame(width: 380)
    }

    private func conflictText(_ conflict: MergeConflict) -> String {
        switch conflict.kind {
        case .sameCategory: String(localized: "Source and target are the same category.")
        case .sourceSystem: String(localized: "System categories cannot be merged.")
        case .targetArchived: String(localized: "\u{201C}\(conflict.value)\u{201D} is archived.")
        }
    }
}
