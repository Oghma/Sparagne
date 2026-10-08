import SwiftUI
import SparagneCore

/// The modal sheets of the CATEGORIE table.

/// The subject of the merge sheet. `.sheet(item:)` needs an `Identifiable`
/// and the core's `CategoryView` is not one.
struct MergeSubject: Identifiable {
    let category: CategoryView
    var id: Uuid { category.id }
}

/// Merges `source` into a chosen target, previewing conflicts first and
/// disabling Merge when the preview is not `ok`
/// (`Core::preview_merge`).
///
/// Drawn with the form kit like the window's other sheets, so a dialog over
/// the table reads as part of the same surface.
struct MergeCategorySheet: View {
    let store: AppStore
    let source: CategoryView
    let targets: [CategoryView]

    @Environment(\.dismiss) private var dismiss
    @State private var targetId: Uuid?
    @State private var preview: MergePreview?

    var body: some View {
        FormSheet(String(localized: "Merge \u{201C}\(source.name)\u{201D} into"), width: 420) {
            FormGroup {
                FormRow(String(localized: "Target category")) {
                    FormPicker(
                        label: String(localized: "Target category"),
                        selection: $targetId,
                        options: [nil] + targets.map { Optional($0.id) },
                        title: targetName
                    )
                }
                .task(id: targetId) {
                    guard let targetId else {
                        preview = nil
                        return
                    }
                    preview = await store.previewCategoryMerge(sourceId: source.id, targetId: targetId)
                }
                if let preview, !preview.conflicts.isEmpty {
                    ForEach(Array(preview.conflicts.enumerated()), id: \.offset) { _, conflict in
                        FormNote(conflictText(conflict), tone: .warning)
                    }
                }
            }
        } footer: {
            FormCancelButton { dismiss() }
            FormPrimaryButton(title: String(localized: "Merge")) {
                guard let targetId else { return }
                Task { await store.mergeCategory(sourceId: source.id, targetId: targetId) }
                dismiss()
            }
            .disabled(targetId == nil || preview?.ok == false)
        }
    }

    /// "Choose…" until a target is picked.
    private func targetName(_ id: Uuid?) -> String {
        guard let id else { return String(localized: "Choose…") }
        return targets.first { $0.id == id }?.name ?? String(localized: "Choose…")
    }

    private func conflictText(_ conflict: MergeConflict) -> String {
        switch conflict.kind {
        case .sameCategory: String(localized: "Source and target are the same category.")
        case .sourceSystem: String(localized: "System categories cannot be merged.")
        case .targetArchived: String(localized: "\u{201C}\(conflict.value)\u{201D} is archived.")
        }
    }
}
